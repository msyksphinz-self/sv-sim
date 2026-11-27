//! 層 3: owned の値型 [`LogicVec`] (4 値) と [`BitVec`] (2 値) (plan.md §3.3)。
//!
//! - 幅 ≤ 64 はインラインで持ち、ヒープ確保をしない。幅 > 64 はプレーン分離配置
//!   (`[a.., b..]`、plan.md §3.2) の `Box<[u64]>` で持つ。
//! - 表現は幅だけで決まる (同じ幅なら常に同じ表現)。そのため派生した `PartialEq` / `Hash` は
//!   表現の違いに左右されず、「幅と全ビットが同一」(`===` の意味、§11.4.5) を表す。
//! - 値は常に正規化不変条件 (plan.md §3.2) を満たす。
//! - 演算の便利メソッドは各演算モジュール (`ops/*.rs`) の `impl LogicVec` ブロックにある。

use std::fmt;

use crate::layout::{MAX_WIDTH, normalize_plane, words_for};
use crate::logic::Logic;
use crate::scalar;
use crate::view::{BitMut, BitRef, LogicMut, LogicRef};

/// bits 文字列の解析エラー ([`LogicVec::from_bits_str`], [`BitVec::from_bits_str`])。
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ParseBitsError {
    /// 使えない文字があった。`index` は文字列中のバイト位置。
    InvalidChar {
        /// 文字列中のバイト位置。
        index: usize,
        /// その文字。
        ch: char,
    },
    /// ビット数が [`MAX_WIDTH`] を超えた。
    TooWide {
        /// 文字列が表すビット数。
        width: usize,
    },
}

impl fmt::Display for ParseBitsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseBitsError::InvalidChar { index, ch } => {
                write!(f, "invalid character {ch:?} at byte {index} in bits string")
            }
            ParseBitsError::TooWide { width } => {
                write!(
                    f,
                    "bits string width {width} exceeds MAX_WIDTH ({MAX_WIDTH})"
                )
            }
        }
    }
}

impl std::error::Error for ParseBitsError {}

#[inline]
#[track_caller]
fn check_width(f: &str, width: u32) {
    assert!(
        width <= MAX_WIDTH,
        "{f}: width {width} exceeds MAX_WIDTH ({MAX_WIDTH})"
    );
}

/// ビット位置の検査。panic のメッセージは `"<f>: ..."` の形 (test-plan TC57)。
#[inline]
#[track_caller]
fn check_index(f: &str, width: u32, i: u32) {
    assert!(
        i < width,
        "{f}: bit index {i} out of range for width {width}"
    );
}

/// bits 文字列の文字数 (= 幅) を数える。文字の検査は呼び出し側で行う。
fn bits_str_width(s: &str) -> Result<u32, ParseBitsError> {
    let n = s.chars().count();
    if n > MAX_WIDTH as usize {
        return Err(ParseBitsError::TooWide { width: n });
    }
    Ok(n as u32)
}

/// 幅 `width` の値のプレーン `a` を、`signed` に従って `i64` として読む。
/// 値が `i64` に収まらなければ `None`。
fn plane_to_i64(width: u32, a: &[u64], signed: bool) -> Option<i64> {
    if width == 0 {
        return Some(0);
    }
    let nw = a.len();
    if !signed {
        if a[1..].iter().any(|&w| w != 0) || a[0] > i64::MAX as u64 {
            return None;
        }
        return Some(a[0] as i64);
    }
    if width <= 64 {
        return Some(scalar::sign_extend(a[0], width) as i64);
    }
    let top_bits = width - 64 * (nw as u32 - 1);
    let negative = scalar::sign_bit(a[nw - 1], top_bits);
    let fill = if negative { !0u64 } else { 0 };
    // 最上位ワードを符号拡張したものと、中間のワードは、すべて符号の値で埋まっていること。
    if scalar::sign_extend(a[nw - 1], top_bits) != fill {
        return None;
    }
    if a[1..nw - 1].iter().any(|&w| w != fill) {
        return None;
    }
    // ワード 0 の最上位ビットが符号と一致すること。
    if ((a[0] >> 63) != 0) != negative {
        return None;
    }
    Some(a[0] as i64)
}

/// 幅 `width` の値のプレーン `a` を `u64` として読む。収まらなければ `None`。
fn plane_to_u64(a: &[u64]) -> Option<u64> {
    match a.split_first() {
        None => Some(0),
        Some((&lo, rest)) => rest.iter().all(|&w| w == 0).then_some(lo),
    }
}

/// `v` を 2 の補数として幅 `width` に書く (符号拡張/切り詰め)。
fn write_i64(width: u32, a: &mut [u64], v: i64) {
    if let Some((lo, rest)) = a.split_first_mut() {
        *lo = v as u64;
        rest.fill(if v < 0 { !0 } else { 0 });
    }
    normalize_plane(width, a);
}

// ---------------------------------------------------------------------------
// LogicVec
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq, Eq, Hash)]
enum Store {
    /// 幅 ≤ 64 (幅 0 を含む)。
    Inline { a: u64, b: u64 },
    /// 幅 > 64。プレーン分離配置で、長さは `2 * words_for(width)`。
    Heap(Box<[u64]>),
}

/// 4 値の owned 値。幅 ≤ 64 はヒープ確保しない。
///
/// `PartialEq` は「幅が同じで全ビットが同一」(`===` の意味、§11.4.5)。
/// `Debug` は `8'b10xz_0001`、`Display` は 2 進 (`%b` 相当) を出す。
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct LogicVec {
    width: u32,
    store: Store,
}

impl LogicVec {
    /// 幅 `width` の全ビット 0。
    ///
    /// # Panics
    /// `width > MAX_WIDTH` の場合。
    pub fn zero(width: u32) -> Self {
        check_width("LogicVec::zero", width);
        let store = if width <= 64 {
            Store::Inline { a: 0, b: 0 }
        } else {
            Store::Heap(vec![0u64; 2 * words_for(width)].into_boxed_slice())
        };
        LogicVec { width, store }
    }

    /// 幅 `width` の全ビットを `v` にした値 (`'0 '1 'x 'z` の文脈幅への展開、§5.7.1)。
    pub fn filled(width: u32, v: Logic) -> Self {
        let mut r = LogicVec::zero(width);
        r.as_mut().fill(v);
        r
    }

    /// 符号なしの `v` を幅 `width` に切り詰めて (または 0 拡張して) 作る。
    pub fn from_u64(width: u32, v: u64) -> Self {
        let mut r = LogicVec::zero(width);
        {
            let (a, _) = r.planes_mut();
            if let Some(lo) = a.first_mut() {
                *lo = v;
            }
            normalize_plane(width, a);
        }
        r
    }

    /// `v` を 2 の補数として幅 `width` に切り詰めて (または符号拡張して) 作る。
    pub fn from_i64(width: u32, v: i64) -> Self {
        let mut r = LogicVec::zero(width);
        write_i64(width, r.planes_mut().0, v);
        r
    }

    /// ビューの内容を写して作る。
    pub fn from_ref(x: LogicRef<'_>) -> Self {
        let mut r = LogicVec::zero(x.width());
        r.as_mut().copy_from(x);
        r
    }

    /// bits 文字列から作る。MSB が先頭で、文字は `0 1 x X z Z` (大文字小文字を同一視)。
    /// `'_'` を含むそれ以外の文字はエラー (マネージャ決定 Q-22)。
    /// 幅は文字数で、空文字列なら幅 0。テスト・デバッグ用で、
    /// 参照モデルとの受け渡しにも使う (plan.md §5.1)。
    ///
    /// ```
    /// # use svsim_logic::{Logic, LogicVec};
    /// let v = LogicVec::from_bits_str("10Xz0001").unwrap();
    /// assert_eq!(v.width(), 8);
    /// assert_eq!(v.bit(5), Logic::X);
    /// assert_eq!(v.to_bits_string(), "10xz0001");
    /// ```
    pub fn from_bits_str(s: &str) -> Result<Self, ParseBitsError> {
        let width = bits_str_width(s)?;
        let mut r = LogicVec::zero(width);
        {
            let mut m = r.as_mut();
            let mut i = width;
            for (index, ch) in s.char_indices() {
                let v = Logic::from_char(ch).ok_or(ParseBitsError::InvalidChar { index, ch })?;
                i -= 1;
                m.set_bit(i, v);
            }
        }
        Ok(r)
    }

    /// bits 文字列 (MSB が先頭、小文字の `x`/`z`)。[`from_bits_str`](Self::from_bits_str) と往復する。
    pub fn to_bits_string(&self) -> String {
        self.as_ref().to_bits_string()
    }

    /// ビット幅。
    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// 読み取りビュー。
    #[inline]
    pub fn as_ref(&self) -> LogicRef<'_> {
        let n = words_for(self.width);
        match &self.store {
            Store::Inline { a, b } => LogicRef::new(
                self.width,
                &std::slice::from_ref(a)[..n],
                &std::slice::from_ref(b)[..n],
            ),
            Store::Heap(w) => LogicRef::from_planar(self.width, w),
        }
    }

    /// 書き込みビュー。
    #[inline]
    pub fn as_mut(&mut self) -> LogicMut<'_> {
        let n = words_for(self.width);
        match &mut self.store {
            Store::Inline { a, b } => LogicMut::new(
                self.width,
                &mut std::slice::from_mut(a)[..n],
                &mut std::slice::from_mut(b)[..n],
            ),
            Store::Heap(w) => LogicMut::from_planar(self.width, w),
        }
    }

    fn planes_mut(&mut self) -> (&mut [u64], &mut [u64]) {
        self.as_mut().into_planes()
    }

    /// ビット `i` (0 = LSB) の値。
    ///
    /// # Panics
    /// `i >= width` の場合。
    #[inline]
    pub fn bit(&self, i: u32) -> Logic {
        check_index("LogicVec::bit", self.width, i);
        self.as_ref().bit(i)
    }

    /// ビット `i` (0 = LSB) を `v` にする。
    ///
    /// # Panics
    /// `i >= width` の場合。
    #[inline]
    pub fn set_bit(&mut self, i: u32, v: Logic) {
        check_index("LogicVec::set_bit", self.width, i);
        self.as_mut().set_bit(i, v);
    }

    /// X または Z のビットがあれば真。
    #[inline]
    pub fn has_unknown(&self) -> bool {
        self.as_ref().has_unknown()
    }

    /// 符号なしの `u64` として読む。X/Z がある、または 64 ビットを超える位置に 1 があれば `None`。
    pub fn to_u64(&self) -> Option<u64> {
        let r = self.as_ref();
        if r.has_unknown() {
            return None;
        }
        plane_to_u64(r.aval())
    }

    /// `i64` として読む。`signed` なら幅 `width` の 2 の補数として解釈する。
    /// X/Z がある、または値が `i64` に収まらなければ `None`。
    pub fn to_i64(&self, signed: bool) -> Option<i64> {
        let r = self.as_ref();
        if r.has_unknown() {
            return None;
        }
        plane_to_i64(self.width, r.aval(), signed)
    }

    /// 2 値に変換する (X/Z → 0、§6.11.2)。
    pub fn to_bit_vec(&self) -> BitVec {
        let src = self.as_ref();
        let mut r = BitVec::zero(self.width);
        for ((d, &a), &b) in r.words_mut().iter_mut().zip(src.aval()).zip(src.bval()) {
            *d = a & !b;
        }
        r
    }
}

impl From<&BitVec> for LogicVec {
    /// 2 値 → 4 値 (bval = 0、§6.11.2)。
    fn from(v: &BitVec) -> LogicVec {
        let mut r = LogicVec::zero(v.width());
        r.planes_mut().0.copy_from_slice(v.as_ref().words());
        r
    }
}

impl From<BitVec> for LogicVec {
    /// 2 値 → 4 値 (bval = 0、§6.11.2)。
    fn from(v: BitVec) -> LogicVec {
        LogicVec::from(&v)
    }
}

impl From<Logic> for LogicVec {
    /// 幅 1 の値。
    fn from(v: Logic) -> LogicVec {
        LogicVec::filled(1, v)
    }
}

impl fmt::Debug for LogicVec {
    /// `8'b10xz_0001` の形。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.as_ref(), f)
    }
}

impl fmt::Display for LogicVec {
    /// 2 進 (`%b` 相当、MSB が先頭、`'_'` なし)。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.as_ref(), f)
    }
}

impl std::str::FromStr for LogicVec {
    type Err = ParseBitsError;

    /// [`LogicVec::from_bits_str`] と同じ。
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        LogicVec::from_bits_str(s)
    }
}

// ---------------------------------------------------------------------------
// BitVec
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq, Eq, Hash)]
enum Store2 {
    /// 幅 ≤ 64 (幅 0 を含む)。
    Inline(u64),
    /// 幅 > 64。長さは `words_for(width)`。
    Heap(Box<[u64]>),
}

/// 2 値の owned 値。幅 ≤ 64 はヒープ確保しない。
///
/// `PartialEq` は「幅が同じで全ビットが同一」。`Debug` は `8'b1010_0001`、`Display` は 2 進。
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct BitVec {
    width: u32,
    store: Store2,
}

impl BitVec {
    /// 幅 `width` の全ビット 0。
    ///
    /// # Panics
    /// `width > MAX_WIDTH` の場合。
    pub fn zero(width: u32) -> Self {
        check_width("BitVec::zero", width);
        let store = if width <= 64 {
            Store2::Inline(0)
        } else {
            Store2::Heap(vec![0u64; words_for(width)].into_boxed_slice())
        };
        BitVec { width, store }
    }

    /// 幅 `width` の全ビットを `v` にした値。
    pub fn filled(width: u32, v: bool) -> Self {
        let mut r = BitVec::zero(width);
        r.as_mut().fill(v);
        r
    }

    /// 符号なしの `v` を幅 `width` に切り詰めて (または 0 拡張して) 作る。
    pub fn from_u64(width: u32, v: u64) -> Self {
        let mut r = BitVec::zero(width);
        let a = r.words_mut();
        if let Some(lo) = a.first_mut() {
            *lo = v;
        }
        normalize_plane(width, a);
        r
    }

    /// `v` を 2 の補数として幅 `width` に切り詰めて (または符号拡張して) 作る。
    pub fn from_i64(width: u32, v: i64) -> Self {
        let mut r = BitVec::zero(width);
        write_i64(width, r.words_mut(), v);
        r
    }

    /// ビューの内容を写して作る。
    pub fn from_ref(x: BitRef<'_>) -> Self {
        let mut r = BitVec::zero(x.width());
        r.as_mut().copy_from(x);
        r
    }

    /// bits 文字列から作る。MSB が先頭で、文字は `0 1` だけ。`'_'` を含むそれ以外の文字はエラー
    /// (マネージャ決定 Q-22)。空文字列なら幅 0。
    pub fn from_bits_str(s: &str) -> Result<Self, ParseBitsError> {
        let width = bits_str_width(s)?;
        let mut r = BitVec::zero(width);
        {
            let mut m = r.as_mut();
            let mut i = width;
            for (index, ch) in s.char_indices() {
                let v = match ch {
                    '0' => false,
                    '1' => true,
                    _ => return Err(ParseBitsError::InvalidChar { index, ch }),
                };
                i -= 1;
                m.set_bit(i, v);
            }
        }
        Ok(r)
    }

    /// bits 文字列 (MSB が先頭)。[`from_bits_str`](Self::from_bits_str) と往復する。
    pub fn to_bits_string(&self) -> String {
        self.as_ref().to_bits_string()
    }

    /// ビット幅。
    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// 読み取りビュー。
    #[inline]
    pub fn as_ref(&self) -> BitRef<'_> {
        let n = words_for(self.width);
        match &self.store {
            Store2::Inline(a) => BitRef::new(self.width, &std::slice::from_ref(a)[..n]),
            Store2::Heap(w) => BitRef::new(self.width, w),
        }
    }

    /// 書き込みビュー。
    #[inline]
    pub fn as_mut(&mut self) -> BitMut<'_> {
        let n = words_for(self.width);
        match &mut self.store {
            Store2::Inline(a) => BitMut::new(self.width, &mut std::slice::from_mut(a)[..n]),
            Store2::Heap(w) => BitMut::new(self.width, w),
        }
    }

    fn words_mut(&mut self) -> &mut [u64] {
        self.as_mut().into_words()
    }

    /// ビット `i` (0 = LSB) の値。
    ///
    /// # Panics
    /// `i >= width` の場合。
    #[inline]
    pub fn bit(&self, i: u32) -> bool {
        check_index("BitVec::bit", self.width, i);
        self.as_ref().bit(i)
    }

    /// ビット `i` (0 = LSB) を `v` にする。
    ///
    /// # Panics
    /// `i >= width` の場合。
    #[inline]
    pub fn set_bit(&mut self, i: u32, v: bool) {
        check_index("BitVec::set_bit", self.width, i);
        self.as_mut().set_bit(i, v);
    }

    /// 符号なしの `u64` として読む。64 ビットを超える位置に 1 があれば `None`。
    pub fn to_u64(&self) -> Option<u64> {
        plane_to_u64(self.as_ref().words())
    }

    /// `i64` として読む。`signed` なら幅 `width` の 2 の補数として解釈する。
    /// 値が `i64` に収まらなければ `None`。
    pub fn to_i64(&self, signed: bool) -> Option<i64> {
        plane_to_i64(self.width, self.as_ref().words(), signed)
    }

    /// 4 値に変換する (bval = 0、§6.11.2)。
    pub fn to_logic_vec(&self) -> LogicVec {
        LogicVec::from(self)
    }
}

impl From<bool> for BitVec {
    /// 幅 1 の値。
    fn from(v: bool) -> BitVec {
        BitVec::from_u64(1, v as u64)
    }
}

impl fmt::Debug for BitVec {
    /// `8'b1010_0001` の形。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.as_ref(), f)
    }
}

impl fmt::Display for BitVec {
    /// 2 進 (`%b` 相当)。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.as_ref(), f)
    }
}

impl std::str::FromStr for BitVec {
    type Err = ParseBitsError;

    /// [`BitVec::from_bits_str`] と同じ。
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        BitVec::from_bits_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    const WIDTHS: [u32; 12] = [0, 1, 2, 3, 31, 63, 64, 65, 127, 128, 129, 257];

    fn hash_of<T: Hash>(v: &T) -> u64 {
        let mut h = DefaultHasher::new();
        v.hash(&mut h);
        h.finish()
    }

    #[test]
    fn zero_and_filled() {
        for w in WIDTHS {
            let z = LogicVec::zero(w);
            assert_eq!(z.width(), w);
            assert!(z.as_ref().is_normalized());
            assert_eq!(z.to_bits_string(), "0".repeat(w as usize));
            for v in Logic::ALL {
                let f = LogicVec::filled(w, v);
                assert!(f.as_ref().is_normalized(), "w={w} v={v:?}");
                assert_eq!(
                    f.to_bits_string(),
                    v.to_char().to_string().repeat(w as usize)
                );
            }
            let b = BitVec::filled(w, true);
            assert!(b.as_ref().is_normalized());
            assert_eq!(b.to_bits_string(), "1".repeat(w as usize));
        }
    }

    #[test]
    fn bits_string_round_trip() {
        let v = LogicVec::from_bits_str("1xz0").unwrap();
        assert_eq!(v.width(), 4);
        assert_eq!(v.bit(3), Logic::L1);
        assert_eq!(v.bit(2), Logic::X);
        assert_eq!(v.bit(1), Logic::Z);
        assert_eq!(v.bit(0), Logic::L0);
        assert_eq!(v.to_bits_string(), "1xz0");
        assert_eq!(
            LogicVec::from_bits_str("XZ").unwrap().to_bits_string(),
            "xz"
        );
        let e = LogicVec::from_bits_str("").unwrap();
        assert_eq!(e.width(), 0);
        assert_eq!(e.to_bits_string(), "");
        let long = format!("1{}x", "0".repeat(127));
        let v = LogicVec::from_bits_str(&long).unwrap();
        assert_eq!(v.width(), 129);
        assert_eq!(v.bit(128), Logic::L1);
        assert_eq!(v.bit(0), Logic::X);
        assert_eq!(v.to_bits_string(), long);
        assert!(v.as_ref().is_normalized());
    }

    #[test]
    fn bits_string_errors() {
        assert_eq!(
            LogicVec::from_bits_str("10?1"),
            Err(ParseBitsError::InvalidChar { index: 2, ch: '?' })
        );
        assert_eq!(
            BitVec::from_bits_str("10x"),
            Err(ParseBitsError::InvalidChar { index: 2, ch: 'x' })
        );
        // '_' はエラー (マネージャ決定 Q-22)
        assert_eq!(
            LogicVec::from_bits_str("1_0"),
            Err(ParseBitsError::InvalidChar { index: 1, ch: '_' })
        );
        assert!(LogicVec::from_bits_str("_").is_err());
        assert!(BitVec::from_bits_str("1_0").is_err());
        assert!(LogicVec::from_bits_str("1 0").is_err());
        let e = LogicVec::from_bits_str("2").unwrap_err();
        assert!(e.to_string().contains("invalid character"));
        let too_wide = "0".repeat(MAX_WIDTH as usize + 1);
        assert_eq!(
            LogicVec::from_bits_str(&too_wide),
            Err(ParseBitsError::TooWide {
                width: MAX_WIDTH as usize + 1
            })
        );
        assert!("10x".parse::<LogicVec>().is_ok());
        assert!("10x".parse::<BitVec>().is_err());
    }

    #[test]
    fn debug_and_display() {
        let v = LogicVec::from_bits_str("10xz0001").unwrap();
        assert_eq!(format!("{v:?}"), "8'b10xz_0001");
        assert_eq!(format!("{v}"), "10xz0001");
        let b = BitVec::from_bits_str("101").unwrap();
        assert_eq!(format!("{b:?}"), "3'b101");
        assert_eq!(format!("{b}"), "101");
        assert_eq!(format!("{:?}", LogicVec::zero(0)), "0'b");
        assert_eq!(
            format!("{:?}", LogicVec::from_u64(9, 0x100)),
            "9'b1_0000_0000"
        );
    }

    #[test]
    fn from_u64_truncates() {
        assert_eq!(LogicVec::from_u64(4, 0x1F).to_u64(), Some(0xF));
        assert_eq!(LogicVec::from_u64(64, u64::MAX).to_u64(), Some(u64::MAX));
        assert_eq!(LogicVec::from_u64(0, 5).to_u64(), Some(0));
        let w = LogicVec::from_u64(130, u64::MAX);
        assert_eq!(w.to_u64(), Some(u64::MAX));
        assert!(w.as_ref().is_normalized());
        assert_eq!(BitVec::from_u64(3, 0xFF).to_u64(), Some(7));
    }

    /// 2 の補数での切り詰めと符号拡張 (§11.8.2 の拡張と同じ規則)。
    #[test]
    fn from_i64_sign_extends_and_truncates() {
        assert_eq!(LogicVec::from_i64(4, -1).to_bits_string(), "1111");
        assert_eq!(LogicVec::from_i64(4, -8).to_bits_string(), "1000");
        assert_eq!(LogicVec::from_i64(4, 9).to_bits_string(), "1001");
        let v = LogicVec::from_i64(130, -2);
        assert_eq!(v.to_bits_string(), format!("{}0", "1".repeat(129)));
        assert!(v.as_ref().is_normalized());
        assert_eq!(v.to_i64(true), Some(-2));
        assert_eq!(v.to_i64(false), None);
        assert_eq!(v.to_u64(), None);
        let p = LogicVec::from_i64(130, 5);
        assert_eq!(p.to_i64(true), Some(5));
        assert_eq!(p.to_i64(false), Some(5));
        assert_eq!(BitVec::from_i64(70, i64::MIN).to_i64(true), Some(i64::MIN));
    }

    #[test]
    fn to_i64_boundaries() {
        // 幅 ≤ 64 の符号付き
        assert_eq!(LogicVec::from_u64(8, 0x80).to_i64(true), Some(-128));
        assert_eq!(LogicVec::from_u64(8, 0x80).to_i64(false), Some(128));
        assert_eq!(LogicVec::from_u64(64, 1 << 63).to_i64(true), Some(i64::MIN));
        assert_eq!(LogicVec::from_u64(64, 1 << 63).to_i64(false), None);
        assert_eq!(
            LogicVec::from_u64(64, i64::MAX as u64).to_i64(false),
            Some(i64::MAX)
        );
        assert_eq!(LogicVec::zero(0).to_i64(true), Some(0));
        // 幅 65: 2^63 は符号付き/符号なしとも i64 に収まらない
        let mut v = LogicVec::zero(65);
        v.set_bit(63, Logic::L1);
        assert_eq!(v.to_i64(true), None);
        assert_eq!(v.to_i64(false), None);
        assert_eq!(v.to_u64(), Some(1 << 63));
        // 幅 65: -2^63 (ビット 64 と 63 が 1) は符号付きで収まる
        v.set_bit(64, Logic::L1);
        assert_eq!(v.to_i64(true), Some(i64::MIN));
        // 幅 65: -1 から最上位以外の 1 ビットを落とすと範囲外
        let mut m = LogicVec::from_i64(129, -1);
        assert_eq!(m.to_i64(true), Some(-1));
        m.set_bit(100, Logic::L0);
        assert_eq!(m.to_i64(true), None);
        // X/Z があれば None
        let x = LogicVec::from_bits_str("0x").unwrap();
        assert_eq!(x.to_u64(), None);
        assert_eq!(x.to_i64(true), None);
    }

    /// 4 値 → 2 値は X/Z → 0、2 値 → 4 値は bval = 0 (§6.11.2)。
    #[test]
    fn two_state_conversions() {
        let v = LogicVec::from_bits_str("1x0z").unwrap();
        let b = v.to_bit_vec();
        assert_eq!(b.to_bits_string(), "1000");
        let back = LogicVec::from(&b);
        assert_eq!(back.to_bits_string(), "1000");
        assert!(!back.has_unknown());
        assert_eq!(b.to_logic_vec(), back);
        let wide = LogicVec::filled(200, Logic::X).to_bit_vec();
        assert_eq!(wide, BitVec::zero(200));
        assert_eq!(LogicVec::from(Logic::Z).to_bits_string(), "z");
        assert_eq!(BitVec::from(true).to_bits_string(), "1");
    }

    /// 同じ幅なら表現は 1 つなので、構築の経路によらず `==` と `Hash` が一致する (plan.md §8.3)。
    #[test]
    fn eq_and_hash_are_representation_independent() {
        for w in WIDTHS {
            let a = LogicVec::from_i64(w, -1);
            let b = LogicVec::filled(w, Logic::L1);
            let c = LogicVec::from_bits_str(&"1".repeat(w as usize)).unwrap();
            let mut d = LogicVec::zero(w);
            for i in 0..w {
                d.set_bit(i, Logic::L1);
            }
            for o in [&b, &c, &d] {
                assert_eq!(&a, o, "w={w}");
                assert_eq!(hash_of(&a), hash_of(o), "w={w}");
            }
        }
        // 幅が違えば別の値 (=== は幅の拡張後に比べるが、owned 値の == は幅も比べる)。
        assert_ne!(LogicVec::zero(3), LogicVec::zero(4));
        assert_ne!(LogicVec::zero(64), LogicVec::zero(65));
        // X と Z は区別する。
        assert_ne!(LogicVec::filled(2, Logic::X), LogicVec::filled(2, Logic::Z));
        assert_eq!(BitVec::from_i64(100, -1), BitVec::filled(100, true));
    }

    #[test]
    fn views_of_inline_and_heap() {
        let mut v = LogicVec::zero(64);
        assert_eq!(v.as_ref().num_words(), 1);
        v.as_mut().set_bit(63, Logic::X);
        assert_eq!(v.bit(63), Logic::X);
        let mut h = LogicVec::zero(65);
        assert_eq!(h.as_ref().num_words(), 2);
        h.as_mut().set_bit(64, Logic::Z);
        assert_eq!(h.as_ref().bval(), &[0, 1]);
        assert_eq!(h.as_ref().aval(), &[0, 0]);
        let copy = LogicVec::from_ref(h.as_ref());
        assert_eq!(copy, h);
        let b = BitVec::from_u64(5, 0b10110);
        assert_eq!(BitVec::from_ref(b.as_ref()), b);
        assert_eq!(LogicVec::zero(0).as_ref().num_words(), 0);
        assert_eq!(BitVec::zero(0).as_ref().num_words(), 0);
    }

    #[test]
    #[should_panic(expected = "MAX_WIDTH")]
    fn zero_rejects_too_wide() {
        let _ = LogicVec::zero(MAX_WIDTH + 1);
    }

    fn arb_bits(max: usize) -> impl Strategy<Value = String> {
        prop::collection::vec(prop::sample::select(vec!['0', '1', 'x', 'z']), 0..max)
            .prop_map(|v| v.into_iter().collect())
    }

    proptest! {
        /// bits 文字列は往復し、結果は常に正規化されている (plan.md §3.2)。
        #[test]
        fn prop_bits_round_trip(s in arb_bits(300)) {
            let v = LogicVec::from_bits_str(&s).unwrap();
            prop_assert_eq!(v.width() as usize, s.len());
            prop_assert!(v.as_ref().is_normalized());
            prop_assert_eq!(v.to_bits_string(), s.clone());
            let b = v.to_bit_vec();
            prop_assert!(b.as_ref().is_normalized());
            let expect: String = s.chars().map(|c| if c == '1' { '1' } else { '0' }).collect();
            prop_assert_eq!(b.to_bits_string(), expect.clone());
            prop_assert_eq!(BitVec::from_bits_str(&expect).unwrap(), b);
        }

        /// from_i64 → to_i64(true) は、幅が 64 以上なら値を保つ。
        #[test]
        fn prop_i64_round_trip(v in any::<i64>(), w in 64u32..300) {
            prop_assert_eq!(LogicVec::from_i64(w, v).to_i64(true), Some(v));
            prop_assert_eq!(BitVec::from_i64(w, v).to_i64(true), Some(v));
        }

        /// 幅 < 64 では、from_i64 は下位 w ビットへの切り詰めで、to_i64(true) は符号拡張。
        #[test]
        fn prop_i64_truncate(v in any::<i64>(), w in 1u32..64) {
            let got = LogicVec::from_i64(w, v).to_i64(true).unwrap();
            prop_assert_eq!(got as u64, scalar::sign_extend(v as u64, w));
            let u = LogicVec::from_i64(w, v).to_u64().unwrap();
            prop_assert_eq!(u, v as u64 & scalar::mask(w));
        }
    }
}
