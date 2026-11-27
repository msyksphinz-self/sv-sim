//! 層 2 のビュー: 4 値の [`LogicRef`] / [`LogicMut`]、2 値の [`BitRef`] / [`BitMut`] (plan.md §3.3)。
//!
//! ビューは幅とプレーンのスライスの組で、アリーナ上の信号や [`LogicVec`](crate::LogicVec) の中身を
//! 指す。4 値のビューの aval プレーンと bval プレーンは隣接していなくてよい。
//!
//! 構築時に検査するのはスライスの長さ (`words_for(width)`) だけで、正規化不変条件 (plan.md §3.2)
//! は検査しない。正規化された値を渡すのは呼び出し側の責務 (検査には `is_normalized` を使う)。

use std::fmt;

use crate::layout::{MAX_WIDTH, is_plane_normalized, normalize_plane, words_for};
use crate::logic::Logic;

#[inline]
fn check_len(width: u32, len: usize) {
    debug_assert!(width <= MAX_WIDTH, "width {width} exceeds MAX_WIDTH");
    assert_eq!(
        len,
        words_for(width),
        "plane length must be words_for({width})"
    );
}

#[inline]
fn check_index(width: u32, i: u32) {
    assert!(i < width, "bit index {i} out of range for width {width}");
}

#[inline]
fn get_plane_bit(words: &[u64], i: u32) -> bool {
    (words[(i / 64) as usize] >> (i % 64)) & 1 != 0
}

#[inline]
fn set_plane_bit(words: &mut [u64], i: u32, v: bool) {
    let w = &mut words[(i / 64) as usize];
    let m = 1u64 << (i % 64);
    if v {
        *w |= m;
    } else {
        *w &= !m;
    }
}

#[inline]
fn fill_plane(width: u32, words: &mut [u64], v: bool) {
    words.fill(if v { !0 } else { 0 });
    normalize_plane(width, words);
}

/// bits 文字列 (MSB が先頭、`'_'` なし) を書く。
fn write_bits(f: &mut fmt::Formatter<'_>, width: u32, bit: impl Fn(u32) -> char) -> fmt::Result {
    use fmt::Write;
    for i in (0..width).rev() {
        f.write_char(bit(i))?;
    }
    Ok(())
}

/// `Debug` 用の `8'b10xz_0001` 形式 (LSB から 4 ビットごとに `'_'`) を書く。
fn write_debug(f: &mut fmt::Formatter<'_>, width: u32, bit: impl Fn(u32) -> char) -> fmt::Result {
    use fmt::Write;
    write!(f, "{width}'b")?;
    for i in (0..width).rev() {
        f.write_char(bit(i))?;
        if i != 0 && i % 4 == 0 {
            f.write_char('_')?;
        }
    }
    Ok(())
}

fn bits_string(width: u32, bit: impl Fn(u32) -> char) -> String {
    let mut s = String::with_capacity(width as usize);
    for i in (0..width).rev() {
        s.push(bit(i));
    }
    s
}

// ---------------------------------------------------------------------------
// LogicRef
// ---------------------------------------------------------------------------

/// 4 値の読み取りビュー。`a` と `b` の長さはどちらも `words_for(width)`。
#[derive(Clone, Copy)]
pub struct LogicRef<'a> {
    width: u32,
    a: &'a [u64],
    b: &'a [u64],
}

impl<'a> LogicRef<'a> {
    /// aval プレーンと bval プレーンから作る。プレーンは隣接していなくてよい。
    ///
    /// # Panics
    /// どちらかの長さが `words_for(width)` でない場合。
    #[inline]
    pub fn new(width: u32, a: &'a [u64], b: &'a [u64]) -> Self {
        check_len(width, a.len());
        check_len(width, b.len());
        LogicRef { width, a, b }
    }

    /// プレーン分離配置 `[a.., b..]` のワード列から作る (アリーナ上の信号用)。
    ///
    /// # Panics
    /// `words.len() != 2 * words_for(width)` の場合。
    #[inline]
    pub fn from_planar(width: u32, words: &'a [u64]) -> Self {
        let nw = words_for(width);
        assert_eq!(
            words.len(),
            2 * nw,
            "planar length must be 2*words_for({width})"
        );
        let (a, b) = words.split_at(nw);
        LogicRef { width, a, b }
    }

    /// ビット幅。
    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// 1 プレーンのワード数 (`words_for(width)`)。
    #[inline]
    pub fn num_words(&self) -> usize {
        self.a.len()
    }

    /// aval プレーン。
    #[inline]
    pub fn aval(&self) -> &'a [u64] {
        self.a
    }

    /// bval プレーン。
    #[inline]
    pub fn bval(&self) -> &'a [u64] {
        self.b
    }

    /// ビット `i` (0 = LSB) の値。
    ///
    /// # Panics
    /// `i >= width` の場合。
    #[inline]
    pub fn bit(&self, i: u32) -> Logic {
        check_index(self.width, i);
        Logic::from_ab(get_plane_bit(self.a, i), get_plane_bit(self.b, i))
    }

    /// X または Z のビットがあれば真 (bval に 1 がある)。
    #[inline]
    pub fn has_unknown(&self) -> bool {
        self.b.iter().any(|&w| w != 0)
    }

    /// 正規化不変条件 (最上位ワードの `width` ビット目以上が両プレーンとも 0) を満たすか。
    #[inline]
    pub fn is_normalized(&self) -> bool {
        is_plane_normalized(self.width, self.a) && is_plane_normalized(self.width, self.b)
    }

    /// bits 文字列 (MSB が先頭、`'_'` なし、小文字の `x`/`z`)。
    pub fn to_bits_string(self) -> String {
        bits_string(self.width, |i| self.bit(i).to_char())
    }
}

impl fmt::Debug for LogicRef<'_> {
    /// `8'b10xz_0001` の形。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_debug(f, self.width, |i| self.bit(i).to_char())
    }
}

impl fmt::Display for LogicRef<'_> {
    /// 2 進 (`%b` 相当、MSB が先頭、`'_'` なし)。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_bits(f, self.width, |i| self.bit(i).to_char())
    }
}

// ---------------------------------------------------------------------------
// LogicMut
// ---------------------------------------------------------------------------

/// 4 値の書き込みビュー。`a` と `b` の長さはどちらも `words_for(width)`。
///
/// このビューのメソッド ([`fill`](Self::fill)、[`set_bit`](Self::set_bit)、
/// [`copy_from`](Self::copy_from)) は正規化不変条件を保つ。
/// [`planes_mut`](Self::planes_mut) で直接書く場合は、呼び出し側が不変条件を保つ。
pub struct LogicMut<'a> {
    width: u32,
    a: &'a mut [u64],
    b: &'a mut [u64],
}

impl<'a> LogicMut<'a> {
    /// aval プレーンと bval プレーンから作る。プレーンは隣接していなくてよい。
    ///
    /// # Panics
    /// どちらかの長さが `words_for(width)` でない場合。
    #[inline]
    pub fn new(width: u32, a: &'a mut [u64], b: &'a mut [u64]) -> Self {
        check_len(width, a.len());
        check_len(width, b.len());
        LogicMut { width, a, b }
    }

    /// プレーン分離配置 `[a.., b..]` のワード列から作る (アリーナ上の信号用)。
    ///
    /// # Panics
    /// `words.len() != 2 * words_for(width)` の場合。
    #[inline]
    pub fn from_planar(width: u32, words: &'a mut [u64]) -> Self {
        let nw = words_for(width);
        assert_eq!(
            words.len(),
            2 * nw,
            "planar length must be 2*words_for({width})"
        );
        let (a, b) = words.split_at_mut(nw);
        LogicMut { width, a, b }
    }

    /// ビット幅。
    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// 1 プレーンのワード数 (`words_for(width)`)。
    #[inline]
    pub fn num_words(&self) -> usize {
        self.a.len()
    }

    /// 短い寿命で借り直す (カーネルに渡したあとも使い続けるため)。
    #[inline]
    pub fn reborrow(&mut self) -> LogicMut<'_> {
        LogicMut {
            width: self.width,
            a: self.a,
            b: self.b,
        }
    }

    /// 読み取りビューとして借りる。
    #[inline]
    pub fn as_ref(&self) -> LogicRef<'_> {
        LogicRef {
            width: self.width,
            a: self.a,
            b: self.b,
        }
    }

    /// 読み取りビューに変換する (元の寿命を保つ)。
    #[inline]
    pub fn into_ref(self) -> LogicRef<'a> {
        LogicRef {
            width: self.width,
            a: self.a,
            b: self.b,
        }
    }

    /// aval プレーンと bval プレーンを可変で借りる。書いたあとの正規化は呼び出し側の責務。
    #[inline]
    pub fn planes_mut(&mut self) -> (&mut [u64], &mut [u64]) {
        (self.a, self.b)
    }

    /// aval プレーンと bval プレーンに分解する (元の寿命を保つ)。
    #[inline]
    pub fn into_planes(self) -> (&'a mut [u64], &'a mut [u64]) {
        (self.a, self.b)
    }

    /// ビット `i` (0 = LSB) の値。
    ///
    /// # Panics
    /// `i >= width` の場合。
    #[inline]
    pub fn bit(&self, i: u32) -> Logic {
        self.as_ref().bit(i)
    }

    /// 全ビットを `v` にする。
    #[inline]
    pub fn fill(&mut self, v: Logic) {
        fill_plane(self.width, self.a, v.aval());
        fill_plane(self.width, self.b, v.bval());
    }

    /// ビット `i` (0 = LSB) を `v` にする。
    ///
    /// # Panics
    /// `i >= width` の場合。
    #[inline]
    pub fn set_bit(&mut self, i: u32, v: Logic) {
        check_index(self.width, i);
        set_plane_bit(self.a, i, v.aval());
        set_plane_bit(self.b, i, v.bval());
    }

    /// `src` の内容を写す。
    ///
    /// # Panics
    /// 幅が一致しない場合。
    #[inline]
    pub fn copy_from(&mut self, src: LogicRef<'_>) {
        assert_eq!(self.width, src.width, "copy_from: width mismatch");
        self.a.copy_from_slice(src.a);
        self.b.copy_from_slice(src.b);
    }

    /// 正規化不変条件を満たすようにする (最上位ワードの `width` ビット目以上を 0 にする)。
    #[inline]
    pub fn normalize(&mut self) {
        normalize_plane(self.width, self.a);
        normalize_plane(self.width, self.b);
    }
}

impl fmt::Debug for LogicMut<'_> {
    /// `8'b10xz_0001` の形。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.as_ref(), f)
    }
}

// ---------------------------------------------------------------------------
// BitRef
// ---------------------------------------------------------------------------

/// 2 値の読み取りビュー。`a` の長さは `words_for(width)`。
#[derive(Clone, Copy)]
pub struct BitRef<'a> {
    width: u32,
    a: &'a [u64],
}

impl<'a> BitRef<'a> {
    /// ワード列から作る。
    ///
    /// # Panics
    /// `a.len() != words_for(width)` の場合。
    #[inline]
    pub fn new(width: u32, a: &'a [u64]) -> Self {
        check_len(width, a.len());
        BitRef { width, a }
    }

    /// ビット幅。
    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// ワード数 (`words_for(width)`)。
    #[inline]
    pub fn num_words(&self) -> usize {
        self.a.len()
    }

    /// ワード列 (aval プレーン)。
    #[inline]
    pub fn words(&self) -> &'a [u64] {
        self.a
    }

    /// ビット `i` (0 = LSB) の値。
    ///
    /// # Panics
    /// `i >= width` の場合。
    #[inline]
    pub fn bit(&self, i: u32) -> bool {
        check_index(self.width, i);
        get_plane_bit(self.a, i)
    }

    /// 正規化不変条件を満たすか。
    #[inline]
    pub fn is_normalized(&self) -> bool {
        is_plane_normalized(self.width, self.a)
    }

    /// bits 文字列 (MSB が先頭、`'_'` なし)。
    pub fn to_bits_string(self) -> String {
        bits_string(self.width, |i| if self.bit(i) { '1' } else { '0' })
    }
}

impl fmt::Debug for BitRef<'_> {
    /// `8'b1010_0001` の形。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_debug(f, self.width, |i| if self.bit(i) { '1' } else { '0' })
    }
}

impl fmt::Display for BitRef<'_> {
    /// 2 進 (`%b` 相当)。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_bits(f, self.width, |i| if self.bit(i) { '1' } else { '0' })
    }
}

// ---------------------------------------------------------------------------
// BitMut
// ---------------------------------------------------------------------------

/// 2 値の書き込みビュー。`a` の長さは `words_for(width)`。
///
/// このビューのメソッドは正規化不変条件を保つ。[`words_mut`](Self::words_mut) で直接書く場合は、
/// 呼び出し側が不変条件を保つ。
pub struct BitMut<'a> {
    width: u32,
    a: &'a mut [u64],
}

impl<'a> BitMut<'a> {
    /// ワード列から作る。
    ///
    /// # Panics
    /// `a.len() != words_for(width)` の場合。
    #[inline]
    pub fn new(width: u32, a: &'a mut [u64]) -> Self {
        check_len(width, a.len());
        BitMut { width, a }
    }

    /// ビット幅。
    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// ワード数 (`words_for(width)`)。
    #[inline]
    pub fn num_words(&self) -> usize {
        self.a.len()
    }

    /// 短い寿命で借り直す。
    #[inline]
    pub fn reborrow(&mut self) -> BitMut<'_> {
        BitMut {
            width: self.width,
            a: self.a,
        }
    }

    /// 読み取りビューとして借りる。
    #[inline]
    pub fn as_ref(&self) -> BitRef<'_> {
        BitRef {
            width: self.width,
            a: self.a,
        }
    }

    /// 読み取りビューに変換する (元の寿命を保つ)。
    #[inline]
    pub fn into_ref(self) -> BitRef<'a> {
        BitRef {
            width: self.width,
            a: self.a,
        }
    }

    /// ワード列を可変で借りる。書いたあとの正規化は呼び出し側の責務。
    #[inline]
    pub fn words_mut(&mut self) -> &mut [u64] {
        self.a
    }

    /// ワード列に変換する (元の寿命を保つ)。
    #[inline]
    pub fn into_words(self) -> &'a mut [u64] {
        self.a
    }

    /// ビット `i` (0 = LSB) の値。
    ///
    /// # Panics
    /// `i >= width` の場合。
    #[inline]
    pub fn bit(&self, i: u32) -> bool {
        self.as_ref().bit(i)
    }

    /// 全ビットを `v` にする。
    #[inline]
    pub fn fill(&mut self, v: bool) {
        fill_plane(self.width, self.a, v);
    }

    /// ビット `i` (0 = LSB) を `v` にする。
    ///
    /// # Panics
    /// `i >= width` の場合。
    #[inline]
    pub fn set_bit(&mut self, i: u32, v: bool) {
        check_index(self.width, i);
        set_plane_bit(self.a, i, v);
    }

    /// `src` の内容を写す。
    ///
    /// # Panics
    /// 幅が一致しない場合。
    #[inline]
    pub fn copy_from(&mut self, src: BitRef<'_>) {
        assert_eq!(self.width, src.width, "copy_from: width mismatch");
        self.a.copy_from_slice(src.a);
    }

    /// 正規化不変条件を満たすようにする。
    #[inline]
    pub fn normalize(&mut self) {
        normalize_plane(self.width, self.a);
    }
}

impl fmt::Debug for BitMut<'_> {
    /// `8'b1010_0001` の形。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.as_ref(), f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logic_ref_from_planar_and_bits() {
        // 幅 4: 値 4'b1x0z (ビット 3..0 = 1, x, 0, z)
        // aval = 1100, bval = 0101
        let words = [0b1100u64, 0b0101];
        let r = LogicRef::from_planar(4, &words);
        assert_eq!(r.width(), 4);
        assert_eq!(r.num_words(), 1);
        assert_eq!(r.bit(0), Logic::Z);
        assert_eq!(r.bit(1), Logic::L0);
        assert_eq!(r.bit(2), Logic::X);
        assert_eq!(r.bit(3), Logic::L1);
        assert!(r.has_unknown());
        assert!(r.is_normalized());
        assert_eq!(r.to_bits_string(), "1x0z");
        assert_eq!(format!("{r}"), "1x0z");
        assert_eq!(format!("{r:?}"), "4'b1x0z");
    }

    #[test]
    fn logic_ref_non_adjacent_planes() {
        let a = [u64::MAX, 1];
        let b = [0u64, 0];
        let r = LogicRef::new(65, &a, &b);
        assert!(!r.has_unknown());
        assert!(r.is_normalized());
        assert_eq!(r.bit(64), Logic::L1);
        assert_eq!(r.to_bits_string(), "1".repeat(65));
        // 正規化されていない値は検出できる。
        let a2 = [0u64, 2];
        assert!(!LogicRef::new(65, &a2, &b).is_normalized());
    }

    #[test]
    #[should_panic(expected = "plane length")]
    fn logic_ref_rejects_bad_length() {
        let a = [0u64; 2];
        let b = [0u64; 1];
        let _ = LogicRef::new(64, &a, &b);
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn bit_index_out_of_range_panics() {
        let w = [0u64, 0];
        let _ = LogicRef::from_planar(8, &w).bit(8);
    }

    #[test]
    fn debug_groups_by_four_from_lsb() {
        // aval 1010_0001, bval 0011_0000 → 10xz_0001 (plan.md §3.3 の Debug の例)
        let words = [0b1010_0001u64, 0b0011_0000];
        let r = LogicRef::from_planar(8, &words);
        assert_eq!(format!("{r:?}"), "8'b10xz_0001");
        let words = [0b1_0000u64, 0];
        assert_eq!(
            format!("{:?}", LogicRef::from_planar(5, &words)),
            "5'b1_0000"
        );
        assert_eq!(format!("{:?}", LogicRef::from_planar(0, &[])), "0'b");
        assert_eq!(LogicRef::from_planar(0, &[]).to_bits_string(), "");
    }

    #[test]
    fn logic_mut_fill_set_copy_keep_normalized() {
        for width in [1u32, 3, 63, 64, 65, 127, 128, 129] {
            let mut words = vec![0u64; 2 * words_for(width)];
            let mut m = LogicMut::from_planar(width, &mut words);
            for v in Logic::ALL {
                m.fill(v);
                assert!(m.as_ref().is_normalized(), "fill {v:?} width {width}");
                for i in 0..width {
                    assert_eq!(m.bit(i), v);
                }
            }
            m.fill(Logic::L0);
            m.set_bit(width - 1, Logic::X);
            m.set_bit(0, Logic::Z);
            assert_eq!(m.bit(0), Logic::Z);
            if width > 1 {
                assert_eq!(m.bit(width - 1), Logic::X);
            }
            assert!(m.as_ref().is_normalized());
            m.set_bit(width - 1, Logic::L0);
            assert_eq!(m.bit(width - 1), Logic::L0);

            // 別のバッファに写す (プレーンが隣接しない書き込みビュー)。
            let mut a = vec![!0u64; words_for(width)];
            let mut b = vec![!0u64; words_for(width)];
            let mut dst = LogicMut::new(width, &mut a, &mut b);
            dst.copy_from(m.as_ref());
            assert_eq!(dst.as_ref().to_bits_string(), m.as_ref().to_bits_string());
        }
    }

    #[test]
    fn logic_mut_reborrow_and_planes() {
        let mut words = [0u64; 4];
        let mut m = LogicMut::from_planar(70, &mut words);
        {
            let mut r = m.reborrow();
            r.set_bit(69, Logic::L1);
        }
        {
            let (a, b) = m.planes_mut();
            a[0] = !0;
            b[1] = !0;
        }
        assert!(!m.as_ref().is_normalized());
        m.normalize();
        assert!(m.as_ref().is_normalized());
        assert_eq!(m.bit(69), Logic::X);
        assert_eq!(m.bit(0), Logic::L1);
        let r = m.into_ref();
        assert_eq!(r.bval()[1], crate::top_mask(70));
    }

    #[test]
    #[should_panic(expected = "width mismatch")]
    fn copy_from_width_mismatch_panics() {
        let mut w1 = [0u64; 2];
        let w2 = [0u64; 2];
        LogicMut::from_planar(8, &mut w1).copy_from(LogicRef::from_planar(7, &w2));
    }

    #[test]
    fn bit_views() {
        let mut words = [0u64; 2];
        let mut m = BitMut::new(100, &mut words);
        m.fill(true);
        assert!(m.as_ref().is_normalized());
        assert_eq!(m.as_ref().words(), &[u64::MAX, (1u64 << 36) - 1]);
        m.set_bit(99, false);
        assert!(!m.bit(99));
        assert!(m.bit(98));
        let mut other = [0u64; 2];
        let mut o = BitMut::new(100, &mut other);
        o.copy_from(m.as_ref());
        assert_eq!(o.as_ref().to_bits_string(), m.as_ref().to_bits_string());
        o.words_mut()[1] = !0;
        o.normalize();
        assert!(o.as_ref().is_normalized());
        let r = BitRef::new(3, &[0b101]);
        assert_eq!(format!("{r:?}"), "3'b101");
        assert_eq!(format!("{r}"), "101");
        assert_eq!(r.to_bits_string(), "101");
    }
}
