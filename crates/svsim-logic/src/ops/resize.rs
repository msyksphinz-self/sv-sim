//! 拡張・切り詰めと 2 値/4 値の変換 (IEEE 1800-2023 §6.11.2, §6.24.1, §11.6.1, §11.8.2, §11.8.4、
//! plan.md §3.5(e))。
//!
//! - 切り詰めは下位ビットを残す。
//! - 拡張は、符号なしなら 0 で埋め、符号付きなら符号ビットの値 (aval・bval とも) を複製する。
//!   符号ビットが X なら X、Z なら Z で埋まる (§11.8.2: 拡張は拡張する前のオペランドの符号で行う)。
//! - 4 値 → 2 値は X/Z → 0、2 値 → 4 値は bval = 0 (§6.11.2)。
//!
//! 目標幅は `dst` の幅。幅 0 の入力 (連結の外では呼び出し側の誤り) は 0 として扱い、panic しない。

use crate::layout::normalize_plane;
use crate::ops::shift::{copy_bits, fill_bits};
use crate::scalar::{L64, mask};
use crate::vec::{BitVec, LogicVec};
use crate::view::{BitMut, BitRef, LogicMut, LogicRef};

/// 1 プレーンを拡張・切り詰めする。`fill` は拡張で埋める値。
fn resize_plane(dst: &mut [u64], dw: u32, src: &[u64], sw: u32, fill: bool) {
    let keep = dw.min(sw) as usize;
    copy_bits(dst, 0, src, 0, keep);
    fill_bits(dst, keep, dw as usize, fill);
    normalize_plane(dw, dst);
}

#[inline]
fn plane_msb(w: &[u64], width: u32) -> bool {
    if width == 0 {
        return false;
    }
    let i = width - 1;
    (w[(i / 64) as usize] >> (i % 64)) & 1 != 0
}

/// `dst = x` を `dst` の幅に拡張・切り詰めしたもの (§11.6.1, §11.8.2, §6.24.1)。
/// `signed` は `x` (拡張する前のオペランド) が符号付きか。
pub fn resize(dst: LogicMut<'_>, x: LogicRef<'_>, signed: bool) {
    let (dw, sw) = (dst.width(), x.width());
    let (fa, fb) = if signed {
        (plane_msb(x.aval(), sw), plane_msb(x.bval(), sw))
    } else {
        (false, false)
    };
    let (a, b) = dst.into_planes();
    resize_plane(a, dw, x.aval(), sw, fa);
    resize_plane(b, dw, x.bval(), sw, fb);
}

/// 2 値の [`resize`]。
pub fn resize2(dst: BitMut<'_>, x: BitRef<'_>, signed: bool) {
    let (dw, sw) = (dst.width(), x.width());
    let f = signed && plane_msb(x.words(), sw);
    resize_plane(dst.into_words(), dw, x.words(), sw, f);
}

/// 4 値 → 2 値 (§6.11.2)。X/Z は 0 になる。
///
/// # Panics
/// 幅が違う場合。
pub fn to_two_state(dst: BitMut<'_>, x: LogicRef<'_>) {
    assert_eq!(
        dst.width(),
        x.width(),
        "to_two_state: width mismatch (dst {}, x {})",
        dst.width(),
        x.width()
    );
    for ((d, &a), &b) in dst.into_words().iter_mut().zip(x.aval()).zip(x.bval()) {
        *d = a & !b;
    }
}

/// 2 値 → 4 値 (§6.11.2)。bval = 0。
///
/// # Panics
/// 幅が違う場合。
pub fn from_two_state(dst: LogicMut<'_>, x: BitRef<'_>) {
    assert_eq!(
        dst.width(),
        x.width(),
        "from_two_state: width mismatch (dst {}, x {})",
        dst.width(),
        x.width()
    );
    let (a, b) = dst.into_planes();
    a.copy_from_slice(x.words());
    b.fill(0);
}

// ---------------------------------------------------------------------------
// 層 1
// ---------------------------------------------------------------------------

/// 層 1 の拡張・切り詰め: 幅 `from` の `x` を幅 `to` にする (`0 <= from, to <= 64`)。
#[inline]
pub const fn resize_l64(x: L64, from: u32, to: u32, signed: bool) -> L64 {
    let keep = mask(if from < to { from } else { to });
    let ext = mask(to) & !keep;
    let (sa, sb) = if signed && from > 0 {
        ((x.a >> (from - 1)) & 1 != 0, (x.b >> (from - 1)) & 1 != 0)
    } else {
        (false, false)
    };
    L64 {
        a: (x.a & keep) | if sa { ext } else { 0 },
        b: (x.b & keep) | if sb { ext } else { 0 },
    }
}

/// 層 1 の 2 値の拡張・切り詰め。
#[inline]
pub const fn resize2_l64(x: u64, from: u32, to: u32, signed: bool) -> u64 {
    resize_l64(L64 { a: x, b: 0 }, from, to, signed).a
}

/// 層 1 の 4 値 → 2 値 (X/Z → 0、§6.11.2)。
#[inline]
pub const fn to_two_state_l64(x: L64) -> u64 {
    x.a & !x.b
}

/// 層 1 の 2 値 → 4 値 (bval = 0)。
#[inline]
pub const fn from_two_state_l64(x: u64) -> L64 {
    L64 { a: x, b: 0 }
}

// ---------------------------------------------------------------------------
// 層 3
// ---------------------------------------------------------------------------

impl LogicVec {
    /// 幅 `width` に拡張・切り詰めした値 (§11.6.1, §11.8.2)。`signed` は `self` が符号付きか。
    pub fn resize(&self, width: u32, signed: bool) -> LogicVec {
        let mut r = LogicVec::zero(width);
        resize(r.as_mut(), self.as_ref(), signed);
        r
    }

    /// 4 値 → 2 値 (X/Z → 0、§6.11.2)。[`LogicVec::to_bit_vec`] と同じ。
    pub fn to_two_state(&self) -> BitVec {
        let mut r = BitVec::zero(self.width());
        to_two_state(r.as_mut(), self.as_ref());
        r
    }

    /// 2 値 → 4 値 (bval = 0、§6.11.2)。
    pub fn from_two_state(x: &BitVec) -> LogicVec {
        let mut r = LogicVec::zero(x.width());
        from_two_state(r.as_mut(), x.as_ref());
        r
    }
}

impl BitVec {
    /// 2 値の拡張・切り詰め (§11.6.1, §11.8.2)。
    pub fn resize(&self, width: u32, signed: bool) -> BitVec {
        let mut r = BitVec::zero(width);
        resize2(r.as_mut(), self.as_ref(), signed);
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::Logic;

    fn lv(s: &str) -> LogicVec {
        LogicVec::from_bits_str(s).unwrap()
    }

    /// §11.8.2: 符号付きの拡張は符号ビット (X/Z を含む) を複製する。
    #[test]
    fn resize_11_8_2() {
        assert_eq!(lv("1010").resize(8, true).to_bits_string(), "11111010");
        assert_eq!(lv("1010").resize(8, false).to_bits_string(), "00001010");
        assert_eq!(lv("x010").resize(8, true).to_bits_string(), "xxxxx010");
        assert_eq!(lv("z010").resize(8, true).to_bits_string(), "zzzzz010");
        assert_eq!(lv("z010").resize(8, false).to_bits_string(), "0000z010");
        assert_eq!(lv("1x0z0110").resize(4, true).to_bits_string(), "0110");
        assert_eq!(lv("x").resize(129, true), LogicVec::filled(129, Logic::X));
        assert_eq!(
            resize_l64(L64::new(0b1010, 0b1000), 4, 8, true),
            L64::new(0xFA, 0xF8)
        );
        assert_eq!(resize_l64(L64::new(!0, 0), 64, 3, true), L64::new(7, 0));
        assert_eq!(LogicVec::zero(0).resize(5, true), LogicVec::zero(5));
    }
}
