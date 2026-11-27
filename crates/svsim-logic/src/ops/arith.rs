//! 算術演算 `+ - * / % **`、単項 `-`、`++ --` と混在カーネル `pow_bits`
//! (IEEE 1800-2023 §11.4.2, §11.4.3、plan.md §3.5(c)、§3.5.1)。
//!
//! 規則 (§11.4.3):
//! - オペランドのどこかに X/Z のビットがあれば、結果は全 X (4 値版)。
//! - 結果は幅 `w` の 2 の補数として切り詰める (オーバフローは捨てる)。単項 `+` は恒等なので関数を
//!   用意しない。単項 `-x` は `0 - x`。
//! - `*` は下位 `w` ビットだけを計算する。下位ビットは符号の解釈に依らないので `signed` を取らない。
//! - `/` は 0 方向へ切り捨てる。`%` の結果の符号は第 1 オペランドの符号。除数が 0 なら全 X。
//!   符号付きの最小値 / -1 は 2 の補数で折り返して最小値、剰余は 0 [要VCS確認 V-1]。
//! - `**` の結果の幅は base の幅 (文脈決定)、指数は自己決定で別の幅を取る。指数が 0 なら 1
//!   (base が 0 でも 1)。指数が負 (指数が符号付きで符号ビットが 1) なら、base = 0 で全 X、
//!   base = 1 で 1、base = -1 (base が符号付きのとき) で指数が偶数なら 1・奇数なら -1、
//!   |base| > 1 で 0。
//! - **2 値版**で規格上の結果が X になる場合 (0 除算、`0 ** 負`) は、2 値の型に格納した値 (§6.11.2)
//!   と同じ **0** を返す [要VCS確認 V-2]。
//!
//! 実装 (plan.md §8.3 T5): 4 値の算術は「未知があれば全 X」の判定を先に行い、そのあと aval プレーンに
//! 2 値のワード列の計算 (crate 内の `words`) を適用する。多倍長の除算は Knuth の Algorithm D
//! (`words::divrem_in_place`)、符号付きは絶対値で計算してから符号を戻す (最小値の絶対値は
//! 符号なしの `w` ビットとして扱う)。べき乗は `w` ビットの剰余環の上で二乗法 (左から右) で計算する。
//! base が偶数で指数が `w` 以上なら 0 を返し、base が奇数なら指数の下位 `w` ビットだけを使う
//! (奇数の剰余類の群 (Z/2^w)^× の位数 2^(w-1) が 2^w を割り切るため)。
//!
//! # 作業領域 ([`Scratch`])
//!
//! 公開カーネルは `&mut Scratch` を受け、内部の補助関数には切り出したスライスを渡す。公開カーネル同士を
//! `Scratch` 経由で入れ子にしない (`scratch.rs` のモジュールの説明)。幅 `w` (`nw = words_for(w)`) の
//! 必要量は次のとおりで、どれも [`Scratch::words_needed`] 以下 ([`Scratch::reserve_for`] で足りる)。
//!
//! | カーネル | ワード数 |
//! |---|---|
//! | `mul` `mul2` | 0 |
//! | `mul_assign` `mul_assign2` `pow` `pow2` `pow_bits` | `nw` |
//! | `div` `rem` とその `_assign` 版・2 値版 | `2·nw + 1` |
//!
//! 幅 ≤ 64 (`nw = 1`) では層 1 の式で計算するので、作業領域を使わない。
//!
//! # 関数の一覧
//!
//! | 層 | 4 値 | 2 値 | 混在 (2 値 base × 4 値の指数、§3.5.1) |
//! |---|---|---|---|
//! | 1 | [`add_l64`] [`sub_l64`] [`neg_l64`] [`inc_l64`] [`dec_l64`] [`mul_l64`] [`div_l64`] [`rem_l64`] [`pow_l64`] | [`add2_l64`] … [`pow2_l64`] | [`pow_bits_l64`] |
//! | 2 | [`add`] [`add_assign`] [`sub`] [`sub_assign`] [`neg`] [`neg_assign`] [`inc`] [`inc_assign`] [`dec`] [`dec_assign`] [`mul`] [`mul_assign`] [`div`] [`div_assign`] [`rem`] [`rem_assign`] [`pow`] | [`add2`] [`add_assign2`] … [`pow2`] | [`pow_bits`] |
//! | 3 | [`LogicVec::add`] … [`LogicVec::pow`] | [`BitVec::add`] … [`BitVec::pow`] | [`BitVec::pow_logic`] |

use crate::layout::{normalize_plane, word_mask};
use crate::logic::Logic;
use crate::scalar::{self, L64, mask, sign_extend};
use crate::scratch::Scratch;
use crate::vec::{BitVec, LogicVec};
use crate::view::{BitMut, BitRef, LogicMut, LogicRef};
use crate::words;

// ===========================================================================
// 幅の検査
// ===========================================================================

#[inline]
fn check_widths2(f: &str, dst: u32, x: u32, y: u32) {
    assert!(
        dst == x && dst == y,
        "{f}: width mismatch (dst {dst}, x {x}, y {y})"
    );
}

#[inline]
fn check_widths1(f: &str, dst: u32, x: u32) {
    assert_eq!(dst, x, "{f}: width mismatch (dst {dst}, x {x})");
}

// ===========================================================================
// 層 1: スカラ (幅 1〜64)
// ===========================================================================
//
// 4 値版の式 (plan.md §3.4): 未知 (`bx | by != 0`) なら aval・bval とも `mask(w)`、そうでなければ
// 2 値の演算結果 `& mask(w)` と bval = 0。入力は幅 `w` で正規化されていること。

/// 層 1 の 2 値 `x + y` (幅 `w`、§11.4.3)。
#[inline]
pub const fn add2_l64(x: u64, y: u64, w: u32) -> u64 {
    x.wrapping_add(y) & mask(w)
}

/// 層 1 の 2 値 `x - y`。
#[inline]
pub const fn sub2_l64(x: u64, y: u64, w: u32) -> u64 {
    x.wrapping_sub(y) & mask(w)
}

/// 層 1 の 2 値 `-x` (`0 - x`)。
#[inline]
pub const fn neg2_l64(x: u64, w: u32) -> u64 {
    x.wrapping_neg() & mask(w)
}

/// 層 1 の 2 値 `x + 1` (`++`、§11.4.2)。
#[inline]
pub const fn inc2_l64(x: u64, w: u32) -> u64 {
    x.wrapping_add(1) & mask(w)
}

/// 層 1 の 2 値 `x - 1` (`--`、§11.4.2)。
#[inline]
pub const fn dec2_l64(x: u64, w: u32) -> u64 {
    x.wrapping_sub(1) & mask(w)
}

/// 層 1 の 2 値 `x * y` (下位 `w` ビット)。
#[inline]
pub const fn mul2_l64(x: u64, y: u64, w: u32) -> u64 {
    x.wrapping_mul(y) & mask(w)
}

/// 商と剰余。除数が 0 なら `None` (規格上は X)。
#[inline]
const fn divrem_l64_core(x: u64, y: u64, w: u32, signed: bool) -> Option<(u64, u64)> {
    if y == 0 {
        return None;
    }
    if signed {
        // i128 で計算すれば、最小値 / -1 もあふれない (商 2^(w-1) を幅 w で折り返すと最小値)。
        let a = sign_extend(x, w) as i64 as i128;
        let b = sign_extend(y, w) as i64 as i128;
        Some(((a / b) as u64 & mask(w), (a % b) as u64 & mask(w)))
    } else {
        Some((x / y, x % y))
    }
}

/// 層 1 の 2 値 `x / y` (0 方向へ切り捨て)。除数が 0 なら 0 [要VCS確認 V-2]。
#[inline]
pub const fn div2_l64(x: u64, y: u64, w: u32, signed: bool) -> u64 {
    match divrem_l64_core(x, y, w, signed) {
        Some((q, _)) => q,
        None => 0,
    }
}

/// 層 1 の 2 値 `x % y` (符号は `x` の符号)。除数が 0 なら 0 [要VCS確認 V-2]。
#[inline]
pub const fn rem2_l64(x: u64, y: u64, w: u32, signed: bool) -> u64 {
    match divrem_l64_core(x, y, w, signed) {
        Some((_, r)) => r,
        None => 0,
    }
}

/// `base^e mod 2^64` (二乗法)。
#[inline]
const fn modpow_u64(base: u64, mut e: u64) -> u64 {
    let mut acc = 1u64;
    let mut b = base;
    while e != 0 {
        if e & 1 != 0 {
            acc = acc.wrapping_mul(b);
        }
        e >>= 1;
        if e != 0 {
            b = b.wrapping_mul(b);
        }
    }
    acc
}

/// べき乗の共通部分 (幅 `w` は 1〜64、指数の幅 `exp_w` は 1〜64)。`None` は X (`0 ** 負`)。
const fn pow_l64_core(
    base: u64,
    w: u32,
    base_signed: bool,
    exp: u64,
    exp_w: u32,
    exp_signed: bool,
) -> Option<u64> {
    let m = mask(w);
    if exp == 0 {
        // 指数が 0 なら 1 (base が 0 でも 1、§11.4.3)。
        return Some(1);
    }
    let exp_neg = exp_signed && exp_w >= 1 && (exp >> (exp_w - 1)) & 1 != 0;
    if exp_neg {
        if base == 0 {
            return None;
        }
        // 幅 1 の符号付きの '1' は -1 であって 1 ではない。
        let is_one = base == 1 && !(base_signed && w == 1);
        if is_one {
            return Some(1);
        }
        if base_signed && base == m {
            // base = -1: 指数が奇数なら -1、偶数なら 1 (負の数の偶奇は 2 の補数の LSB と同じ)。
            return Some(if exp & 1 != 0 { m } else { 1 });
        }
        return Some(0);
    }
    Some(modpow_u64(base, exp) & m)
}

/// 層 1 の 2 値 `base ** exp` (§11.4.3)。結果の幅は base の幅 `w`、指数の幅は `exp_w` (1〜64)。
/// `0 ** 負` は 0 [要VCS確認 V-2]。
#[inline]
pub const fn pow2_l64(
    base: u64,
    w: u32,
    base_signed: bool,
    exp: u64,
    exp_w: u32,
    exp_signed: bool,
) -> u64 {
    match pow_l64_core(base, w, base_signed, exp, exp_w, exp_signed) {
        Some(v) => v,
        None => 0,
    }
}

/// 2 値の結果を 4 値にする (`None` は全 X)。
#[inline]
const fn known_or_x(v: Option<u64>, w: u32) -> L64 {
    match v {
        Some(a) => L64 { a, b: 0 },
        None => L64::all_x(w),
    }
}

/// 層 1 の `x + y` (§11.4.3)。未知があれば全 X。
#[inline]
pub const fn add_l64(x: L64, y: L64, w: u32) -> L64 {
    if x.b | y.b != 0 {
        L64::all_x(w)
    } else {
        L64 {
            a: add2_l64(x.a, y.a, w),
            b: 0,
        }
    }
}

/// 層 1 の `x - y`。
#[inline]
pub const fn sub_l64(x: L64, y: L64, w: u32) -> L64 {
    if x.b | y.b != 0 {
        L64::all_x(w)
    } else {
        L64 {
            a: sub2_l64(x.a, y.a, w),
            b: 0,
        }
    }
}

/// 層 1 の `-x`。
#[inline]
pub const fn neg_l64(x: L64, w: u32) -> L64 {
    if x.b != 0 {
        L64::all_x(w)
    } else {
        L64 {
            a: neg2_l64(x.a, w),
            b: 0,
        }
    }
}

/// 層 1 の `x + 1` (`++`)。
#[inline]
pub const fn inc_l64(x: L64, w: u32) -> L64 {
    if x.b != 0 {
        L64::all_x(w)
    } else {
        L64 {
            a: inc2_l64(x.a, w),
            b: 0,
        }
    }
}

/// 層 1 の `x - 1` (`--`)。
#[inline]
pub const fn dec_l64(x: L64, w: u32) -> L64 {
    if x.b != 0 {
        L64::all_x(w)
    } else {
        L64 {
            a: dec2_l64(x.a, w),
            b: 0,
        }
    }
}

/// 層 1 の `x * y`。
#[inline]
pub const fn mul_l64(x: L64, y: L64, w: u32) -> L64 {
    if x.b | y.b != 0 {
        L64::all_x(w)
    } else {
        L64 {
            a: mul2_l64(x.a, y.a, w),
            b: 0,
        }
    }
}

/// 層 1 の `x / y`。未知があれば、または除数が 0 なら全 X。
#[inline]
pub const fn div_l64(x: L64, y: L64, w: u32, signed: bool) -> L64 {
    if x.b | y.b != 0 {
        return L64::all_x(w);
    }
    match divrem_l64_core(x.a, y.a, w, signed) {
        Some((q, _)) => L64 { a: q, b: 0 },
        None => L64::all_x(w),
    }
}

/// 層 1 の `x % y`。未知があれば、または除数が 0 なら全 X。
#[inline]
pub const fn rem_l64(x: L64, y: L64, w: u32, signed: bool) -> L64 {
    if x.b | y.b != 0 {
        return L64::all_x(w);
    }
    match divrem_l64_core(x.a, y.a, w, signed) {
        Some((_, r)) => L64 { a: r, b: 0 },
        None => L64::all_x(w),
    }
}

/// 層 1 の `base ** exp` (§11.4.3)。結果の幅は base の幅 `w`、指数の幅は `exp_w` (1〜64)。
/// どちらかに未知があれば全 X。
#[inline]
pub const fn pow_l64(
    base: L64,
    w: u32,
    base_signed: bool,
    exp: L64,
    exp_w: u32,
    exp_signed: bool,
) -> L64 {
    if base.b | exp.b != 0 {
        return L64::all_x(w);
    }
    known_or_x(
        pow_l64_core(base.a, w, base_signed, exp.a, exp_w, exp_signed),
        w,
    )
}

/// 層 1 の混在 `base ** exp` (2 値の base × 4 値の指数 → 4 値の結果、plan.md §3.5.1)。
#[inline]
pub const fn pow_bits_l64(
    base: u64,
    w: u32,
    base_signed: bool,
    exp: L64,
    exp_w: u32,
    exp_signed: bool,
) -> L64 {
    pow_l64(
        L64 { a: base, b: 0 },
        w,
        base_signed,
        exp,
        exp_w,
        exp_signed,
    )
}

// ===========================================================================
// ワード列の補助 (内部。作業領域はスライスで受ける)
// ===========================================================================

/// `x` を幅 `w` の 2 の補数として符号反転する (in-place、正規化する)。
#[inline]
fn neg_plane(x: &mut [u64], w: u32) {
    words::neg_assign(x);
    normalize_plane(w, x);
}

/// `x` (幅 `w`) が全ビット 1 か (符号付きなら -1)。
fn is_all_ones(x: &[u64], w: u32) -> bool {
    x.iter().enumerate().all(|(i, &v)| v == word_mask(w, i))
}

/// `x` が 1 か。
fn is_one(x: &[u64]) -> bool {
    match x.split_first() {
        Some((&lo, rest)) => lo == 1 && words::is_zero(rest),
        None => false,
    }
}

/// `x` (in-place、幅 `w`、`nw = x.len() >= 2`) を `y` で割った商 (`rem` が偽) または剰余 (`rem` が真)
/// にする。除数が 0 なら偽を返し、`x` は変えない。`un` (長さ `nw + 1`) と `vn` (長さ `nw`) は作業領域。
fn divrem_words(
    x: &mut [u64],
    y: &[u64],
    w: u32,
    signed: bool,
    rem: bool,
    un: &mut [u64],
    vn: &mut [u64],
) -> bool {
    let nw = x.len();
    debug_assert_eq!(un.len(), nw + 1);
    debug_assert_eq!(vn.len(), nw);
    if words::is_zero(y) {
        return false;
    }
    un[..nw].copy_from_slice(x);
    un[nw] = 0;
    vn.copy_from_slice(y);
    // 符号付きは絶対値で計算する。最小値の絶対値 2^(w-1) は符号なしの w ビットとして収まる。
    let sx = signed && words::sign_bit(x, w);
    let sy = signed && words::sign_bit(y, w);
    if sx {
        neg_plane(&mut un[..nw], w);
    }
    if sy {
        neg_plane(vn, w);
    }
    if rem {
        words::divrem_in_place(un, vn, None);
        x.copy_from_slice(&un[..nw]);
        // 剰余の符号は被除数の符号 (§11.4.3)。
        if sx {
            neg_plane(x, w);
        }
    } else {
        words::divrem_in_place(un, vn, Some(x));
        // 商は 0 方向へ切り捨て。符号が違えば負。最小値 / -1 は 2^(w-1) = 最小値に折り返す [V-1]。
        if sx != sy {
            neg_plane(x, w);
        }
    }
    true
}

/// `x` (in-place、幅 `w`) を `y` で割った商または剰余にする。除数が 0 なら偽 (`x` は変えない)。
/// 幅 ≤ 64 は層 1 の式で計算し、作業領域を使わない。
fn divrem_assign_words(
    x: &mut [u64],
    y: &[u64],
    w: u32,
    signed: bool,
    rem: bool,
    s: &mut Scratch,
) -> bool {
    let nw = x.len();
    match nw {
        0 => true,
        1 => match divrem_l64_core(x[0], y[0], w, signed) {
            Some((q, r)) => {
                x[0] = if rem { r } else { q };
                true
            }
            None => false,
        },
        _ => {
            let [un, vn] = s.parts([nw + 1, nw]);
            divrem_words(x, y, w, signed, rem, un, vn)
        }
    }
}

/// `dst = base ** exp mod 2^w` (幅 `w`、`nw = dst.len()`)。`0 ** 負` なら偽を返す (`dst` の内容は不定)。
/// `tmp` は長さ `nw` の作業領域。`base` は幅 `w`、`exp` は幅 `exp_w` で正規化されていること。
#[allow(clippy::too_many_arguments)]
fn pow_words(
    dst: &mut [u64],
    w: u32,
    base: &[u64],
    base_signed: bool,
    exp: &[u64],
    exp_w: u32,
    exp_signed: bool,
    tmp: &mut [u64],
) -> bool {
    let nw = dst.len();
    if nw == 0 {
        return true;
    }
    dst.fill(0);
    if words::is_zero(exp) {
        // 指数が 0 なら 1 (§11.4.3)。
        dst[0] = 1;
        return true;
    }
    if exp_signed && words::sign_bit(exp, exp_w) {
        // 指数が負 (§11.4.3 のべき乗の規則)。
        if words::is_zero(base) {
            return false;
        }
        if is_one(base) && !(base_signed && w == 1) {
            dst[0] = 1;
        } else if base_signed && is_all_ones(base, w) {
            // base = -1: 奇数なら -1、偶数なら 1。
            if exp[0] & 1 != 0 {
                dst.copy_from_slice(base);
            } else {
                dst[0] = 1;
            }
        }
        // |base| > 1 なら 0 (dst は 0 のまま)。
        return true;
    }
    // 指数は正。使う指数のビット数を決める。
    let ebits = words::bit_len(exp);
    let nbits = if base.first().is_some_and(|&b| b & 1 == 0) {
        // base が偶数: 指数が w 以上なら base^e は 2^w で割り切れて 0。
        let ge_w = ebits > 32 || exp[0] >= w as u64;
        if ge_w {
            return true;
        }
        ebits
    } else {
        // base が奇数: 指数の下位 w ビット (e mod 2^w) だけを使う。
        let mut n = ebits.min(w as u64);
        while n > 0 && !words::bit(exp, n - 1) {
            n -= 1;
        }
        n
    };
    if nbits == 0 {
        dst[0] = 1;
        return true;
    }
    // 左から右への二乗法。dst = base から始める (最上位ビットは 1)。
    dst.copy_from_slice(base);
    for i in (0..nbits - 1).rev() {
        words::mul_lo(tmp, dst, dst);
        dst.copy_from_slice(tmp);
        if words::bit(exp, i) {
            words::mul_lo(tmp, dst, base);
            dst.copy_from_slice(tmp);
        }
    }
    normalize_plane(w, dst);
    true
}

/// 層 2 のべき乗の共通部分 (2 値の値で計算する)。`0 ** 負` なら偽。幅 ≤ 64 は層 1 の式を使う。
#[allow(clippy::too_many_arguments)]
fn pow_into(
    dst: &mut [u64],
    w: u32,
    base: &[u64],
    base_signed: bool,
    exp: &[u64],
    exp_w: u32,
    exp_signed: bool,
    s: &mut Scratch,
) -> bool {
    let nw = dst.len();
    if nw == 1 && exp.len() == 1 {
        return match pow_l64_core(base[0], w, base_signed, exp[0], exp_w, exp_signed) {
            Some(v) => {
                dst[0] = v;
                true
            }
            None => false,
        };
    }
    if nw == 1 {
        // 指数だけが 64 ビットより広い: 作業領域はスタックの 1 ワードで足りる。
        let mut tmp = [0u64; 1];
        return pow_words(dst, w, base, base_signed, exp, exp_w, exp_signed, &mut tmp);
    }
    let tmp = s.words(nw);
    pow_words(dst, w, base, base_signed, exp, exp_w, exp_signed, tmp)
}

// ===========================================================================
// 層 2: 4 値
// ===========================================================================

/// 4 値の出力先に、2 値の計算 `f(aval)` の結果を書く。`f` が偽を返したら (0 除算など) 全 X。
#[inline]
fn write_known(mut dst: LogicMut<'_>, f: impl FnOnce(&mut [u64]) -> bool) {
    let ok = {
        let (a, b) = dst.planes_mut();
        b.fill(0);
        f(a)
    };
    if !ok {
        dst.fill(Logic::X);
    }
}

/// `dst = x + y` (§11.4.3)。未知があれば全 X。オーバフローは捨てる。
///
/// # Panics
/// `dst`, `x`, `y` の幅が揃っていない場合。
pub fn add(mut dst: LogicMut<'_>, x: LogicRef<'_>, y: LogicRef<'_>) {
    check_widths2("add", dst.width(), x.width(), y.width());
    if x.has_unknown() || y.has_unknown() {
        return dst.fill(Logic::X);
    }
    let w = dst.width();
    write_known(dst, |a| {
        words::add(a, x.aval(), y.aval());
        normalize_plane(w, a);
        true
    });
}

/// `dst_x += y` (in-place 版)。
///
/// # Panics
/// 幅が揃っていない場合。
pub fn add_assign(mut dst_x: LogicMut<'_>, y: LogicRef<'_>) {
    check_widths1("add_assign", dst_x.width(), y.width());
    if dst_x.as_ref().has_unknown() || y.has_unknown() {
        return dst_x.fill(Logic::X);
    }
    let w = dst_x.width();
    let a = dst_x.planes_mut().0;
    words::add_assign(a, y.aval());
    normalize_plane(w, a);
}

/// `dst = x - y` (§11.4.3)。未知があれば全 X。
///
/// # Panics
/// `dst`, `x`, `y` の幅が揃っていない場合。
pub fn sub(mut dst: LogicMut<'_>, x: LogicRef<'_>, y: LogicRef<'_>) {
    check_widths2("sub", dst.width(), x.width(), y.width());
    if x.has_unknown() || y.has_unknown() {
        return dst.fill(Logic::X);
    }
    let w = dst.width();
    write_known(dst, |a| {
        words::sub(a, x.aval(), y.aval());
        normalize_plane(w, a);
        true
    });
}

/// `dst_x -= y` (in-place 版)。
///
/// # Panics
/// 幅が揃っていない場合。
pub fn sub_assign(mut dst_x: LogicMut<'_>, y: LogicRef<'_>) {
    check_widths1("sub_assign", dst_x.width(), y.width());
    if dst_x.as_ref().has_unknown() || y.has_unknown() {
        return dst_x.fill(Logic::X);
    }
    let w = dst_x.width();
    let a = dst_x.planes_mut().0;
    words::sub_assign(a, y.aval());
    normalize_plane(w, a);
}

/// `dst = -x` (単項 `-`、`0 - x`、§11.4.3)。未知があれば全 X。
///
/// # Panics
/// `dst` と `x` の幅が違う場合。
pub fn neg(mut dst: LogicMut<'_>, x: LogicRef<'_>) {
    check_widths1("neg", dst.width(), x.width());
    if x.has_unknown() {
        return dst.fill(Logic::X);
    }
    let w = dst.width();
    write_known(dst, |a| {
        words::neg(a, x.aval());
        normalize_plane(w, a);
        true
    });
}

/// `x = -x` (in-place 版)。
pub fn neg_assign(mut x: LogicMut<'_>) {
    if x.as_ref().has_unknown() {
        return x.fill(Logic::X);
    }
    let w = x.width();
    neg_plane(x.planes_mut().0, w);
}

/// `dst = x + 1` (`++`、§11.4.2)。未知があれば全 X。
///
/// # Panics
/// `dst` と `x` の幅が違う場合。
pub fn inc(mut dst: LogicMut<'_>, x: LogicRef<'_>) {
    check_widths1("inc", dst.width(), x.width());
    dst.copy_from(x);
    inc_assign(dst);
}

/// `x += 1` (`++` の in-place 版)。
pub fn inc_assign(mut x: LogicMut<'_>) {
    if x.as_ref().has_unknown() {
        return x.fill(Logic::X);
    }
    let w = x.width();
    let a = x.planes_mut().0;
    words::add_small_assign(a, 1);
    normalize_plane(w, a);
}

/// `dst = x - 1` (`--`、§11.4.2)。未知があれば全 X。
///
/// # Panics
/// `dst` と `x` の幅が違う場合。
pub fn dec(mut dst: LogicMut<'_>, x: LogicRef<'_>) {
    check_widths1("dec", dst.width(), x.width());
    dst.copy_from(x);
    dec_assign(dst);
}

/// `x -= 1` (`--` の in-place 版)。
pub fn dec_assign(mut x: LogicMut<'_>) {
    if x.as_ref().has_unknown() {
        return x.fill(Logic::X);
    }
    let w = x.width();
    let a = x.planes_mut().0;
    words::sub_small_assign(a, 1);
    normalize_plane(w, a);
}

/// `dst = x * y` の下位 `w` ビット (§11.4.3)。未知があれば全 X。下位ビットは符号に依らないので
/// `signed` を取らない。作業領域は使わない (`s` は他の算術カーネルと形を揃えるために受ける)。
///
/// # Panics
/// `dst`, `x`, `y` の幅が揃っていない場合。
pub fn mul(mut dst: LogicMut<'_>, x: LogicRef<'_>, y: LogicRef<'_>, s: &mut Scratch) {
    check_widths2("mul", dst.width(), x.width(), y.width());
    let _ = s;
    if x.has_unknown() || y.has_unknown() {
        return dst.fill(Logic::X);
    }
    let w = dst.width();
    write_known(dst, |a| {
        words::mul_lo(a, x.aval(), y.aval());
        normalize_plane(w, a);
        true
    });
}

/// `dst_x *= y` (in-place 版)。作業領域は `nw` ワード。
///
/// # Panics
/// 幅が揃っていない場合。
pub fn mul_assign(mut dst_x: LogicMut<'_>, y: LogicRef<'_>, s: &mut Scratch) {
    check_widths1("mul_assign", dst_x.width(), y.width());
    if dst_x.as_ref().has_unknown() || y.has_unknown() {
        return dst_x.fill(Logic::X);
    }
    let w = dst_x.width();
    mul_assign_words(dst_x.planes_mut().0, y.aval(), w, s);
}

/// `x *= y` (ワード列、幅 `w`)。
fn mul_assign_words(x: &mut [u64], y: &[u64], w: u32, s: &mut Scratch) {
    let nw = x.len();
    if nw == 1 {
        x[0] = mul2_l64(x[0], y[0], w);
        return;
    }
    let tmp = s.words(nw);
    words::mul_lo(tmp, x, y);
    x.copy_from_slice(tmp);
    normalize_plane(w, x);
}

/// `dst = x / y` (§11.4.3)。`signed` は両オペランドが符号付きか (§11.8.1、呼び出し側が決める)。
/// 0 方向へ切り捨てる。未知があれば、または除数が 0 なら全 X。符号付きの最小値 / -1 は最小値
/// [要VCS確認 V-1]。作業領域は `2·nw + 1` ワード。
///
/// # Panics
/// `dst`, `x`, `y` の幅が揃っていない場合。
pub fn div(mut dst: LogicMut<'_>, x: LogicRef<'_>, y: LogicRef<'_>, signed: bool, s: &mut Scratch) {
    check_widths2("div", dst.width(), x.width(), y.width());
    if x.has_unknown() || y.has_unknown() {
        return dst.fill(Logic::X);
    }
    let w = dst.width();
    write_known(dst, |a| {
        a.copy_from_slice(x.aval());
        divrem_assign_words(a, y.aval(), w, signed, false, s)
    });
}

/// `dst_x /= y` (in-place 版)。
///
/// # Panics
/// 幅が揃っていない場合。
pub fn div_assign(mut dst_x: LogicMut<'_>, y: LogicRef<'_>, signed: bool, s: &mut Scratch) {
    check_widths1("div_assign", dst_x.width(), y.width());
    if dst_x.as_ref().has_unknown() || y.has_unknown() {
        return dst_x.fill(Logic::X);
    }
    let w = dst_x.width();
    if !divrem_assign_words(dst_x.planes_mut().0, y.aval(), w, signed, false, s) {
        dst_x.fill(Logic::X);
    }
}

/// `dst = x % y` (§11.4.3)。結果の符号は `x` の符号。未知があれば、または除数が 0 なら全 X。
/// 符号付きの最小値 % -1 は 0 [要VCS確認 V-1]。作業領域は `2·nw + 1` ワード。
///
/// # Panics
/// `dst`, `x`, `y` の幅が揃っていない場合。
pub fn rem(mut dst: LogicMut<'_>, x: LogicRef<'_>, y: LogicRef<'_>, signed: bool, s: &mut Scratch) {
    check_widths2("rem", dst.width(), x.width(), y.width());
    if x.has_unknown() || y.has_unknown() {
        return dst.fill(Logic::X);
    }
    let w = dst.width();
    write_known(dst, |a| {
        a.copy_from_slice(x.aval());
        divrem_assign_words(a, y.aval(), w, signed, true, s)
    });
}

/// `dst_x %= y` (in-place 版)。
///
/// # Panics
/// 幅が揃っていない場合。
pub fn rem_assign(mut dst_x: LogicMut<'_>, y: LogicRef<'_>, signed: bool, s: &mut Scratch) {
    check_widths1("rem_assign", dst_x.width(), y.width());
    if dst_x.as_ref().has_unknown() || y.has_unknown() {
        return dst_x.fill(Logic::X);
    }
    let w = dst_x.width();
    if !divrem_assign_words(dst_x.planes_mut().0, y.aval(), w, signed, true, s) {
        dst_x.fill(Logic::X);
    }
}

/// `dst = base ** exp` (§11.4.3)。結果の幅は `base` の幅 (文脈決定)、`exp` は自己決定で任意の幅。
/// `base_signed` / `exp_signed` はそれぞれの型が符号付きか。どちらかに未知があれば全 X。
/// 特殊ケースはモジュールの説明を参照。作業領域は `nw` ワード。
///
/// # Panics
/// `dst` と `base` の幅が違う場合。
pub fn pow(
    dst: LogicMut<'_>,
    base: LogicRef<'_>,
    base_signed: bool,
    exp: LogicRef<'_>,
    exp_signed: bool,
    s: &mut Scratch,
) {
    check_widths1("pow", dst.width(), base.width());
    pow_known(
        dst,
        base.aval(),
        base.has_unknown(),
        base_signed,
        exp,
        exp_signed,
        s,
    );
}

/// `pow` と `pow_bits` の共通部分。
fn pow_known(
    mut dst: LogicMut<'_>,
    base: &[u64],
    base_unknown: bool,
    base_signed: bool,
    exp: LogicRef<'_>,
    exp_signed: bool,
    s: &mut Scratch,
) {
    if base_unknown || exp.has_unknown() {
        return dst.fill(Logic::X);
    }
    let w = dst.width();
    let ew = exp.width();
    write_known(dst, |a| {
        pow_into(a, w, base, base_signed, exp.aval(), ew, exp_signed, s)
    });
}

// ===========================================================================
// 層 2: 混在 (2 値の base × 4 値の指数 → 4 値の結果、plan.md §3.5.1)
// ===========================================================================

/// `dst = base ** exp` で、`base` が 2 値、`exp` が 4 値の場合 (plan.md §3.5.1 規則 1)。結果は 4 値で、
/// `exp` に X/Z があれば全 X、`0 ** 負` も全 X (2 値の変数への代入で 0 にするのは呼び出し側)。
/// それ以外は bval = 0 の [`pow2`] の結果。作業領域は `nw` ワード。
///
/// # Panics
/// `dst` と `base` の幅が違う場合。
pub fn pow_bits(
    dst: LogicMut<'_>,
    base: BitRef<'_>,
    base_signed: bool,
    exp: LogicRef<'_>,
    exp_signed: bool,
    s: &mut Scratch,
) {
    check_widths1("pow_bits", dst.width(), base.width());
    pow_known(dst, base.words(), false, base_signed, exp, exp_signed, s);
}

// ===========================================================================
// 層 2: 2 値
// ===========================================================================

/// 2 値の `dst = x + y` (§11.4.3)。
///
/// # Panics
/// `dst`, `x`, `y` の幅が揃っていない場合。
pub fn add2(mut dst: BitMut<'_>, x: BitRef<'_>, y: BitRef<'_>) {
    check_widths2("add2", dst.width(), x.width(), y.width());
    words::add(dst.words_mut(), x.words(), y.words());
    dst.normalize();
}

/// 2 値の `dst_x += y`。
///
/// # Panics
/// 幅が揃っていない場合。
pub fn add_assign2(mut dst_x: BitMut<'_>, y: BitRef<'_>) {
    check_widths1("add_assign2", dst_x.width(), y.width());
    words::add_assign(dst_x.words_mut(), y.words());
    dst_x.normalize();
}

/// 2 値の `dst = x - y`。
///
/// # Panics
/// `dst`, `x`, `y` の幅が揃っていない場合。
pub fn sub2(mut dst: BitMut<'_>, x: BitRef<'_>, y: BitRef<'_>) {
    check_widths2("sub2", dst.width(), x.width(), y.width());
    words::sub(dst.words_mut(), x.words(), y.words());
    dst.normalize();
}

/// 2 値の `dst_x -= y`。
///
/// # Panics
/// 幅が揃っていない場合。
pub fn sub_assign2(mut dst_x: BitMut<'_>, y: BitRef<'_>) {
    check_widths1("sub_assign2", dst_x.width(), y.width());
    words::sub_assign(dst_x.words_mut(), y.words());
    dst_x.normalize();
}

/// 2 値の `dst = -x`。
///
/// # Panics
/// `dst` と `x` の幅が違う場合。
pub fn neg2(mut dst: BitMut<'_>, x: BitRef<'_>) {
    check_widths1("neg2", dst.width(), x.width());
    words::neg(dst.words_mut(), x.words());
    dst.normalize();
}

/// 2 値の `x = -x`。
pub fn neg_assign2(mut x: BitMut<'_>) {
    words::neg_assign(x.words_mut());
    x.normalize();
}

/// 2 値の `dst = x + 1` (`++`)。
///
/// # Panics
/// `dst` と `x` の幅が違う場合。
pub fn inc2(mut dst: BitMut<'_>, x: BitRef<'_>) {
    check_widths1("inc2", dst.width(), x.width());
    dst.copy_from(x);
    inc_assign2(dst);
}

/// 2 値の `x += 1`。
pub fn inc_assign2(mut x: BitMut<'_>) {
    words::add_small_assign(x.words_mut(), 1);
    x.normalize();
}

/// 2 値の `dst = x - 1` (`--`)。
///
/// # Panics
/// `dst` と `x` の幅が違う場合。
pub fn dec2(mut dst: BitMut<'_>, x: BitRef<'_>) {
    check_widths1("dec2", dst.width(), x.width());
    dst.copy_from(x);
    dec_assign2(dst);
}

/// 2 値の `x -= 1`。
pub fn dec_assign2(mut x: BitMut<'_>) {
    words::sub_small_assign(x.words_mut(), 1);
    x.normalize();
}

/// 2 値の `dst = x * y` (下位 `w` ビット)。作業領域は使わない。
///
/// # Panics
/// `dst`, `x`, `y` の幅が揃っていない場合。
pub fn mul2(mut dst: BitMut<'_>, x: BitRef<'_>, y: BitRef<'_>, s: &mut Scratch) {
    check_widths2("mul2", dst.width(), x.width(), y.width());
    let _ = s;
    words::mul_lo(dst.words_mut(), x.words(), y.words());
    dst.normalize();
}

/// 2 値の `dst_x *= y`。作業領域は `nw` ワード。
///
/// # Panics
/// 幅が揃っていない場合。
pub fn mul_assign2(mut dst_x: BitMut<'_>, y: BitRef<'_>, s: &mut Scratch) {
    check_widths1("mul_assign2", dst_x.width(), y.width());
    let w = dst_x.width();
    mul_assign_words(dst_x.words_mut(), y.words(), w, s);
}

/// 2 値の `dst = x / y`。除数が 0 なら 0 [要VCS確認 V-2]。作業領域は `2·nw + 1` ワード。
///
/// # Panics
/// `dst`, `x`, `y` の幅が揃っていない場合。
pub fn div2(mut dst: BitMut<'_>, x: BitRef<'_>, y: BitRef<'_>, signed: bool, s: &mut Scratch) {
    check_widths2("div2", dst.width(), x.width(), y.width());
    dst.copy_from(x);
    div_assign2(dst, y, signed, s);
}

/// 2 値の `dst_x /= y`。除数が 0 なら 0 [要VCS確認 V-2]。
///
/// # Panics
/// 幅が揃っていない場合。
pub fn div_assign2(mut dst_x: BitMut<'_>, y: BitRef<'_>, signed: bool, s: &mut Scratch) {
    check_widths1("div_assign2", dst_x.width(), y.width());
    let w = dst_x.width();
    if !divrem_assign_words(dst_x.words_mut(), y.words(), w, signed, false, s) {
        dst_x.fill(false);
    }
}

/// 2 値の `dst = x % y`。除数が 0 なら 0 [要VCS確認 V-2]。作業領域は `2·nw + 1` ワード。
///
/// # Panics
/// `dst`, `x`, `y` の幅が揃っていない場合。
pub fn rem2(mut dst: BitMut<'_>, x: BitRef<'_>, y: BitRef<'_>, signed: bool, s: &mut Scratch) {
    check_widths2("rem2", dst.width(), x.width(), y.width());
    dst.copy_from(x);
    rem_assign2(dst, y, signed, s);
}

/// 2 値の `dst_x %= y`。除数が 0 なら 0 [要VCS確認 V-2]。
///
/// # Panics
/// 幅が揃っていない場合。
pub fn rem_assign2(mut dst_x: BitMut<'_>, y: BitRef<'_>, signed: bool, s: &mut Scratch) {
    check_widths1("rem_assign2", dst_x.width(), y.width());
    let w = dst_x.width();
    if !divrem_assign_words(dst_x.words_mut(), y.words(), w, signed, true, s) {
        dst_x.fill(false);
    }
}

/// 2 値の `dst = base ** exp` (§11.4.3)。`0 ** 負` は 0 [要VCS確認 V-2]。作業領域は `nw` ワード。
///
/// # Panics
/// `dst` と `base` の幅が違う場合。
pub fn pow2(
    mut dst: BitMut<'_>,
    base: BitRef<'_>,
    base_signed: bool,
    exp: BitRef<'_>,
    exp_signed: bool,
    s: &mut Scratch,
) {
    check_widths1("pow2", dst.width(), base.width());
    let w = dst.width();
    let ok = pow_into(
        dst.words_mut(),
        w,
        base.words(),
        base_signed,
        exp.words(),
        exp.width(),
        exp_signed,
        s,
    );
    if !ok {
        dst.fill(false);
    }
}

// ===========================================================================
// 層 3
// ===========================================================================
//
// 幅 1〜64 は層 1 の式で計算し、ヒープ確保をしない (plan.md §5.5)。それより広い幅は、結果の値と
// 作業領域をその場で確保して層 2 を呼ぶ。

/// 層 3 の 4 値の 2 項演算の共通部分。
fn logic_binary(
    x: &LogicVec,
    y: &LogicVec,
    l1: impl FnOnce(L64, L64, u32) -> L64,
    l2: impl FnOnce(LogicMut<'_>, LogicRef<'_>, LogicRef<'_>, &mut Scratch),
) -> LogicVec {
    let w = x.width();
    let mut r = LogicVec::zero(w);
    if (1..=64).contains(&w) && y.width() == w {
        l1(L64::load(x.as_ref()), L64::load(y.as_ref()), w).store(r.as_mut());
    } else {
        let mut s = Scratch::for_width(w);
        l2(r.as_mut(), x.as_ref(), y.as_ref(), &mut s);
    }
    r
}

/// 層 3 の 4 値の単項演算の共通部分。
fn logic_unary(
    x: &LogicVec,
    l1: impl FnOnce(L64, u32) -> L64,
    l2: impl FnOnce(LogicMut<'_>, LogicRef<'_>),
) -> LogicVec {
    let w = x.width();
    let mut r = LogicVec::zero(w);
    if (1..=64).contains(&w) {
        l1(L64::load(x.as_ref()), w).store(r.as_mut());
    } else {
        l2(r.as_mut(), x.as_ref());
    }
    r
}

/// 層 3 の 2 値の 2 項演算の共通部分。
fn bit_binary(
    x: &BitVec,
    y: &BitVec,
    l1: impl FnOnce(u64, u64, u32) -> u64,
    l2: impl FnOnce(BitMut<'_>, BitRef<'_>, BitRef<'_>, &mut Scratch),
) -> BitVec {
    let w = x.width();
    let mut r = BitVec::zero(w);
    if (1..=64).contains(&w) && y.width() == w {
        let v = l1(scalar::load2(x.as_ref()), scalar::load2(y.as_ref()), w);
        scalar::store2(v, r.as_mut());
    } else {
        let mut s = Scratch::for_width(w);
        l2(r.as_mut(), x.as_ref(), y.as_ref(), &mut s);
    }
    r
}

/// 層 3 の 2 値の単項演算の共通部分。
fn bit_unary(
    x: &BitVec,
    l1: impl FnOnce(u64, u32) -> u64,
    l2: impl FnOnce(BitMut<'_>, BitRef<'_>),
) -> BitVec {
    let w = x.width();
    let mut r = BitVec::zero(w);
    if (1..=64).contains(&w) {
        scalar::store2(l1(scalar::load2(x.as_ref()), w), r.as_mut());
    } else {
        l2(r.as_mut(), x.as_ref());
    }
    r
}

/// 層 3 の 4 値の結果のべき乗 (`pow` と `pow_logic`)。`base` は base の値のワード列 (4 値なら aval)、
/// `base_unknown` は base に X/Z があるか。幅 > 64 では `l2` (層 2) を呼ぶ。
fn logic_pow(
    w: u32,
    base: &[u64],
    base_unknown: bool,
    base_signed: bool,
    exp: &LogicVec,
    exp_signed: bool,
    l2: impl FnOnce(LogicMut<'_>, &mut Scratch),
) -> LogicVec {
    let mut r = LogicVec::zero(w);
    if (1..=64).contains(&w) {
        // 幅 ≤ 64: 作業領域はスタックの 1 ワードで足りる (指数が 64 ビットより広くても)。
        let e = exp.as_ref();
        let mut a = [0u64; 1];
        let mut tmp = [0u64; 1];
        let v = if base_unknown || e.has_unknown() {
            L64::all_x(w)
        } else if pow_words(
            &mut a,
            w,
            base,
            base_signed,
            e.aval(),
            e.width(),
            exp_signed,
            &mut tmp,
        ) {
            L64 { a: a[0], b: 0 }
        } else {
            L64::all_x(w)
        };
        v.store(r.as_mut());
    } else {
        let mut s = Scratch::for_width(w);
        l2(r.as_mut(), &mut s);
    }
    r
}

impl LogicVec {
    /// `self + y` (§11.4.3)。幅は揃っていること (文脈決定、呼び出し側で拡張する)。
    pub fn add(&self, y: &LogicVec) -> LogicVec {
        logic_binary(self, y, add_l64, |d, x, y, _| add(d, x, y))
    }

    /// `self += y`。
    pub fn add_assign(&mut self, y: &LogicVec) {
        add_assign(self.as_mut(), y.as_ref());
    }

    /// `self - y` (§11.4.3)。
    pub fn sub(&self, y: &LogicVec) -> LogicVec {
        logic_binary(self, y, sub_l64, |d, x, y, _| sub(d, x, y))
    }

    /// `self -= y`。
    pub fn sub_assign(&mut self, y: &LogicVec) {
        sub_assign(self.as_mut(), y.as_ref());
    }

    /// `-self` (単項 `-`、§11.4.3)。
    pub fn neg(&self) -> LogicVec {
        logic_unary(self, neg_l64, neg)
    }

    /// `self = -self`。
    pub fn neg_assign(&mut self) {
        neg_assign(self.as_mut());
    }

    /// `self + 1` (`++` の値、§11.4.2)。
    pub fn inc(&self) -> LogicVec {
        logic_unary(self, inc_l64, inc)
    }

    /// `self += 1` (`++`)。
    pub fn inc_assign(&mut self) {
        inc_assign(self.as_mut());
    }

    /// `self - 1` (`--` の値、§11.4.2)。
    pub fn dec(&self) -> LogicVec {
        logic_unary(self, dec_l64, dec)
    }

    /// `self -= 1` (`--`)。
    pub fn dec_assign(&mut self) {
        dec_assign(self.as_mut());
    }

    /// `self * y` の下位 `w` ビット (§11.4.3)。
    pub fn mul(&self, y: &LogicVec) -> LogicVec {
        logic_binary(self, y, mul_l64, mul)
    }

    /// `self *= y`。
    pub fn mul_assign(&mut self, y: &LogicVec) {
        if (1..=64).contains(&self.width()) && y.width() == self.width() {
            *self = self.mul(y);
        } else {
            let mut s = Scratch::for_width(self.width());
            mul_assign(self.as_mut(), y.as_ref(), &mut s);
        }
    }

    /// `self / y` (§11.4.3)。`signed` は両オペランドが符号付きか。除数が 0 なら全 X。
    pub fn div(&self, y: &LogicVec, signed: bool) -> LogicVec {
        logic_binary(
            self,
            y,
            |x, y, w| div_l64(x, y, w, signed),
            |d, x, y, s| div(d, x, y, signed, s),
        )
    }

    /// `self /= y`。
    pub fn div_assign(&mut self, y: &LogicVec, signed: bool) {
        if (1..=64).contains(&self.width()) && y.width() == self.width() {
            *self = self.div(y, signed);
        } else {
            let mut s = Scratch::for_width(self.width());
            div_assign(self.as_mut(), y.as_ref(), signed, &mut s);
        }
    }

    /// `self % y` (§11.4.3)。結果の符号は `self` の符号。除数が 0 なら全 X。
    pub fn rem(&self, y: &LogicVec, signed: bool) -> LogicVec {
        logic_binary(
            self,
            y,
            |x, y, w| rem_l64(x, y, w, signed),
            |d, x, y, s| rem(d, x, y, signed, s),
        )
    }

    /// `self %= y`。
    pub fn rem_assign(&mut self, y: &LogicVec, signed: bool) {
        if (1..=64).contains(&self.width()) && y.width() == self.width() {
            *self = self.rem(y, signed);
        } else {
            let mut s = Scratch::for_width(self.width());
            rem_assign(self.as_mut(), y.as_ref(), signed, &mut s);
        }
    }

    /// `self ** exp` (§11.4.3)。結果の幅は `self` の幅、`exp` は自己決定で任意の幅。
    /// `base_signed` は `self` の型、`exp_signed` は `exp` の型が符号付きか。
    pub fn pow(&self, base_signed: bool, exp: &LogicVec, exp_signed: bool) -> LogicVec {
        let x = self.as_ref();
        logic_pow(
            self.width(),
            x.aval(),
            x.has_unknown(),
            base_signed,
            exp,
            exp_signed,
            |d, s| pow(d, x, base_signed, exp.as_ref(), exp_signed, s),
        )
    }
}

impl BitVec {
    /// 2 値の `self + y` (§11.4.3)。
    pub fn add(&self, y: &BitVec) -> BitVec {
        bit_binary(self, y, add2_l64, |d, x, y, _| add2(d, x, y))
    }

    /// 2 値の `self += y`。
    pub fn add_assign(&mut self, y: &BitVec) {
        add_assign2(self.as_mut(), y.as_ref());
    }

    /// 2 値の `self - y`。
    pub fn sub(&self, y: &BitVec) -> BitVec {
        bit_binary(self, y, sub2_l64, |d, x, y, _| sub2(d, x, y))
    }

    /// 2 値の `self -= y`。
    pub fn sub_assign(&mut self, y: &BitVec) {
        sub_assign2(self.as_mut(), y.as_ref());
    }

    /// 2 値の `-self`。
    pub fn neg(&self) -> BitVec {
        bit_unary(self, neg2_l64, neg2)
    }

    /// 2 値の `self = -self`。
    pub fn neg_assign(&mut self) {
        neg_assign2(self.as_mut());
    }

    /// 2 値の `self + 1`。
    pub fn inc(&self) -> BitVec {
        bit_unary(self, inc2_l64, inc2)
    }

    /// 2 値の `self += 1`。
    pub fn inc_assign(&mut self) {
        inc_assign2(self.as_mut());
    }

    /// 2 値の `self - 1`。
    pub fn dec(&self) -> BitVec {
        bit_unary(self, dec2_l64, dec2)
    }

    /// 2 値の `self -= 1`。
    pub fn dec_assign(&mut self) {
        dec_assign2(self.as_mut());
    }

    /// 2 値の `self * y` (下位 `w` ビット)。
    pub fn mul(&self, y: &BitVec) -> BitVec {
        bit_binary(self, y, mul2_l64, mul2)
    }

    /// 2 値の `self *= y`。
    pub fn mul_assign(&mut self, y: &BitVec) {
        if (1..=64).contains(&self.width()) && y.width() == self.width() {
            *self = self.mul(y);
        } else {
            let mut s = Scratch::for_width(self.width());
            mul_assign2(self.as_mut(), y.as_ref(), &mut s);
        }
    }

    /// 2 値の `self / y`。除数が 0 なら 0 [要VCS確認 V-2]。
    pub fn div(&self, y: &BitVec, signed: bool) -> BitVec {
        bit_binary(
            self,
            y,
            |x, y, w| div2_l64(x, y, w, signed),
            |d, x, y, s| div2(d, x, y, signed, s),
        )
    }

    /// 2 値の `self /= y`。
    pub fn div_assign(&mut self, y: &BitVec, signed: bool) {
        if (1..=64).contains(&self.width()) && y.width() == self.width() {
            *self = self.div(y, signed);
        } else {
            let mut s = Scratch::for_width(self.width());
            div_assign2(self.as_mut(), y.as_ref(), signed, &mut s);
        }
    }

    /// 2 値の `self % y`。除数が 0 なら 0 [要VCS確認 V-2]。
    pub fn rem(&self, y: &BitVec, signed: bool) -> BitVec {
        bit_binary(
            self,
            y,
            |x, y, w| rem2_l64(x, y, w, signed),
            |d, x, y, s| rem2(d, x, y, signed, s),
        )
    }

    /// 2 値の `self %= y`。
    pub fn rem_assign(&mut self, y: &BitVec, signed: bool) {
        if (1..=64).contains(&self.width()) && y.width() == self.width() {
            *self = self.rem(y, signed);
        } else {
            let mut s = Scratch::for_width(self.width());
            rem_assign2(self.as_mut(), y.as_ref(), signed, &mut s);
        }
    }

    /// 2 値の `self ** exp` (§11.4.3)。`0 ** 負` は 0 [要VCS確認 V-2]。
    pub fn pow(&self, base_signed: bool, exp: &BitVec, exp_signed: bool) -> BitVec {
        let w = self.width();
        let mut r = BitVec::zero(w);
        if (1..=64).contains(&w) {
            let mut a = [0u64; 1];
            let mut tmp = [0u64; 1];
            let ok = pow_words(
                &mut a,
                w,
                self.as_ref().words(),
                base_signed,
                exp.as_ref().words(),
                exp.width(),
                exp_signed,
                &mut tmp,
            );
            scalar::store2(if ok { a[0] } else { 0 }, r.as_mut());
        } else {
            let mut s = Scratch::for_width(w);
            pow2(
                r.as_mut(),
                self.as_ref(),
                base_signed,
                exp.as_ref(),
                exp_signed,
                &mut s,
            );
        }
        r
    }

    /// 混在の `self ** exp` (2 値の base × 4 値の指数 → 4 値の結果、plan.md §3.5.1)。
    pub fn pow_logic(&self, base_signed: bool, exp: &LogicVec, exp_signed: bool) -> LogicVec {
        let x = self.as_ref();
        logic_pow(
            self.width(),
            x.words(),
            false,
            base_signed,
            exp,
            exp_signed,
            |d, s| pow_bits(d, x, base_signed, exp.as_ref(), exp_signed, s),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lv(s: &str) -> LogicVec {
        LogicVec::from_bits_str(s).unwrap()
    }

    /// §11.4.3: 未知が 1 ビットでもあれば全 X。
    #[test]
    fn unknown_gives_all_x_11_4_3() {
        let x = lv("000x");
        let y = lv("0001");
        assert_eq!(x.add(&y).to_bits_string(), "xxxx");
        assert_eq!(lv("0z00").add(&lv("0000")).to_bits_string(), "xxxx");
        assert_eq!(y.mul(&lv("000z")).to_bits_string(), "xxxx");
        assert_eq!(y.div(&x, false).to_bits_string(), "xxxx");
        assert_eq!(
            lv("0011").pow(false, &lv("00z1"), false).to_bits_string(),
            "xxxx"
        );
        assert_eq!(lv("1x00").inc().to_bits_string(), "xxxx");
    }

    /// §11.4.3: 符号付き除算は 0 方向へ切り捨て、剰余の符号は被除数。
    #[test]
    fn signed_div_rem_l64() {
        let w = 8;
        let f = |v: i64| (v as u64) & 0xff;
        assert_eq!(div2_l64(f(-7), 2, w, true), f(-3));
        assert_eq!(rem2_l64(f(-7), 2, w, true), f(-1));
        assert_eq!(div2_l64(7, f(-2), w, true), f(-3));
        assert_eq!(rem2_l64(7, f(-2), w, true), 1);
        // 最小値 / -1 → 最小値、剰余 0 [V-1]
        assert_eq!(div2_l64(0x80, 0xff, w, true), 0x80);
        assert_eq!(rem2_l64(0x80, 0xff, w, true), 0);
        // 幅 64 の最小値 / -1 も i128 であふれない
        assert_eq!(div2_l64(1 << 63, !0, 64, true), 1 << 63);
        // 2 値の 0 除算は 0 [V-2]
        assert_eq!(div2_l64(5, 0, w, false), 0);
        assert_eq!(div_l64(L64::new(5, 0), L64::ZERO, w, false), L64::all_x(w));
    }

    /// 多ワードの符号付き除算 (幅 65 の最小値 / -1、§11.4.3、[V-1])。
    #[test]
    fn signed_div_multiword_min() {
        let mut min = LogicVec::zero(65);
        min.set_bit(64, Logic::L1);
        let m1 = LogicVec::filled(65, Logic::L1);
        assert_eq!(min.div(&m1, true), min);
        assert_eq!(min.rem(&m1, true), LogicVec::zero(65));
        // 符号なしでは 2^64 / (2^65 - 1) = 0、剰余 2^64
        assert_eq!(min.div(&m1, false), LogicVec::zero(65));
        assert_eq!(min.rem(&m1, false), min);
    }

    /// べき乗: 奇数の base で指数の下位 w ビットだけを使っても結果は同じ (3^(2^64) mod 2^8 = 1)。
    #[test]
    fn pow_large_exponent_reduction() {
        let mut e = LogicVec::zero(65);
        e.set_bit(64, Logic::L1);
        assert_eq!(
            LogicVec::from_u64(8, 3).pow(false, &e, false),
            LogicVec::from_u64(8, 1)
        );
        assert_eq!(
            LogicVec::from_u64(8, 2).pow(false, &e, false),
            LogicVec::zero(8)
        );
        // 多ワードの base でも同じ
        let b = LogicVec::from_u64(130, 3);
        assert_eq!(b.pow(false, &e, false).width(), 130);
    }
}
