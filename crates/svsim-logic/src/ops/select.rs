//! ビット選択・部分選択とインデックスの解釈、`get_slice2` 等 (IEEE 1800-2023 §11.5.1、
//! plan.md §3.5(e)、§3.5.1 規則 5)。
//!
//! - オフセットは LSB を 0 とする**物理オフセット** ([`Index::Value`])。宣言範囲 `[msb:lsb]` や
//!   `+:` `-:` からの変換は sema/MIR の責務。
//! - 読み: 範囲外のビットは **4 値なら X、2 値なら 0** (§11.5.1)。一部だけ範囲外でも、完全に範囲外でも
//!   同じ。[`Index::Unknown`] (X/Z を含むインデックス) なら全ビットが範囲外と同じ扱い。
//! - **2 値のベクタを X/Z のインデックスで読んだ値は 2 値の 0** (plan.md §3.5.1 規則 5、決定 R-2)。
//!   そのため、2 値の選択の結果は 4 値にしない ([`get_slice2`])。
//! - 書き: 範囲内のビットだけを書き、範囲外は無視する。`Unknown` なら何も書かない。
//! - `offset + 幅` のオーバフローを起こさないよう、範囲の計算は `i128` で行う。

use crate::logic::Logic;
use crate::ops::shift::{copy_bits, fill_bits, plane_is_zero};
use crate::scalar::{L64, mask, sign_bit, sign_extend};
use crate::vec::{BitVec, LogicVec};
use crate::view::{BitMut, BitRef, LogicMut, LogicRef};

/// 選択のインデックス (§11.5.1)。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Index {
    /// インデックスに X/Z がある。読みは全ビットが範囲外と同じ、書きは何もしない。
    Unknown,
    /// LSB を 0 とする物理オフセット。
    Value(i64),
}

/// 選択 `[off, off + len)` と `[0, width)` の重なり。`(lo, hi, dst_off)` を返す
/// (`src[lo..hi]` が `dst[dst_off..dst_off + (hi - lo)]` に対応する)。重ならなければ `None`。
#[inline]
fn overlap(off: i64, len: u32, width: u32) -> Option<(usize, usize, usize)> {
    let start = off as i128;
    let end = start + len as i128;
    let lo = start.max(0);
    let hi = end.min(width as i128);
    if lo < hi {
        Some((lo as usize, hi as usize, (lo - start) as usize))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// インデックスの解釈
// ---------------------------------------------------------------------------

/// 1 プレーン (X/Z なし) を `i64` に飽和させて読む。
fn plane_to_i64_sat(width: u32, a: &[u64], signed: bool) -> i64 {
    if width == 0 {
        return 0;
    }
    let nw = a.len();
    if !signed || width < 64 && !sign_bit(a[0], width) {
        // 非負の値。
        if !plane_is_zero(&a[1..]) || a[0] > i64::MAX as u64 {
            return i64::MAX;
        }
        return a[0] as i64;
    }
    if width <= 64 {
        return sign_extend(a[0], width) as i64;
    }
    let top_bits = width - 64 * (nw as u32 - 1);
    let negative = sign_bit(a[nw - 1], top_bits);
    let fill = if negative { !0u64 } else { 0 };
    let fits = sign_extend(a[nw - 1], top_bits) == fill
        && a[1..nw - 1].iter().all(|&w| w == fill)
        && ((a[0] >> 63) != 0) == negative;
    match (fits, negative) {
        (true, _) => a[0] as i64,
        (false, true) => i64::MIN,
        (false, false) => i64::MAX,
    }
}

/// インデックスの値を解釈する (§11.5.1)。X/Z があれば [`Index::Unknown`]。`signed` ならインデックスを
/// 2 の補数として読む。`i64` に収まらない値は飽和させる (どのみち範囲外になる)。
pub fn index_value(idx: LogicRef<'_>, signed: bool) -> Index {
    if idx.has_unknown() {
        Index::Unknown
    } else {
        Index::Value(plane_to_i64_sat(idx.width(), idx.aval(), signed))
    }
}

/// 2 値のインデックスを解釈する。結果は常に [`Index::Value`]。
pub fn index_value2(idx: BitRef<'_>, signed: bool) -> Index {
    Index::Value(plane_to_i64_sat(idx.width(), idx.words(), signed))
}

/// 層 1 のインデックスの解釈 (幅 `w`、`1 <= w <= 64`)。
#[inline]
pub const fn index_value_l64(idx: L64, w: u32, signed: bool) -> Index {
    if idx.b != 0 {
        return Index::Unknown;
    }
    let v = idx.a & mask(w);
    if signed {
        Index::Value(sign_extend(v, w) as i64)
    } else if v > i64::MAX as u64 {
        Index::Value(i64::MAX)
    } else {
        Index::Value(v as i64)
    }
}

// ---------------------------------------------------------------------------
// 層 2: 4 値
// ---------------------------------------------------------------------------

/// ビット選択・部分選択の読み (§11.5.1)。選択幅は `dst` の幅。範囲外のビットは X。
/// `Unknown` なら全 X。
pub fn get_slice(dst: LogicMut<'_>, src: LogicRef<'_>, idx: Index) {
    let dw = dst.width();
    let (a, b) = dst.into_planes();
    let hit = match idx {
        Index::Unknown => None,
        Index::Value(off) => overlap(off, dw, src.width()),
    };
    match hit {
        None => {
            fill_bits(a, 0, dw as usize, true);
            fill_bits(b, 0, dw as usize, true);
        }
        Some((lo, hi, d)) => {
            let n = hi - lo;
            fill_bits(a, 0, d, true);
            fill_bits(b, 0, d, true);
            copy_bits(a, d, src.aval(), lo, n);
            copy_bits(b, d, src.bval(), lo, n);
            fill_bits(a, d + n, dw as usize, true);
            fill_bits(b, d + n, dw as usize, true);
        }
    }
}

/// ビット選択の読み (§11.5.1)。範囲外または `Unknown` なら X。
pub fn get_bit(src: LogicRef<'_>, idx: Index) -> Logic {
    match idx {
        Index::Value(off) if off >= 0 && off < src.width() as i64 => src.bit(off as u32),
        _ => Logic::X,
    }
}

/// ビット選択・部分選択への書き込み (§11.5.1)。範囲内のビットだけを書き、範囲外は無視する。
/// `Unknown` なら何も書かない。選択幅は `src` の幅。
pub fn set_slice(dst: LogicMut<'_>, idx: Index, src: LogicRef<'_>) {
    let Index::Value(off) = idx else { return };
    let Some((lo, hi, s)) = overlap(off, src.width(), dst.width()) else {
        return;
    };
    let (a, b) = dst.into_planes();
    copy_bits(a, lo, src.aval(), s, hi - lo);
    copy_bits(b, lo, src.bval(), s, hi - lo);
}

/// ビット選択への書き込み (§11.5.1)。範囲外または `Unknown` なら何もしない。
pub fn set_bit(mut dst: LogicMut<'_>, idx: Index, v: Logic) {
    if let Index::Value(off) = idx
        && off >= 0
        && off < dst.width() as i64
    {
        dst.set_bit(off as u32, v);
    }
}

// ---------------------------------------------------------------------------
// 層 2: 2 値 (範囲外・Unknown は 0、§11.5.1、plan.md §3.5.1 規則 5)
// ---------------------------------------------------------------------------

/// 2 値の選択の読み (§11.5.1)。範囲外のビットは 0、`Unknown` なら全 0 (決定 R-2)。
/// インデックスが 4 値でも、結果は 2 値のまま (plan.md §3.5.1 規則 5)。
pub fn get_slice2(dst: BitMut<'_>, src: BitRef<'_>, idx: Index) {
    let dw = dst.width();
    let a = dst.into_words();
    a.fill(0);
    if let Index::Value(off) = idx
        && let Some((lo, hi, d)) = overlap(off, dw, src.width())
    {
        copy_bits(a, d, src.words(), lo, hi - lo);
    }
}

/// 2 値のビット選択の読み。範囲外または `Unknown` なら 0。
pub fn get_bit2(src: BitRef<'_>, idx: Index) -> bool {
    match idx {
        Index::Value(off) if off >= 0 && off < src.width() as i64 => src.bit(off as u32),
        _ => false,
    }
}

/// 2 値の選択への書き込み。範囲内のビットだけを書く。`Unknown` なら何も書かない。
pub fn set_slice2(dst: BitMut<'_>, idx: Index, src: BitRef<'_>) {
    let Index::Value(off) = idx else { return };
    let Some((lo, hi, s)) = overlap(off, src.width(), dst.width()) else {
        return;
    };
    copy_bits(dst.into_words(), lo, src.words(), s, hi - lo);
}

/// 2 値のビット選択への書き込み。範囲外または `Unknown` なら何もしない。
pub fn set_bit2(mut dst: BitMut<'_>, idx: Index, v: bool) {
    if let Index::Value(off) = idx
        && off >= 0
        && off < dst.width() as i64
    {
        dst.set_bit(off as u32, v);
    }
}

// ---------------------------------------------------------------------------
// 層 1 (選択元・選択幅とも 64 以下)
// ---------------------------------------------------------------------------

/// 読みの結果のうち、選択元の範囲内にあるビットのマスク (`[off, off + sel_w)` と `[0, src_w)` の重なり
/// を、結果のビット位置で表したもの)。重ならなければ `None`。
#[inline]
const fn l64_overlap(off: i64, sel_w: u32, src_w: u32) -> Option<u64> {
    let start = off as i128;
    let end = start + sel_w as i128;
    let lo = if start > 0 { start } else { 0 };
    let hi = if end < src_w as i128 {
        end
    } else {
        src_w as i128
    };
    if lo >= hi {
        return None;
    }
    // 結果のビット [lo - start, hi - start) が範囲内。
    Some(mask((hi - lo) as u32) << (lo - start) as u32)
}

/// `s >= 0` なら `v >> s`、負なら `v << -s` (64 以上ずらせば 0)。
#[inline]
const fn shift_by(v: u64, s: i128) -> u64 {
    if s >= 0 {
        if s >= 64 { 0 } else { v >> (s as u32) }
    } else if -s >= 64 {
        0
    } else {
        v << ((-s) as u32)
    }
}

/// 層 1 の選択の読み (`src_w`, `sel_w` は 1〜64)。範囲外は X。
#[inline]
pub const fn get_slice_l64(src: L64, src_w: u32, sel_w: u32, idx: Index) -> L64 {
    let all = mask(sel_w);
    let off = match idx {
        Index::Unknown => return L64::all_x(sel_w),
        Index::Value(off) => off,
    };
    match l64_overlap(off, sel_w, src_w) {
        None => L64::all_x(sel_w),
        Some(m) => {
            let out = all & !m;
            L64 {
                a: (shift_by(src.a, off as i128) & m) | out,
                b: (shift_by(src.b, off as i128) & m) | out,
            }
        }
    }
}

/// 層 1 の 2 値の選択の読み。範囲外・`Unknown` は 0。
#[inline]
pub const fn get_slice2_l64(src: u64, src_w: u32, sel_w: u32, idx: Index) -> u64 {
    let off = match idx {
        Index::Unknown => return 0,
        Index::Value(off) => off,
    };
    match l64_overlap(off, sel_w, src_w) {
        None => 0,
        Some(m) => shift_by(src, off as i128) & m,
    }
}

/// 層 1 の選択への書き込み。`dst` (幅 `dst_w`) のうち範囲内のビットを `src` (幅 `src_w`) で置き換えた
/// 値を返す。
#[inline]
pub const fn set_slice_l64(dst: L64, dst_w: u32, idx: Index, src: L64, src_w: u32) -> L64 {
    let off = match idx {
        Index::Unknown => return dst,
        Index::Value(off) => off,
    };
    // dst のビット p は src のビット p - off。
    match set_mask(off, src_w, dst_w) {
        None => dst,
        Some(m) => L64 {
            a: (dst.a & !m) | (shift_by(src.a, -(off as i128)) & m),
            b: (dst.b & !m) | (shift_by(src.b, -(off as i128)) & m),
        },
    }
}

/// 層 1 の 2 値の選択への書き込み。
#[inline]
pub const fn set_slice2_l64(dst: u64, dst_w: u32, idx: Index, src: u64, src_w: u32) -> u64 {
    let off = match idx {
        Index::Unknown => return dst,
        Index::Value(off) => off,
    };
    match set_mask(off, src_w, dst_w) {
        None => dst,
        Some(m) => (dst & !m) | (shift_by(src, -(off as i128)) & m),
    }
}

/// 書き込みで変わる `dst` のビットのマスク (`[off, off + src_w)` と `[0, dst_w)` の重なり)。
#[inline]
const fn set_mask(off: i64, src_w: u32, dst_w: u32) -> Option<u64> {
    let start = off as i128;
    let end = start + src_w as i128;
    let lo = if start > 0 { start } else { 0 };
    let hi = if end < dst_w as i128 {
        end
    } else {
        dst_w as i128
    };
    if lo >= hi {
        None
    } else {
        Some(mask((hi - lo) as u32) << lo as u32)
    }
}

/// 層 1 のビット選択の読み。
#[inline]
pub const fn get_bit_l64(src: L64, src_w: u32, idx: Index) -> Logic {
    match idx {
        Index::Value(off) if off >= 0 && off < src_w as i64 => src.bit(off as u32),
        _ => Logic::X,
    }
}

// ---------------------------------------------------------------------------
// 層 3
// ---------------------------------------------------------------------------

impl LogicVec {
    /// インデックスとしての解釈 (§11.5.1)。[`index_value`] を参照。
    pub fn index_value(&self, signed: bool) -> Index {
        index_value(self.as_ref(), signed)
    }

    /// 幅 `width` の選択の読み (§11.5.1)。範囲外は X。
    pub fn get_slice(&self, width: u32, idx: Index) -> LogicVec {
        let mut r = LogicVec::zero(width);
        get_slice(r.as_mut(), self.as_ref(), idx);
        r
    }

    /// ビット選択の読み (§11.5.1)。範囲外・`Unknown` は X。
    pub fn get_bit(&self, idx: Index) -> Logic {
        get_bit(self.as_ref(), idx)
    }

    /// 選択への書き込み (§11.5.1)。範囲外は無視する。
    pub fn set_slice(&mut self, idx: Index, src: &LogicVec) {
        set_slice(self.as_mut(), idx, src.as_ref());
    }

    /// ビット選択への書き込み (§11.5.1)。範囲外・`Unknown` なら何もしない。
    /// (範囲外で panic する [`LogicVec::set_bit`] と違い、インデックスは [`Index`]。)
    pub fn set_bit_at(&mut self, idx: Index, v: Logic) {
        set_bit(self.as_mut(), idx, v);
    }
}

impl BitVec {
    /// 2 値のインデックスとしての解釈。
    pub fn index_value(&self, signed: bool) -> Index {
        index_value2(self.as_ref(), signed)
    }

    /// 2 値の選択の読み (§11.5.1)。範囲外・`Unknown` は 0。
    pub fn get_slice(&self, width: u32, idx: Index) -> BitVec {
        let mut r = BitVec::zero(width);
        get_slice2(r.as_mut(), self.as_ref(), idx);
        r
    }

    /// 2 値のビット選択の読み。範囲外・`Unknown` は 0。
    pub fn get_bit(&self, idx: Index) -> bool {
        get_bit2(self.as_ref(), idx)
    }

    /// 2 値の選択への書き込み。
    pub fn set_slice(&mut self, idx: Index, src: &BitVec) {
        set_slice2(self.as_mut(), idx, src.as_ref());
    }

    /// 2 値のビット選択への書き込み。範囲外・`Unknown` なら何もしない。
    pub fn set_bit_at(&mut self, idx: Index, v: bool) {
        set_bit2(self.as_mut(), idx, v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lv(s: &str) -> LogicVec {
        LogicVec::from_bits_str(s).unwrap()
    }

    /// §11.5.1: 範囲外のビットは 4 値なら x、2 値なら 0。
    #[test]
    fn select_11_5_1() {
        let src = lv("1010x01z");
        assert_eq!(src.get_slice(4, Index::Value(2)).to_bits_string(), "10x0");
        assert_eq!(src.get_slice(4, Index::Value(6)).to_bits_string(), "xx10");
        assert_eq!(src.get_slice(4, Index::Value(-2)).to_bits_string(), "1zxx");
        assert_eq!(
            src.get_slice(4, Index::Value(i64::MAX)).to_bits_string(),
            "xxxx"
        );
        assert_eq!(src.get_slice(4, Index::Unknown).to_bits_string(), "xxxx");
        let b = BitVec::from_bits_str("10100011").unwrap();
        assert_eq!(b.get_slice(4, Index::Value(6)).to_bits_string(), "0010");
        assert_eq!(b.get_slice(4, Index::Unknown).to_bits_string(), "0000");
        let mut d = LogicVec::zero(8);
        d.set_slice(Index::Value(-2), &lv("1011"));
        assert_eq!(d.to_bits_string(), "00000010");
        assert_eq!(
            get_slice_l64(L64::load(src.as_ref()), 8, 4, Index::Value(-2)),
            L64::load(lv("1zxx").as_ref())
        );
        assert_eq!(
            set_slice_l64(L64::ZERO, 8, Index::Value(6), L64::new(0xF, 0), 4),
            L64::new(0xC0, 0)
        );
    }
}
