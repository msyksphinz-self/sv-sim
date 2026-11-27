//! 実数との変換 (IEEE 1800-2023 §6.12.1, §20.5、plan.md §3.5(i))。
//!
//! - 整数 → 実数 ([`to_f64`], [`to_f32`]): X/Z のビットは 0 として扱う [要VCS確認 V-14]。
//!   仮数に収まらない値は、IEEE 754 の最近接偶数丸め (roundTiesToEven) で、**多倍長の値から直接
//!   1 回だけ**丸める。`to_f32` も f64 を経由しない (二重丸めを避ける。R-12)。範囲を超える値は
//!   ±∞ になる (最大値に飽和させない。test-plan Q-5)。
//! - 実数 → 整数 ([`from_f64_round`], [`from_f64_trunc`]): f64 を符号・指数・仮数に分解し、
//!   正確な整数値を丸めてから dst の幅の 2 の補数に切り詰める。dst は任意の幅でよい。
//!   変換の状態を [`RealToIntStatus`] で返す (test-plan Q-6)。
//! - `$realtobits` 系 (§20.5): ビットパターンをそのまま使う ([`real_to_bits`] 等)。
//!
//! どの関数もヒープ確保をせず、作業領域も要らない。

use crate::layout::{normalize_plane, top_mask};
use crate::vec::{BitVec, LogicVec};
use crate::view::{BitMut, BitRef, LogicMut, LogicRef};
use crate::words;

// ===========================================================================
// 整数 → 実数
// ===========================================================================

/// 2 進浮動小数点形式のパラメタ。
struct Format {
    /// 仮数のビット数 (暗黙の 1 を含む)。f64 は 53、f32 は 24。
    precision: u32,
    /// 正規化数の最大の指数。f64 は 1023、f32 は 127。
    max_exp: u64,
}

const F64: Format = Format {
    precision: 53,
    max_exp: 1023,
};
const F32: Format = Format {
    precision: 24,
    max_exp: 127,
};

/// 整数の絶対値のワード列を、確保せずに 1 ワードずつ読むための表現。
///
/// 値は `aval & !bval` (X/Z を 0 として扱う、V-14)。負の値 (符号付きで符号ビットが 1) なら、
/// 絶対値は 2 の補数 `!x + 1` を幅 `width` で切り詰めたもの。`!x + 1` の繰り上がりは、
/// `x` の最下位の 0 でないワード `z` より下では全部 0 になり、`z` で止まるので、ワード `i` は
/// `i < z` なら 0、`i == z` なら `-x[z]`、`i > z` なら `!x[i]` になる。
struct Magnitude<'a> {
    a: &'a [u64],
    /// 2 値なら空 (bval = 0 とみなす)。
    b: &'a [u64],
    width: u32,
    neg: bool,
    /// `neg` のとき、x の最下位の 0 でないワードの位置。
    lowest: usize,
}

impl<'a> Magnitude<'a> {
    fn new(width: u32, a: &'a [u64], b: &'a [u64], signed: bool) -> Self {
        let mut m = Magnitude {
            a,
            b,
            width,
            neg: false,
            lowest: 0,
        };
        if signed && width > 0 {
            let i = width - 1;
            m.neg = (m.raw((i / 64) as usize) >> (i % 64)) & 1 != 0;
        }
        if m.neg {
            // 符号ビットが 1 なので、0 でないワードが必ずある。
            m.lowest = (0..a.len()).find(|&i| m.raw(i) != 0).unwrap_or(0);
        }
        m
    }

    /// X/Z を 0 にした値のワード `i`。
    #[inline]
    fn raw(&self, i: usize) -> u64 {
        let b = self.b.get(i).copied().unwrap_or(0);
        self.a[i] & !b
    }

    #[inline]
    fn len(&self) -> usize {
        self.a.len()
    }

    /// 絶対値のワード `i`。
    #[inline]
    fn word(&self, i: usize) -> u64 {
        let w = if !self.neg {
            self.raw(i)
        } else if i < self.lowest {
            0
        } else if i == self.lowest {
            self.raw(i).wrapping_neg()
        } else {
            !self.raw(i)
        };
        if i + 1 == self.len() {
            w & top_mask(self.width)
        } else {
            w
        }
    }

    /// 絶対値のビット `off` から `n` ビット (`n <= 64`)。範囲外のビットは 0。
    fn bits(&self, off: u64, n: u32) -> u64 {
        if n == 0 {
            return 0;
        }
        let wi = (off / 64) as usize;
        let b = (off % 64) as u32;
        let get = |i: usize| if i < self.len() { self.word(i) } else { 0 };
        let lo = get(wi) >> b;
        let hi = if b != 0 { get(wi + 1) << (64 - b) } else { 0 };
        let v = lo | hi;
        if n == 64 { v } else { v & ((1u64 << n) - 1) }
    }

    /// 絶対値を表すのに要るビット数 (0 なら 0)。
    fn bit_len(&self) -> u64 {
        (0..self.len())
            .rev()
            .map(|i| (i, self.word(i)))
            .find(|&(_, w)| w != 0)
            .map_or(0, |(i, w)| {
                64 * i as u64 + u64::from(64 - w.leading_zeros())
            })
    }

    /// ビット `0..n` に 1 があるか。
    fn any_below(&self, n: u64) -> bool {
        let full = (n / 64) as usize;
        if (0..full.min(self.len())).any(|i| self.word(i) != 0) {
            return true;
        }
        let r = (n % 64) as u32;
        r != 0 && full < self.len() && self.word(full) & ((1u64 << r) - 1) != 0
    }
}

/// 正の整数を形式 `fmt` に最近接偶数で 1 回だけ丸める。結果は `(q, e)` で値は `q · 2^e`
/// (`q < 2^precision`)。オーバフローなら `None` (∞、Q-5)。値が 0 なら `Some((0, 0))`。
fn round_magnitude(m: &Magnitude<'_>, fmt: &Format) -> Option<(u64, u64)> {
    let n = m.bit_len();
    let p = u64::from(fmt.precision);
    if n <= p {
        return Some((m.bits(0, fmt.precision), 0));
    }
    let mut shift = n - p;
    let mut q = m.bits(shift, fmt.precision);
    // 丸めのビット (shift - 1) と、それより下の sticky ビット。
    let round = m.bits(shift - 1, 1) != 0;
    let sticky = m.any_below(shift - 1);
    if round && (sticky || q & 1 != 0) {
        q += 1;
        if q >> fmt.precision != 0 {
            q >>= 1;
            shift += 1;
        }
    }
    // 最上位ビットの指数 = shift + precision - 1。
    if shift + p - 1 > fmt.max_exp {
        return None;
    }
    Some((q, shift))
}

fn to_f64_impl(m: &Magnitude<'_>) -> f64 {
    let mag = match round_magnitude(m, &F64) {
        None => f64::INFINITY,
        Some((q, e)) => {
            // q < 2^53 は f64 に正確に入る。2^e (e <= 971) は指数部を直接組んで作る。積は正確。
            q as f64 * f64::from_bits((e + 1023) << 52)
        }
    };
    if m.neg { -mag } else { mag }
}

fn to_f32_impl(m: &Magnitude<'_>) -> f32 {
    let mag = match round_magnitude(m, &F32) {
        None => f32::INFINITY,
        Some((q, e)) => {
            // q < 2^24 は f32 に正確に入る。2^e (e <= 104)。積は正確。
            q as f32 * f32::from_bits(((e + 127) as u32) << 23)
        }
    };
    if m.neg { -mag } else { mag }
}

/// 整数 → `real` (暗黙の変換、`$itor`。§6.12.1, §20.5)。
///
/// `signed` は `x` が符号付きか。X/Z のビットは 0 として扱う [要VCS確認 V-14]。
/// 仮数に収まらない値は最近接偶数で 1 回だけ丸め、範囲を超えれば ±∞ にする (Q-5)。
pub fn to_f64(x: LogicRef<'_>, signed: bool) -> f64 {
    to_f64_impl(&Magnitude::new(x.width(), x.aval(), x.bval(), signed))
}

/// 2 値の [`to_f64`]。
pub fn to_f64_2(x: BitRef<'_>, signed: bool) -> f64 {
    to_f64_impl(&Magnitude::new(x.width(), x.words(), &[], signed))
}

/// 整数 → `shortreal` (§6.12.1)。f64 を経由せず 1 回だけ丸める (R-12)。それ以外は [`to_f64`] と同じ。
pub fn to_f32(x: LogicRef<'_>, signed: bool) -> f32 {
    to_f32_impl(&Magnitude::new(x.width(), x.aval(), x.bval(), signed))
}

/// 2 値の [`to_f32`]。
pub fn to_f32_2(x: BitRef<'_>, signed: bool) -> f32 {
    to_f32_impl(&Magnitude::new(x.width(), x.words(), &[], signed))
}

// ===========================================================================
// 実数 → 整数
// ===========================================================================

/// 実数 → 整数の変換の状態 (test-plan Q-6)。
///
/// 幅 `w` の dst に整数 `n` が「収まる」とは、`-2^(w-1) <= n <= 2^w - 1` (符号付きの範囲か
/// 符号なしの範囲のどちらか) であること。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RealToIntStatus {
    /// 値が整数で、dst に収まった。
    Exact,
    /// 小数部を丸めた (または切り捨てた) が、丸めた後の整数は dst に収まった。
    Rounded,
    /// 丸めた後の整数が dst に収まらず、2 の補数で切り詰めた (丸めの有無によらない)。
    Wrapped,
    /// NaN または ±∞。結果は 0 [要VCS確認 V-4]。
    NotFinite,
}

/// 丸め方。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Rounding {
    /// 最近接、ちょうど半分は 0 から遠い方へ (§6.12.1)。
    HalfAwayFromZero,
    /// 0 方向へ切り捨て (§20.5 `$rtoi`)。
    TowardZero,
}

/// 有限の `v` を丸めた整数を、`m · 2^e` の絶対値と符号で表したもの。
struct RoundedInt {
    neg: bool,
    m: u64,
    e: u32,
    /// 小数部があった (丸めた・切り捨てた)。
    inexact: bool,
}

fn round_f64(v: f64, mode: Rounding) -> RoundedInt {
    // IEEE 754 binary64: 符号 1、指数 11、仮数 52 ビット。値 = mant · 2^exp (正確)。
    let bits = v.to_bits();
    let neg = bits >> 63 == 1;
    let biased = ((bits >> 52) & 0x7ff) as i64;
    let frac = bits & ((1u64 << 52) - 1);
    let (mant, exp) = if biased == 0 {
        (frac, -1074i64) // 非正規化数と ±0
    } else {
        (frac | (1u64 << 52), biased - 1075)
    };
    if exp >= 0 {
        return RoundedInt {
            neg,
            m: mant,
            e: exp as u32,
            inexact: false,
        };
    }
    let sh = (-exp) as u64;
    let (q, rest) = if sh >= 64 {
        (0, mant)
    } else {
        (mant >> sh, mant & ((1u64 << sh) - 1))
    };
    // ちょうど半分以上なら丸め上げる (0 から遠い方へ。偶数丸めではない)。
    // 半分のビット (sh - 1) は sh <= 64 のときだけ mant の中にある。
    let half_or_more = sh <= 64 && (mant >> (sh - 1)) & 1 != 0;
    let up = mode == Rounding::HalfAwayFromZero && half_or_more;
    RoundedInt {
        neg,
        m: q + u64::from(up),
        e: 0,
        inexact: rest != 0,
    }
}

/// 丸めた整数を幅 `width` のプレーン `dst` に 2 の補数で書き (全ワードを上書きする)、状態を返す。
fn write_rounded(dst: &mut [u64], width: u32, r: &RoundedInt) -> RealToIntStatus {
    dst.fill(0);
    let total = 64 * dst.len() as u64;
    let e = u64::from(r.e);
    if r.m != 0 && e < total {
        let wi = (e / 64) as usize;
        let b = (e % 64) as u32;
        dst[wi] = r.m << b;
        if b != 0 && wi + 1 < dst.len() {
            dst[wi + 1] = r.m >> (64 - b);
        }
    }
    if r.neg {
        words::neg_assign(dst);
    }
    normalize_plane(width, dst);

    // 収まるか: 正なら bit_len <= w、負なら |n| <= 2^(w-1)。
    let fits = if r.m == 0 {
        true
    } else {
        let len = e + u64::from(64 - r.m.leading_zeros());
        let w = u64::from(width);
        if !r.neg {
            len <= w
        } else {
            w > 0 && (len < w || (len == w && r.m.is_power_of_two()))
        }
    };
    if !fits {
        RealToIntStatus::Wrapped
    } else if r.inexact {
        RealToIntStatus::Rounded
    } else {
        RealToIntStatus::Exact
    }
}

fn from_f64_plane(dst: &mut [u64], width: u32, v: f64, mode: Rounding) -> RealToIntStatus {
    if !v.is_finite() {
        dst.fill(0);
        return RealToIntStatus::NotFinite;
    }
    write_rounded(dst, width, &round_f64(v, mode))
}

/// 実数 → 整数 (暗黙の変換、キャスト。§6.12.1)。
///
/// 最も近い整数に丸め、ちょうど半分のときは 0 から遠い方へ丸める (35.5 → 36、-1.5 → -2)。
/// 結果は dst の幅の 2 の補数に切り詰める。dst は任意の幅でよく、64 ビットを超えても正確。
/// NaN/±∞ は 0 にして [`RealToIntStatus::NotFinite`] を返す [要VCS確認 V-4]。bval は 0 になる。
pub fn from_f64_round(dst: LogicMut<'_>, v: f64) -> RealToIntStatus {
    let w = dst.width();
    let (a, b) = dst.into_planes();
    b.fill(0);
    from_f64_plane(a, w, v, Rounding::HalfAwayFromZero)
}

/// 2 値の [`from_f64_round`]。
pub fn from_f64_round2(dst: BitMut<'_>, v: f64) -> RealToIntStatus {
    let w = dst.width();
    from_f64_plane(dst.into_words(), w, v, Rounding::HalfAwayFromZero)
}

/// `$rtoi` (§20.5)。0 方向へ切り捨てる。それ以外は [`from_f64_round`] と同じ。
/// 小数部を捨てた場合の状態は [`RealToIntStatus::Rounded`] (STATUS の決定)。
pub fn from_f64_trunc(dst: LogicMut<'_>, v: f64) -> RealToIntStatus {
    let w = dst.width();
    let (a, b) = dst.into_planes();
    b.fill(0);
    from_f64_plane(a, w, v, Rounding::TowardZero)
}

/// 2 値の [`from_f64_trunc`]。
pub fn from_f64_trunc2(dst: BitMut<'_>, v: f64) -> RealToIntStatus {
    let w = dst.width();
    from_f64_plane(dst.into_words(), w, v, Rounding::TowardZero)
}

// ===========================================================================
// ビットパターン (§20.5)
// ===========================================================================

/// `$realtobits` (§20.5)。ビットパターンをそのまま返す。
#[inline]
pub fn real_to_bits(v: f64) -> u64 {
    v.to_bits()
}

/// `$bitstoreal` (§20.5)。NaN のビットもそのまま保つ。
#[inline]
pub fn bits_to_real(bits: u64) -> f64 {
    f64::from_bits(bits)
}

/// `$shortrealtobits` (§20.5)。
#[inline]
pub fn shortreal_to_bits(v: f32) -> u32 {
    v.to_bits()
}

/// `$bitstoshortreal` (§20.5)。
#[inline]
pub fn bits_to_shortreal(bits: u32) -> f32 {
    f32::from_bits(bits)
}

// ===========================================================================
// 層 3
// ===========================================================================

impl LogicVec {
    /// 整数 → `real` ([`to_f64`])。
    pub fn to_f64(&self, signed: bool) -> f64 {
        to_f64(self.as_ref(), signed)
    }

    /// 整数 → `shortreal` ([`to_f32`])。
    pub fn to_f32(&self, signed: bool) -> f32 {
        to_f32(self.as_ref(), signed)
    }

    /// 実数 → 幅 `width` の整数 ([`from_f64_round`])。
    pub fn from_f64_round(width: u32, v: f64) -> (LogicVec, RealToIntStatus) {
        let mut r = LogicVec::zero(width);
        let s = from_f64_round(r.as_mut(), v);
        (r, s)
    }

    /// `$rtoi` で幅 `width` の整数にする ([`from_f64_trunc`])。
    pub fn from_f64_trunc(width: u32, v: f64) -> (LogicVec, RealToIntStatus) {
        let mut r = LogicVec::zero(width);
        let s = from_f64_trunc(r.as_mut(), v);
        (r, s)
    }
}

impl BitVec {
    /// 整数 → `real` ([`to_f64_2`])。
    pub fn to_f64(&self, signed: bool) -> f64 {
        to_f64_2(self.as_ref(), signed)
    }

    /// 整数 → `shortreal` ([`to_f32_2`])。
    pub fn to_f32(&self, signed: bool) -> f32 {
        to_f32_2(self.as_ref(), signed)
    }

    /// 実数 → 幅 `width` の 2 値の整数 ([`from_f64_round2`])。
    pub fn from_f64_round(width: u32, v: f64) -> (BitVec, RealToIntStatus) {
        let mut r = BitVec::zero(width);
        let s = from_f64_round2(r.as_mut(), v);
        (r, s)
    }

    /// `$rtoi` で幅 `width` の 2 値の整数にする ([`from_f64_trunc2`])。
    pub fn from_f64_trunc(width: u32, v: f64) -> (BitVec, RealToIntStatus) {
        let mut r = BitVec::zero(width);
        let s = from_f64_trunc2(r.as_mut(), v);
        (r, s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use RealToIntStatus::*;

    fn u(width: u32, v: u64) -> LogicVec {
        LogicVec::from_u64(width, v)
    }

    /// 幅 `w` で、ビット `k` だけ 1。
    fn pow2(w: u32, k: u32) -> LogicVec {
        let mut x = LogicVec::zero(w);
        x.set_bit(k, crate::Logic::L1);
        x
    }

    /// §6.12.1: 整数 → 実数は最近接偶数で 1 回だけ丸める (R-12)。
    #[test]
    fn int_to_real_rounding_6_12_1() {
        assert_eq!(u(32, 0xffff_ffff).to_f64(true), -1.0);
        assert_eq!(u(32, 0xffff_ffff).to_f64(false), 4294967295.0);
        assert_eq!(u(64, u64::MAX).to_f64(false), 18446744073709551616.0);
        assert_eq!(u(64, (1 << 53) + 1).to_f64(false), 9007199254740992.0);
        assert_eq!(u(64, (1 << 53) + 3).to_f64(false), 9007199254740996.0);
        assert_eq!(pow2(128, 100).to_f64(false), 2f64.powi(100));
        assert_eq!(u(64, 1 << 63).to_f32(true), -(2f32.powi(63)));
        // 二重丸めの検出 (R-12): 2^54 + 2^30 + 1 は f32 で 2^54 + 2^31。
        let v = (1u64 << 54) + (1 << 30) + 1;
        assert_eq!(u(64, v).to_f32(false), 18014400656965632.0);
        assert_eq!(u(64, (1 << 24) + 1).to_f32(false), 16777216.0);
        assert_eq!(LogicVec::zero(77).to_f64(true), 0.0);
        assert_eq!(LogicVec::zero(0).to_f64(true), 0.0);
    }

    /// Q-5: 範囲を超える値は ±∞。
    #[test]
    fn int_to_real_overflow_q5() {
        let ones = LogicVec::filled(1024, crate::Logic::L1);
        assert_eq!(ones.to_f64(false), f64::INFINITY);
        assert_eq!(ones.to_f64(true), -1.0);
        assert_eq!(pow2(1025, 1024).to_f64(true), f64::NEG_INFINITY);
        assert_eq!(pow2(129, 128).to_f32(false), f32::INFINITY);
    }

    /// V-14: X/Z のビットは 0 として扱う。
    #[test]
    fn int_to_real_xz_as_zero_v14() {
        let x = LogicVec::from_bits_str("1x01").unwrap();
        assert_eq!(x.to_f64(true), -7.0);
        let x = LogicVec::from_bits_str("00001x01").unwrap();
        assert_eq!(x.to_f64(false), 9.0);
        // 符号ビットが x なら正 (0 として扱う)。
        let x = LogicVec::from_bits_str("x111").unwrap();
        assert_eq!(x.to_f64(true), 7.0);
    }

    /// §6.12.1: 実数 → 整数は最も近い整数に、半分は 0 から遠い方へ丸める (Q-6 の状態)。
    #[test]
    fn real_to_int_6_12_1() {
        let r = |w, v| {
            let (x, s) = LogicVec::from_f64_round(w, v);
            (x.to_i64(true).unwrap(), s)
        };
        assert_eq!(r(32, 35.5), (36, Rounded));
        assert_eq!(r(32, 35.4), (35, Rounded));
        assert_eq!(r(32, 36.0), (36, Exact));
        assert_eq!(r(32, -1.5), (-2, Rounded));
        assert_eq!(r(32, -2.5), (-3, Rounded));
        assert_eq!(r(32, 0.5), (1, Rounded));
        assert_eq!(r(32, -0.5), (-1, Rounded));
        assert_eq!(r(32, 0.49999999999999994), (0, Rounded));
        assert_eq!(r(32, -0.0), (0, Exact));
        assert_eq!(r(32, 5e-324), (0, Rounded));
        assert_eq!(r(8, -1.0), (-1, Exact));
        assert_eq!(r(8, 255.5), (0, Wrapped));
        assert_eq!(r(8, -129.0), (127, Wrapped));
        assert_eq!(r(8, -128.0), (-128, Exact));
        assert_eq!(r(32, f64::NAN), (0, NotFinite));
        let (x, s) = LogicVec::from_f64_round(64, 1e20);
        assert_eq!((x.to_u64().unwrap(), s), (7766279631452241920, Wrapped));
        let (x, s) = LogicVec::from_f64_round(128, -(2f64.powi(100)));
        assert_eq!(s, Exact);
        assert_eq!(x.to_f64(true), -(2f64.powi(100)));
        let t = |w, v| {
            let (x, s) = LogicVec::from_f64_trunc(w, v);
            (x.to_i64(true).unwrap(), s)
        };
        assert_eq!(t(32, 35.9), (35, Rounded));
        assert_eq!(t(32, -35.9), (-35, Rounded));
        assert_eq!(t(32, -0.5), (0, Rounded));
        assert_eq!(t(32, 7.0), (7, Exact));
        assert_eq!(t(8, 256.7), (0, Wrapped));
        let (b, s) = BitVec::from_f64_round(8, 300.0);
        assert_eq!((b.to_u64().unwrap(), s), (44, Wrapped));
        // 幅 0: 0 だけが収まる。
        assert_eq!(LogicVec::from_f64_round(0, 0.0).1, Exact);
        assert_eq!(LogicVec::from_f64_round(0, 1.0).1, Wrapped);
        assert_eq!(LogicVec::from_f64_round(0, -1.0).1, Wrapped);
        // 1 ビット: -1 と 1 は収まる、2 は収まらない。
        assert_eq!(r(1, -1.0), (-1, Exact));
        assert_eq!(r(1, 1.0), (-1, Exact));
        assert_eq!(LogicVec::from_f64_round(1, 2.0).1, Wrapped);
    }

    /// §20.5: ビットパターンの変換。
    #[test]
    fn bits_20_5() {
        assert_eq!(real_to_bits(1.0), 0x3ff0_0000_0000_0000);
        assert_eq!(bits_to_real(0x3ff0_0000_0000_0000), 1.0);
        assert_eq!(shortreal_to_bits(1.0), 0x3f80_0000);
        assert_eq!(bits_to_shortreal(0x3f80_0000), 1.0);
        assert_eq!(real_to_bits(-0.0), 0x8000_0000_0000_0000);
        let nan = 0x7ff8_0000_dead_beefu64;
        assert_eq!(real_to_bits(bits_to_real(nan)), nan);
    }
}
