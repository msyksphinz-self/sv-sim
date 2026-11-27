//! ビット演算 `~ & | ^ ~^` (IEEE 1800-2023 §11.4.8)。タスク M0-T4。
//!
//! 全オペランドと結果が同じ幅の文脈決定 (CD) の演算。式は plan.md §3.4 の表のとおり
//! (16 通りの入力で §11.4.8 の表と一致することを確認済み)。z の入力は x と同じに扱うので、
//! 結果に z は出ない。
//!
//! | 層 | 4 値 | 2 値 |
//! |---|---|---|
//! | 層 1 (幅 ≤ 64) | [`not_l64`] [`and_l64`] [`or_l64`] [`xor_l64`] [`xnor_l64`] | [`not2_l64`] [`and2_l64`] [`or2_l64`] [`xor2_l64`] [`xnor2_l64`] |
//! | 層 2 (任意幅) | [`not`] [`and`] [`or`] [`xor`] [`xnor`] と `_assign` 版 | [`not2`] [`and2`] [`or2`] [`xor2`] [`xnor2`] と `_assign` 版 |
//! | 層 3 | `!&x` (`std::ops::Not`)、`LogicVec::{and, or, xor, xnor}` と `_assign` 版 (`not_assign` も) | `!&x`、`BitVec::{and, or, xor, xnor}` と `_assign` 版 (`not_assign` も) |

use crate::ops::{check_nonzero_width, check_same_width};
use crate::scalar::{L64, mask};
use crate::vec::{BitVec, LogicVec};
use crate::view::{BitMut, BitRef, LogicMut, LogicRef};

// ===========================================================================
// ワード単位の式 (層 1 と層 2 で共有する。plan.md §3.4)
// ===========================================================================

/// `~x` の 1 ワード分 (マスク前)。
#[inline(always)]
const fn not_w(ax: u64, bx: u64) -> (u64, u64) {
    (!ax | bx, bx)
}

/// `x & y` の 1 ワード分。0 が勝つ。
#[inline(always)]
const fn and_w(ax: u64, bx: u64, ay: u64, by: u64) -> (u64, u64) {
    let a = (ax | bx) & (ay | by);
    (a, a & (bx | by))
}

/// `x | y` の 1 ワード分。1 が勝つ。
#[inline(always)]
const fn or_w(ax: u64, bx: u64, ay: u64, by: u64) -> (u64, u64) {
    let a = ax | bx | ay | by;
    let one = (ax & !bx) | (ay & !by);
    (a, (bx | by) & !one)
}

/// `x ^ y` の 1 ワード分。
#[inline(always)]
const fn xor_w(ax: u64, bx: u64, ay: u64, by: u64) -> (u64, u64) {
    let b = bx | by;
    ((ax ^ ay) | b, b)
}

/// `x ~^ y` の 1 ワード分 (マスク前)。
#[inline(always)]
const fn xnor_w(ax: u64, bx: u64, ay: u64, by: u64) -> (u64, u64) {
    let b = bx | by;
    (!(ax ^ ay) | b, b)
}

// ===========================================================================
// 層 1: 4 値
// ===========================================================================

/// `~x` (§11.4.8)。幅 `w` (1〜64) でマスクする。
#[inline]
pub const fn not_l64(x: L64, w: u32) -> L64 {
    let (a, b) = not_w(x.a, x.b);
    L64::new(a, b).masked(w)
}

/// `x & y` (§11.4.8)。
#[inline]
pub const fn and_l64(x: L64, y: L64, w: u32) -> L64 {
    let (a, b) = and_w(x.a, x.b, y.a, y.b);
    L64::new(a, b).masked(w)
}

/// `x | y` (§11.4.8)。
#[inline]
pub const fn or_l64(x: L64, y: L64, w: u32) -> L64 {
    let (a, b) = or_w(x.a, x.b, y.a, y.b);
    L64::new(a, b).masked(w)
}

/// `x ^ y` (§11.4.8)。
#[inline]
pub const fn xor_l64(x: L64, y: L64, w: u32) -> L64 {
    let (a, b) = xor_w(x.a, x.b, y.a, y.b);
    L64::new(a, b).masked(w)
}

/// `x ~^ y` / `x ^~ y` (§11.4.8)。
#[inline]
pub const fn xnor_l64(x: L64, y: L64, w: u32) -> L64 {
    let (a, b) = xnor_w(x.a, x.b, y.a, y.b);
    L64::new(a, b).masked(w)
}

// ===========================================================================
// 層 1: 2 値
// ===========================================================================

/// 2 値の `~x` (§11.4.8)。
#[inline]
pub const fn not2_l64(x: u64, w: u32) -> u64 {
    !x & mask(w)
}

/// 2 値の `x & y` (§11.4.8)。
#[inline]
pub const fn and2_l64(x: u64, y: u64, w: u32) -> u64 {
    x & y & mask(w)
}

/// 2 値の `x | y` (§11.4.8)。
#[inline]
pub const fn or2_l64(x: u64, y: u64, w: u32) -> u64 {
    (x | y) & mask(w)
}

/// 2 値の `x ^ y` (§11.4.8)。
#[inline]
pub const fn xor2_l64(x: u64, y: u64, w: u32) -> u64 {
    (x ^ y) & mask(w)
}

/// 2 値の `x ~^ y` (§11.4.8)。
#[inline]
pub const fn xnor2_l64(x: u64, y: u64, w: u32) -> u64 {
    !(x ^ y) & mask(w)
}

// ===========================================================================
// 層 2: 4 値
// ===========================================================================

/// 2 項演算をワードごとに適用して `dst` に書き、正規化する。
#[inline(always)]
fn map2(
    mut dst: LogicMut<'_>,
    x: LogicRef<'_>,
    y: LogicRef<'_>,
    f: impl Fn(u64, u64, u64, u64) -> (u64, u64),
) {
    {
        let (da, db) = dst.planes_mut();
        let it = da
            .iter_mut()
            .zip(db.iter_mut())
            .zip(x.aval().iter().zip(x.bval()))
            .zip(y.aval().iter().zip(y.bval()));
        for (((da, db), (&ax, &bx)), (&ay, &by)) in it {
            let (a, b) = f(ax, bx, ay, by);
            *da = a;
            *db = b;
        }
    }
    dst.normalize();
}

/// 2 項演算を in-place (`dst_x op= y`) でワードごとに適用する。
#[inline(always)]
fn map2_assign(
    mut dst_x: LogicMut<'_>,
    y: LogicRef<'_>,
    f: impl Fn(u64, u64, u64, u64) -> (u64, u64),
) {
    {
        let (da, db) = dst_x.planes_mut();
        let it = da
            .iter_mut()
            .zip(db.iter_mut())
            .zip(y.aval().iter().zip(y.bval()));
        for ((da, db), (&ay, &by)) in it {
            let (a, b) = f(*da, *db, ay, by);
            *da = a;
            *db = b;
        }
    }
    dst_x.normalize();
}

/// `dst = ~x` (§11.4.8)。
///
/// # Panics
/// `dst` と `x` の幅が違う場合。
pub fn not(mut dst: LogicMut<'_>, x: LogicRef<'_>) {
    check_same_width("not", dst.width(), x.width());
    {
        let (da, db) = dst.planes_mut();
        let it = da
            .iter_mut()
            .zip(db.iter_mut())
            .zip(x.aval().iter().zip(x.bval()));
        for ((da, db), (&ax, &bx)) in it {
            let (a, b) = not_w(ax, bx);
            *da = a;
            *db = b;
        }
    }
    dst.normalize();
}

/// `dst = ~dst` (§11.4.8)。
pub fn not_assign(mut dst: LogicMut<'_>) {
    check_nonzero_width("not_assign", dst.width());
    {
        let (da, db) = dst.planes_mut();
        for (da, db) in da.iter_mut().zip(db.iter_mut()) {
            let (a, b) = not_w(*da, *db);
            *da = a;
            *db = b;
        }
    }
    dst.normalize();
}

macro_rules! binop4 {
    ($name:ident, $assign:ident, $w:ident, $sym:literal) => {
        #[doc = concat!("`dst = x ", $sym, " y` (§11.4.8)。")]
        ///
        /// # Panics
        /// `dst`, `x`, `y` の幅が揃っていない場合。
        pub fn $name(dst: LogicMut<'_>, x: LogicRef<'_>, y: LogicRef<'_>) {
            check_same_width(stringify!($name), dst.width(), x.width());
            check_same_width(stringify!($name), x.width(), y.width());
            map2(dst, x, y, $w);
        }

        #[doc = concat!("`dst_x = dst_x ", $sym, " y` (§11.4.8)。")]
        ///
        /// # Panics
        /// `dst_x` と `y` の幅が違う場合。
        pub fn $assign(dst_x: LogicMut<'_>, y: LogicRef<'_>) {
            check_same_width(stringify!($assign), dst_x.width(), y.width());
            map2_assign(dst_x, y, $w);
        }
    };
}

binop4!(and, and_assign, and_w, "&");
binop4!(or, or_assign, or_w, "|");
binop4!(xor, xor_assign, xor_w, "^");
binop4!(xnor, xnor_assign, xnor_w, "~^");

// ===========================================================================
// 層 2: 2 値
// ===========================================================================

/// `dst = ~x` (2 値、§11.4.8)。
///
/// # Panics
/// `dst` と `x` の幅が違う場合。
pub fn not2(mut dst: BitMut<'_>, x: BitRef<'_>) {
    check_same_width("not2", dst.width(), x.width());
    for (d, &s) in dst.words_mut().iter_mut().zip(x.words()) {
        *d = !s;
    }
    dst.normalize();
}

/// `dst = ~dst` (2 値、§11.4.8)。
pub fn not_assign2(mut dst: BitMut<'_>) {
    check_nonzero_width("not_assign2", dst.width());
    for d in dst.words_mut() {
        *d = !*d;
    }
    dst.normalize();
}

macro_rules! binop2 {
    ($name:ident, $assign:ident, $sym:literal, |$x:ident, $y:ident| $e:expr) => {
        #[doc = concat!("`dst = x ", $sym, " y` (2 値、§11.4.8)。")]
        ///
        /// # Panics
        /// `dst`, `x`, `y` の幅が揃っていない場合。
        pub fn $name(mut dst: BitMut<'_>, x: BitRef<'_>, y: BitRef<'_>) {
            check_same_width(stringify!($name), dst.width(), x.width());
            check_same_width(stringify!($name), x.width(), y.width());
            for ((d, &$x), &$y) in dst.words_mut().iter_mut().zip(x.words()).zip(y.words()) {
                *d = $e;
            }
            dst.normalize();
        }

        #[doc = concat!("`dst_x = dst_x ", $sym, " y` (2 値、§11.4.8)。")]
        ///
        /// # Panics
        /// `dst_x` と `y` の幅が違う場合。
        pub fn $assign(mut dst_x: BitMut<'_>, y: BitRef<'_>) {
            check_same_width(stringify!($assign), dst_x.width(), y.width());
            for (d, &$y) in dst_x.words_mut().iter_mut().zip(y.words()) {
                let $x = *d;
                *d = $e;
            }
            dst_x.normalize();
        }
    };
}

binop2!(and2, and_assign2, "&", |x, y| x & y);
binop2!(or2, or_assign2, "|", |x, y| x | y);
binop2!(xor2, xor_assign2, "^", |x, y| x ^ y);
binop2!(xnor2, xnor_assign2, "~^", |x, y| !(x ^ y));

// ===========================================================================
// 層 3
// ===========================================================================

macro_rules! vec_binop {
    ($ty:ident, $name:ident, $assign:ident, $k:ident, $ka:ident, $sym:literal) => {
        #[doc = concat!("`self ", $sym, " y` (§11.4.8)。幅は同じであること。")]
        ///
        /// # Panics
        /// 幅が違う場合。
        pub fn $name(&self, y: &$ty) -> $ty {
            let mut r = $ty::zero(self.width());
            $k(r.as_mut(), self.as_ref(), y.as_ref());
            r
        }

        #[doc = concat!("`self = self ", $sym, " y` (§11.4.8)。")]
        ///
        /// # Panics
        /// 幅が違う場合。
        pub fn $assign(&mut self, y: &$ty) {
            $ka(self.as_mut(), y.as_ref());
        }
    };
}

/// `~x` (§11.4.8)。`!` 演算子で書く (`!&x` は `x` を残す)。
impl core::ops::Not for &LogicVec {
    type Output = LogicVec;
    fn not(self) -> LogicVec {
        let mut r = LogicVec::zero(self.width());
        not(r.as_mut(), self.as_ref());
        r
    }
}

/// `~x` (§11.4.8)。in-place で反転する。
impl core::ops::Not for LogicVec {
    type Output = LogicVec;
    fn not(mut self) -> LogicVec {
        not_assign(self.as_mut());
        self
    }
}

/// `~x` (2 値、§11.4.8)。
impl core::ops::Not for &BitVec {
    type Output = BitVec;
    fn not(self) -> BitVec {
        let mut r = BitVec::zero(self.width());
        not2(r.as_mut(), self.as_ref());
        r
    }
}

/// `~x` (2 値、§11.4.8)。in-place で反転する。
impl core::ops::Not for BitVec {
    type Output = BitVec;
    fn not(mut self) -> BitVec {
        not_assign2(self.as_mut());
        self
    }
}

impl LogicVec {
    /// `self = ~self` (§11.4.8)。
    pub fn not_assign(&mut self) {
        not_assign(self.as_mut());
    }

    vec_binop!(LogicVec, and, and_assign, and, and_assign, "&");
    vec_binop!(LogicVec, or, or_assign, or, or_assign, "|");
    vec_binop!(LogicVec, xor, xor_assign, xor, xor_assign, "^");
    vec_binop!(LogicVec, xnor, xnor_assign, xnor, xnor_assign, "~^");
}

impl BitVec {
    /// `self = ~self` (2 値、§11.4.8)。
    pub fn not_assign(&mut self) {
        not_assign2(self.as_mut());
    }

    vec_binop!(BitVec, and, and_assign, and2, and_assign2, "&");
    vec_binop!(BitVec, or, or_assign, or2, or_assign2, "|");
    vec_binop!(BitVec, xor, xor_assign, xor2, xor_assign2, "^");
    vec_binop!(BitVec, xnor, xnor_assign, xnor2, xnor_assign2, "~^");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Logic;

    fn l1(v: Logic) -> L64 {
        L64::filled(v, 1)
    }

    /// §11.4.8 の表 (1 ビット × 16 通り)。添字は 0, 1, x, z の順。
    #[test]
    fn tables_11_4_8() {
        use Logic::{L0 as O, L1 as I, X, Z};
        let order = [O, I, X, Z];
        let and_t = [[O, O, O, O], [O, I, X, X], [O, X, X, X], [O, X, X, X]];
        let or_t = [[O, I, X, X], [I, I, I, I], [X, I, X, X], [X, I, X, X]];
        let xor_t = [[O, I, X, X], [I, O, X, X], [X, X, X, X], [X, X, X, X]];
        let not_t = [I, O, X, X];
        for (i, &x) in order.iter().enumerate() {
            assert_eq!(not_l64(l1(x), 1).bit(0), not_t[i], "~{x}");
            for (j, &y) in order.iter().enumerate() {
                assert_eq!(and_l64(l1(x), l1(y), 1).bit(0), and_t[i][j], "{x}&{y}");
                assert_eq!(or_l64(l1(x), l1(y), 1).bit(0), or_t[i][j], "{x}|{y}");
                assert_eq!(xor_l64(l1(x), l1(y), 1).bit(0), xor_t[i][j], "{x}^{y}");
                let xn = xnor_l64(l1(x), l1(y), 1).bit(0);
                let want = match xor_t[i][j] {
                    O => I,
                    I => O,
                    v => v,
                };
                assert_eq!(xn, want, "{x}~^{y}");
            }
        }
    }

    #[test]
    fn slice_matches_scalar_and_normalizes() {
        let x = LogicVec::from_bits_str("01x1z0").unwrap();
        let y = LogicVec::from_bits_str("1z0x10").unwrap();
        let n = !&x;
        assert_eq!(n.to_bits_string(), "10x0x1");
        assert!(n.as_ref().is_normalized());
        assert_eq!(x.and(&y).to_bits_string(), "0x0xx0");
        assert_eq!(x.or(&y).to_bits_string(), "11x110");
        let mut z = x.clone();
        z.xnor_assign(&y);
        assert_eq!(z, x.xnor(&y));
        assert!(z.as_ref().is_normalized());
    }

    #[test]
    fn two_state() {
        let x = BitVec::from_u64(8, 0xf0);
        let y = BitVec::from_u64(8, 0x3c);
        assert_eq!(x.and(&y).to_u64(), Some(0x30));
        assert_eq!(x.or(&y).to_u64(), Some(0xfc));
        assert_eq!(x.xor(&y).to_u64(), Some(0xcc));
        assert_eq!(x.xnor(&y).to_u64(), Some(0x33));
        assert_eq!((!&x).to_u64(), Some(0x0f));
        assert_eq!((!x.clone()).to_u64(), Some(0x0f));
        assert_eq!(xnor2_l64(0xf0, 0x3c, 8), 0x33);
        assert_eq!(not2_l64(0, 65), u64::MAX);
    }

    #[test]
    #[should_panic(expected = "and: width mismatch (8 vs 9)")]
    fn width_mismatch_panics() {
        let _ = LogicVec::zero(8).and(&LogicVec::zero(9));
    }
}
