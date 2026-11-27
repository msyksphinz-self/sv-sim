//! シフト演算と `shl_bits` 等の混在カーネル (IEEE 1800-2023 §11.4.10、plan.md §3.5(d)、§3.5.1)。
//!
//! 規則 (§11.4.10):
//! - `<<` と `<<<` は同じ。空いたビットは 0。
//! - `>>` は空いたビットを 0 で埋める。
//! - `>>>` は、結果の型が符号付きなら符号ビットの値 (X/Z を含めてそのまま) で埋め、符号なしなら
//!   `>>` と同じ。符号ビットが Z なら Z で埋める [要VCS確認 V-3]。
//! - シフト量は自己決定で、**常に符号なし**として扱う。シフト量に X/Z があれば結果は全 X。
//! - シフト量が幅 `w` 以上なら、`<<` `>>` は全 0、`>>>` (符号付き) は全ビットが符号ビットの値。
//!   シフト量が 64 ビットより広い場合も、上位ワードに 1 があれば「`w` 以上」として扱う (切り詰めない)。
//! - データの X/Z は、そのままの位置関係で移動する (データを移すだけの演算、plan.md §3.4)。
//!
//! 関数の一覧:
//!
//! | 層 | 4 値 | 2 値 | 混在 (2 値データ × 4 値シフト量、§3.5.1) |
//! |---|---|---|---|
//! | 1 | [`shl_l64`] [`shl_const_l64`] … | [`shl2_l64`] … | [`shl_bits_l64`] … |
//! | 2 | [`shl`] [`shl_const`] [`shl_assign`] [`shl_const_assign`] … | [`shl2`] [`shl_const2`] [`shl_assign2`] … | [`shl_bits`] [`shr_bits`] [`ashr_bits`] |
//! | 3 | [`LogicVec::shl`] … | [`BitVec::shl`] … | — |
//!
//! ビット列の補助 (`read_bits` 等) とワード列のシフト (`words::shl` 等) は `words.rs` (crate 内専用) にある
//! (M0-T14 で統合)。

use crate::layout::normalize_plane;
use crate::logic::Logic;
use crate::scalar::{L64, mask};
use crate::vec::{BitVec, LogicVec};
use crate::view::{BitMut, BitRef, LogicMut, LogicRef};
use crate::words::{self, fill_bits, sign_bit};

// ===========================================================================
// シフト量
// ===========================================================================

/// 4 値のシフト量を読む (§11.4.10、常に符号なし)。X/Z があれば `None`。
/// `u64` に収まらない値は `u64::MAX` に飽和させる (どのみち幅以上になる)。
#[inline]
fn amount(amt: LogicRef<'_>) -> Option<u64> {
    if amt.has_unknown() {
        None
    } else {
        Some(amount2(amt.aval()))
    }
}

/// 2 値のシフト量 (ワード列) を読む。`u64` に収まらなければ `u64::MAX`。
#[inline]
fn amount2(words: &[u64]) -> u64 {
    match words.split_first() {
        None => 0,
        Some((&lo, rest)) => {
            if words::is_zero(rest) {
                lo
            } else {
                u64::MAX
            }
        }
    }
}

// ===========================================================================
// プレーン単位のシフト
// ===========================================================================

/// プレーン `d = s << n` (幅 `width`、`s` は正規化済み)。空いたビットは 0。1 パスで書く。
fn shl_plane(d: &mut [u64], s: &[u64], width: u32, n: u64) {
    if n >= width as u64 {
        d.fill(0);
        return;
    }
    words::shl(d, s, n);
    normalize_plane(width, d);
}

/// [`shl_plane`] の in-place 版。
fn shl_plane_assign(w: &mut [u64], width: u32, n: u64) {
    if n >= width as u64 {
        w.fill(0);
        return;
    }
    words::shl_assign(w, n);
    normalize_plane(width, w);
}

/// 右シフトの後始末: `n >= width` なら全体を `fill` にし、そうでなければ空いた上位 `n` ビットを
/// `fill` で埋める。最後に正規化する。
fn shr_plane_finish(w: &mut [u64], width: u32, n: u64, fill: bool) {
    if n >= width as u64 {
        w.fill(if fill { !0 } else { 0 });
    } else if fill {
        fill_bits(w, (width as u64 - n) as usize, width as usize, true);
    }
    normalize_plane(width, w);
}

/// プレーン `d = s >> n` (幅 `width`、`s` は正規化済み) で、空いた上位ビットを `fill` で埋める。
/// 1 パスで書く。
fn shr_plane(d: &mut [u64], s: &[u64], width: u32, n: u64, fill: bool) {
    if n < width as u64 {
        words::shr(d, s, n);
    }
    shr_plane_finish(d, width, n, fill);
}

/// [`shr_plane`] の in-place 版。
fn shr_plane_assign(w: &mut [u64], width: u32, n: u64, fill: bool) {
    if n < width as u64 {
        words::shr_assign(w, n);
    }
    shr_plane_finish(w, width, n, fill);
}

#[inline]
fn check_same_width(f: &str, dst: u32, x: u32) {
    assert_eq!(dst, x, "{f}: width mismatch (dst {dst}, x {x})");
    debug_assert!(dst != 0, "{f}: width 0 is only allowed in concat");
}

// ===========================================================================
// 層 2: 4 値
// ===========================================================================

/// 4 値の `dst = x << n` (シフト量は既知の `n`)。1 パスで書く。
fn shl_into(mut dst: LogicMut<'_>, x: LogicRef<'_>, n: u64) {
    let w = dst.width();
    let (a, b) = dst.planes_mut();
    shl_plane(a, x.aval(), w, n);
    shl_plane(b, x.bval(), w, n);
}

/// 4 値の `dst = x >> n`。`arith` が真なら `x` の符号ビットの値 (X/Z もそのまま) で埋める。
fn shr_into(mut dst: LogicMut<'_>, x: LogicRef<'_>, n: u64, arith: bool) {
    let w = dst.width();
    let fa = arith && sign_bit(x.aval(), w);
    let fb = arith && sign_bit(x.bval(), w);
    let (a, b) = dst.planes_mut();
    shr_plane(a, x.aval(), w, n, fa);
    shr_plane(b, x.bval(), w, n, fb);
}

/// 4 値の in-place 左シフト (シフト量は既知の `n`)。
fn shl_in_place(mut d: LogicMut<'_>, n: u64) {
    let w = d.width();
    let (a, b) = d.planes_mut();
    shl_plane_assign(a, w, n);
    shl_plane_assign(b, w, n);
}

/// 4 値の in-place 右シフト。`arith` が真なら符号ビットの値で埋める。
fn shr_in_place(mut d: LogicMut<'_>, n: u64, arith: bool) {
    let w = d.width();
    let (a, b) = d.planes_mut();
    let fa = arith && sign_bit(a, w);
    let fb = arith && sign_bit(b, w);
    shr_plane_assign(a, w, n, fa);
    shr_plane_assign(b, w, n, fb);
}

/// `dst = x << amt` (`<<` と `<<<`、§11.4.10)。`x` と `dst` は同じ幅 (文脈決定)、`amt` は自己決定で
/// 符号なし。`amt` に X/Z があれば全 X。
///
/// # Panics
/// `dst` と `x` の幅が違う場合。
pub fn shl(mut dst: LogicMut<'_>, x: LogicRef<'_>, amt: LogicRef<'_>) {
    check_same_width("shl", dst.width(), x.width());
    match amount(amt) {
        None => dst.fill(Logic::X),
        Some(n) => shl_into(dst, x, n),
    }
}

/// `dst = x >> amt` (§11.4.10)。空いたビットは 0。`amt` に X/Z があれば全 X。
///
/// # Panics
/// `dst` と `x` の幅が違う場合。
pub fn shr(mut dst: LogicMut<'_>, x: LogicRef<'_>, amt: LogicRef<'_>) {
    check_same_width("shr", dst.width(), x.width());
    match amount(amt) {
        None => dst.fill(Logic::X),
        Some(n) => shr_into(dst, x, n, false),
    }
}

/// `dst = x >>> amt` (§11.4.10)。`signed` (結果の型が符号付き) なら符号ビットの値 (X/Z もそのまま)
/// で埋める。符号なしなら [`shr`] と同じ。`amt` に X/Z があれば全 X。
///
/// # Panics
/// `dst` と `x` の幅が違う場合。
pub fn ashr(mut dst: LogicMut<'_>, x: LogicRef<'_>, amt: LogicRef<'_>, signed: bool) {
    check_same_width("ashr", dst.width(), x.width());
    match amount(amt) {
        None => dst.fill(Logic::X),
        Some(n) => shr_into(dst, x, n, signed),
    }
}

/// `dst = x << n` (シフト量が定数の版)。
///
/// # Panics
/// `dst` と `x` の幅が違う場合。
pub fn shl_const(dst: LogicMut<'_>, x: LogicRef<'_>, n: u64) {
    check_same_width("shl_const", dst.width(), x.width());
    shl_into(dst, x, n);
}

/// `dst = x >> n` (シフト量が定数の版)。
///
/// # Panics
/// `dst` と `x` の幅が違う場合。
pub fn shr_const(dst: LogicMut<'_>, x: LogicRef<'_>, n: u64) {
    check_same_width("shr_const", dst.width(), x.width());
    shr_into(dst, x, n, false);
}

/// `dst = x >>> n` (シフト量が定数の版)。
///
/// # Panics
/// `dst` と `x` の幅が違う場合。
pub fn ashr_const(dst: LogicMut<'_>, x: LogicRef<'_>, n: u64, signed: bool) {
    check_same_width("ashr_const", dst.width(), x.width());
    shr_into(dst, x, n, signed);
}

/// `x <<= amt` (in-place 版)。
pub fn shl_assign(mut x: LogicMut<'_>, amt: LogicRef<'_>) {
    match amount(amt) {
        None => x.fill(Logic::X),
        Some(n) => shl_in_place(x, n),
    }
}

/// `x >>= amt` (in-place 版)。
pub fn shr_assign(mut x: LogicMut<'_>, amt: LogicRef<'_>) {
    match amount(amt) {
        None => x.fill(Logic::X),
        Some(n) => shr_in_place(x, n, false),
    }
}

/// `x >>>= amt` (in-place 版)。
pub fn ashr_assign(mut x: LogicMut<'_>, amt: LogicRef<'_>, signed: bool) {
    match amount(amt) {
        None => x.fill(Logic::X),
        Some(n) => shr_in_place(x, n, signed),
    }
}

/// `x <<= n` (in-place、定数の版)。
pub fn shl_const_assign(x: LogicMut<'_>, n: u64) {
    shl_in_place(x, n);
}

/// `x >>= n` (in-place、定数の版)。
pub fn shr_const_assign(x: LogicMut<'_>, n: u64) {
    shr_in_place(x, n, false);
}

/// `x >>>= n` (in-place、定数の版)。
pub fn ashr_const_assign(x: LogicMut<'_>, n: u64, signed: bool) {
    shr_in_place(x, n, signed);
}

// ===========================================================================
// 層 2: 2 値
// ===========================================================================

/// 2 値の `dst = x >> n` (1 パス)。`arith` が真なら `x` の符号ビットで埋める。
fn shr2_into(mut dst: BitMut<'_>, x: BitRef<'_>, n: u64, arith: bool) {
    let w = dst.width();
    let f = arith && sign_bit(x.words(), w);
    shr_plane(dst.words_mut(), x.words(), w, n, f);
}

/// 2 値の `dst = x << n` (1 パス)。
fn shl2_into(mut dst: BitMut<'_>, x: BitRef<'_>, n: u64) {
    let w = dst.width();
    shl_plane(dst.words_mut(), x.words(), w, n);
}

fn shr2_in_place(mut d: BitMut<'_>, n: u64, arith: bool) {
    let w = d.width();
    let a = d.words_mut();
    let f = arith && sign_bit(a, w);
    shr_plane_assign(a, w, n, f);
}

fn shl2_in_place(mut d: BitMut<'_>, n: u64) {
    let w = d.width();
    shl_plane_assign(d.words_mut(), w, n);
}

/// 2 値の `dst = x << amt` (§11.4.10)。
///
/// # Panics
/// `dst` と `x` の幅が違う場合。
pub fn shl2(dst: BitMut<'_>, x: BitRef<'_>, amt: BitRef<'_>) {
    check_same_width("shl2", dst.width(), x.width());
    shl2_into(dst, x, amount2(amt.words()));
}

/// 2 値の `dst = x >> amt` (§11.4.10)。
///
/// # Panics
/// `dst` と `x` の幅が違う場合。
pub fn shr2(dst: BitMut<'_>, x: BitRef<'_>, amt: BitRef<'_>) {
    check_same_width("shr2", dst.width(), x.width());
    shr2_into(dst, x, amount2(amt.words()), false);
}

/// 2 値の `dst = x >>> amt` (§11.4.10)。
///
/// # Panics
/// `dst` と `x` の幅が違う場合。
pub fn ashr2(dst: BitMut<'_>, x: BitRef<'_>, amt: BitRef<'_>, signed: bool) {
    check_same_width("ashr2", dst.width(), x.width());
    shr2_into(dst, x, amount2(amt.words()), signed);
}

/// 2 値の `dst = x << n`。
///
/// # Panics
/// `dst` と `x` の幅が違う場合。
pub fn shl_const2(dst: BitMut<'_>, x: BitRef<'_>, n: u64) {
    check_same_width("shl_const2", dst.width(), x.width());
    shl2_into(dst, x, n);
}

/// 2 値の `dst = x >> n`。
///
/// # Panics
/// `dst` と `x` の幅が違う場合。
pub fn shr_const2(dst: BitMut<'_>, x: BitRef<'_>, n: u64) {
    check_same_width("shr_const2", dst.width(), x.width());
    shr2_into(dst, x, n, false);
}

/// 2 値の `dst = x >>> n`。
///
/// # Panics
/// `dst` と `x` の幅が違う場合。
pub fn ashr_const2(dst: BitMut<'_>, x: BitRef<'_>, n: u64, signed: bool) {
    check_same_width("ashr_const2", dst.width(), x.width());
    shr2_into(dst, x, n, signed);
}

/// 2 値の `x <<= amt`。
pub fn shl_assign2(x: BitMut<'_>, amt: BitRef<'_>) {
    shl2_in_place(x, amount2(amt.words()));
}

/// 2 値の `x >>= amt`。
pub fn shr_assign2(x: BitMut<'_>, amt: BitRef<'_>) {
    shr2_in_place(x, amount2(amt.words()), false);
}

/// 2 値の `x >>>= amt`。
pub fn ashr_assign2(x: BitMut<'_>, amt: BitRef<'_>, signed: bool) {
    shr2_in_place(x, amount2(amt.words()), signed);
}

/// 2 値の `x <<= n`。
pub fn shl_const_assign2(x: BitMut<'_>, n: u64) {
    shl2_in_place(x, n);
}

/// 2 値の `x >>= n`。
pub fn shr_const_assign2(x: BitMut<'_>, n: u64) {
    shr2_in_place(x, n, false);
}

/// 2 値の `x >>>= n`。
pub fn ashr_const_assign2(x: BitMut<'_>, n: u64, signed: bool) {
    shr2_in_place(x, n, signed);
}

// ===========================================================================
// 層 2: 混在 (2 値データ × 4 値シフト量 → 4 値結果、plan.md §3.5.1)
// ===========================================================================

/// 2 値の `x` をずらして 4 値の `dst` に書く (bval = 0)。`shr` が偽なら左に、真なら右に `n` ビット
/// ずらし、右シフトで空いた上位ビットは `fill` で埋める。1 パスで書く。
fn shift_bits_into(dst: &mut LogicMut<'_>, x: BitRef<'_>, n: u64, shr: bool, fill: bool) {
    let w = dst.width();
    let (a, b) = dst.planes_mut();
    if shr {
        shr_plane(a, x.words(), w, n, fill);
    } else {
        shl_plane(a, x.words(), w, n);
    }
    b.fill(0);
}

/// `dst = x << amt` で、`x` が 2 値、`amt` が 4 値の場合 (plan.md §3.5.1 規則 1)。結果は 4 値で、
/// `amt` に X/Z があれば全 X、無ければ bval = 0 のシフト結果。
///
/// # Panics
/// `dst` と `x` の幅が違う場合。
pub fn shl_bits(mut dst: LogicMut<'_>, x: BitRef<'_>, amt: LogicRef<'_>) {
    check_same_width("shl_bits", dst.width(), x.width());
    match amount(amt) {
        None => dst.fill(Logic::X),
        Some(n) => {
            shift_bits_into(&mut dst, x, n, false, false);
        }
    }
}

/// `dst = x >> amt` で、`x` が 2 値、`amt` が 4 値の場合 (plan.md §3.5.1)。
///
/// # Panics
/// `dst` と `x` の幅が違う場合。
pub fn shr_bits(mut dst: LogicMut<'_>, x: BitRef<'_>, amt: LogicRef<'_>) {
    check_same_width("shr_bits", dst.width(), x.width());
    match amount(amt) {
        None => dst.fill(Logic::X),
        Some(n) => {
            shift_bits_into(&mut dst, x, n, true, false);
        }
    }
}

/// `dst = x >>> amt` で、`x` が 2 値、`amt` が 4 値の場合 (plan.md §3.5.1)。
///
/// # Panics
/// `dst` と `x` の幅が違う場合。
pub fn ashr_bits(mut dst: LogicMut<'_>, x: BitRef<'_>, amt: LogicRef<'_>, signed: bool) {
    check_same_width("ashr_bits", dst.width(), x.width());
    match amount(amt) {
        None => dst.fill(Logic::X),
        Some(n) => {
            let f = signed && sign_bit(x.words(), x.width());
            shift_bits_into(&mut dst, x, n, true, f);
        }
    }
}

// ===========================================================================
// 層 1: スカラ (幅 1〜64)
// ===========================================================================

/// 幅 `w` の値の上位 `n` ビット (`n < w`) のマスク。
#[inline]
const fn top_n_mask(w: u32, n: u64) -> u64 {
    let m = mask(w);
    m ^ (m >> n)
}

/// 層 1 の `x << n` (幅 `w`、`1 <= w <= 64`)。`n >= w` なら 0。
#[inline]
pub const fn shl_const_l64(x: L64, w: u32, n: u64) -> L64 {
    if n >= w as u64 {
        L64::ZERO
    } else {
        let m = mask(w);
        L64 {
            a: (x.a << n) & m,
            b: (x.b << n) & m,
        }
    }
}

/// 層 1 の `x >> n` (幅 `w`)。`n >= w` なら 0。
#[inline]
pub const fn shr_const_l64(x: L64, w: u32, n: u64) -> L64 {
    if n >= w as u64 {
        L64::ZERO
    } else {
        L64 {
            a: x.a >> n,
            b: x.b >> n,
        }
    }
}

/// 層 1 の `x >>> n` (幅 `w`)。`signed` なら符号ビット (ビット `w - 1`) の値で埋める。
#[inline]
pub const fn ashr_const_l64(x: L64, w: u32, n: u64, signed: bool) -> L64 {
    if !signed {
        return shr_const_l64(x, w, n);
    }
    let sa = (x.a >> (w - 1)) & 1 != 0;
    let sb = (x.b >> (w - 1)) & 1 != 0;
    let (base, fm) = if n >= w as u64 {
        (L64::ZERO, mask(w))
    } else {
        (
            L64 {
                a: x.a >> n,
                b: x.b >> n,
            },
            top_n_mask(w, n),
        )
    };
    L64 {
        a: base.a | if sa { fm } else { 0 },
        b: base.b | if sb { fm } else { 0 },
    }
}

/// 層 1 の `x << amt`。`amt` は幅 64 以下の 4 値のシフト量 (正規化済み)。X/Z があれば全 X。
/// 64 ビットより広いシフト量は、層 2 を使うか、呼び出し側で「幅以上」に読み替える。
#[inline]
pub const fn shl_l64(x: L64, w: u32, amt: L64) -> L64 {
    if amt.b != 0 {
        L64::all_x(w)
    } else {
        shl_const_l64(x, w, amt.a)
    }
}

/// 層 1 の `x >> amt`。
#[inline]
pub const fn shr_l64(x: L64, w: u32, amt: L64) -> L64 {
    if amt.b != 0 {
        L64::all_x(w)
    } else {
        shr_const_l64(x, w, amt.a)
    }
}

/// 層 1 の `x >>> amt`。
#[inline]
pub const fn ashr_l64(x: L64, w: u32, amt: L64, signed: bool) -> L64 {
    if amt.b != 0 {
        L64::all_x(w)
    } else {
        ashr_const_l64(x, w, amt.a, signed)
    }
}

/// 層 1 の 2 値 `x << n` (幅 `w`)。
#[inline]
pub const fn shl2_l64(x: u64, w: u32, n: u64) -> u64 {
    if n >= w as u64 { 0 } else { (x << n) & mask(w) }
}

/// 層 1 の 2 値 `x >> n` (幅 `w`)。
#[inline]
pub const fn shr2_l64(x: u64, w: u32, n: u64) -> u64 {
    if n >= w as u64 { 0 } else { x >> n }
}

/// 層 1 の 2 値 `x >>> n` (幅 `w`)。
#[inline]
pub const fn ashr2_l64(x: u64, w: u32, n: u64, signed: bool) -> u64 {
    ashr_const_l64(L64 { a: x, b: 0 }, w, n, signed).a
}

/// 層 1 の混在 `x << amt` (2 値データ、4 値シフト量 → 4 値結果)。
#[inline]
pub const fn shl_bits_l64(x: u64, w: u32, amt: L64) -> L64 {
    shl_l64(L64 { a: x, b: 0 }, w, amt)
}

/// 層 1 の混在 `x >> amt`。
#[inline]
pub const fn shr_bits_l64(x: u64, w: u32, amt: L64) -> L64 {
    shr_l64(L64 { a: x, b: 0 }, w, amt)
}

/// 層 1 の混在 `x >>> amt`。
#[inline]
pub const fn ashr_bits_l64(x: u64, w: u32, amt: L64, signed: bool) -> L64 {
    ashr_l64(L64 { a: x, b: 0 }, w, amt, signed)
}

// ===========================================================================
// 層 3
// ===========================================================================

impl LogicVec {
    /// `self << amt` (§11.4.10)。結果の幅は `self` の幅。
    pub fn shl(&self, amt: &LogicVec) -> LogicVec {
        let mut r = LogicVec::zero(self.width());
        shl(r.as_mut(), self.as_ref(), amt.as_ref());
        r
    }

    /// `self >> amt` (§11.4.10)。
    pub fn shr(&self, amt: &LogicVec) -> LogicVec {
        let mut r = LogicVec::zero(self.width());
        shr(r.as_mut(), self.as_ref(), amt.as_ref());
        r
    }

    /// `self >>> amt` (§11.4.10)。`signed` は結果の型 (= `self` の型) が符号付きか。
    pub fn ashr(&self, amt: &LogicVec, signed: bool) -> LogicVec {
        let mut r = LogicVec::zero(self.width());
        ashr(r.as_mut(), self.as_ref(), amt.as_ref(), signed);
        r
    }

    /// `self << n`。
    pub fn shl_const(&self, n: u64) -> LogicVec {
        let mut r = self.clone();
        shl_const_assign(r.as_mut(), n);
        r
    }

    /// `self >> n`。
    pub fn shr_const(&self, n: u64) -> LogicVec {
        let mut r = self.clone();
        shr_const_assign(r.as_mut(), n);
        r
    }

    /// `self >>> n`。
    pub fn ashr_const(&self, n: u64, signed: bool) -> LogicVec {
        let mut r = self.clone();
        ashr_const_assign(r.as_mut(), n, signed);
        r
    }

    /// `self <<= amt`。
    pub fn shl_assign(&mut self, amt: &LogicVec) {
        shl_assign(self.as_mut(), amt.as_ref());
    }

    /// `self >>= amt`。
    pub fn shr_assign(&mut self, amt: &LogicVec) {
        shr_assign(self.as_mut(), amt.as_ref());
    }

    /// `self >>>= amt`。
    pub fn ashr_assign(&mut self, amt: &LogicVec, signed: bool) {
        ashr_assign(self.as_mut(), amt.as_ref(), signed);
    }
}

impl BitVec {
    /// 2 値の `self << amt` (§11.4.10)。
    pub fn shl(&self, amt: &BitVec) -> BitVec {
        let mut r = self.clone();
        shl_assign2(r.as_mut(), amt.as_ref());
        r
    }

    /// 2 値の `self >> amt`。
    pub fn shr(&self, amt: &BitVec) -> BitVec {
        let mut r = self.clone();
        shr_assign2(r.as_mut(), amt.as_ref());
        r
    }

    /// 2 値の `self >>> amt`。
    pub fn ashr(&self, amt: &BitVec, signed: bool) -> BitVec {
        let mut r = self.clone();
        ashr_assign2(r.as_mut(), amt.as_ref(), signed);
        r
    }

    /// 2 値の `self << n`。
    pub fn shl_const(&self, n: u64) -> BitVec {
        let mut r = self.clone();
        shl_const_assign2(r.as_mut(), n);
        r
    }

    /// 2 値の `self >> n`。
    pub fn shr_const(&self, n: u64) -> BitVec {
        let mut r = self.clone();
        shr_const_assign2(r.as_mut(), n);
        r
    }

    /// 2 値の `self >>> n`。
    pub fn ashr_const(&self, n: u64, signed: bool) -> BitVec {
        let mut r = self.clone();
        ashr_const_assign2(r.as_mut(), n, signed);
        r
    }

    /// 混在の `self << amt` (2 値データ × 4 値シフト量 → 4 値結果、plan.md §3.5.1)。
    pub fn shl_logic(&self, amt: &LogicVec) -> LogicVec {
        let mut r = LogicVec::zero(self.width());
        shl_bits(r.as_mut(), self.as_ref(), amt.as_ref());
        r
    }

    /// 混在の `self >> amt` (plan.md §3.5.1)。
    pub fn shr_logic(&self, amt: &LogicVec) -> LogicVec {
        let mut r = LogicVec::zero(self.width());
        shr_bits(r.as_mut(), self.as_ref(), amt.as_ref());
        r
    }

    /// 混在の `self >>> amt` (plan.md §3.5.1)。
    pub fn ashr_logic(&self, amt: &LogicVec, signed: bool) -> LogicVec {
        let mut r = LogicVec::zero(self.width());
        ashr_bits(r.as_mut(), self.as_ref(), amt.as_ref(), signed);
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lv(s: &str) -> LogicVec {
        LogicVec::from_bits_str(s).unwrap()
    }

    /// 1 パスの `dst = x << n` / `>> n` / `>>> n` が、in-place 版 (`_assign`) と一致する
    /// (§11.4.10)。出力先の上位ビット (幅より上) を汚しておき、正規化されることも見る (plan.md §3.2)。
    #[test]
    fn one_pass_matches_in_place() {
        for w in [1u32, 2, 63, 64, 65, 127, 128, 129, 200] {
            let mut x = LogicVec::zero(w);
            for i in 0..w {
                x.set_bit(i, Logic::ALL[((i * 7 + w) % 4) as usize]);
            }
            x.set_bit(w - 1, Logic::Z);
            for n in [0u64, 1, 5, 63, 64, 65, (w - 1) as u64, w as u64, 1000] {
                let nw = crate::words_for(w);
                let mut a = vec![!0u64; nw];
                let mut b = vec![!0u64; nw];
                shl_const(LogicMut::new(w, &mut a, &mut b), x.as_ref(), n);
                let got = LogicVec::from_ref(LogicRef::new(w, &a, &b));
                assert!(LogicRef::new(w, &a, &b).is_normalized(), "shl w={w} n={n}");
                assert_eq!(got, x.shl_const(n), "shl w={w} n={n}");
                for signed in [false, true] {
                    let (mut a, mut b) = (vec![!0u64; nw], vec![!0u64; nw]);
                    ashr_const(LogicMut::new(w, &mut a, &mut b), x.as_ref(), n, signed);
                    assert!(LogicRef::new(w, &a, &b).is_normalized(), "ashr w={w} n={n}");
                    let got = LogicVec::from_ref(LogicRef::new(w, &a, &b));
                    assert_eq!(got, x.ashr_const(n, signed), "ashr w={w} n={n} s={signed}");
                }
            }
        }
    }

    /// §11.4.10 の例: `4'b1001 << 1` = `4'b0010`、`>>>` は符号ビットで埋める。
    #[test]
    fn shift_basic_11_4_10() {
        let x = lv("1001x0z1");
        assert_eq!(x.shl_const(2).to_bits_string(), "01x0z100");
        assert_eq!(x.shr_const(2).to_bits_string(), "001001x0");
        assert_eq!(x.ashr_const(2, true).to_bits_string(), "111001x0");
        assert_eq!(x.ashr_const(2, false).to_bits_string(), "001001x0");
        assert_eq!(lv("z000").ashr_const(1, true).to_bits_string(), "zz00");
        assert_eq!(lv("z000").ashr_const(8, true).to_bits_string(), "zzzz");
        assert_eq!(x.shl(&lv("x01")).to_bits_string(), "xxxxxxxx");
        let l = shl_l64(L64::new(0b1001, 0), 4, L64::new(1, 0));
        assert_eq!(l, L64::new(0b0010, 0));
        assert_eq!(
            ashr_l64(L64::new(0, 0b1000), 4, L64::new(9, 0), true),
            L64::new(0, 0xF)
        );
    }
}
