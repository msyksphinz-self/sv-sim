//! 実数との変換 (plan.md §3.5(i)、IEEE 1800-2023 §6.12.1, §20.5)。
//!
//! - 整数 → 実数: `BigUint` の値から、IEEE 754 の最近接偶数丸めで 1 回だけ丸める (R-12)。
//!   f64 用と f32 用は、仮数のビット数と指数の範囲だけが違う同じ手順で、`f64` を経由しない。
//! - 実数 → 整数: `f64` を符号・指数・仮数に分解し、`BigUint` で正確に扱ってから丸める。

use num_bigint::{BigInt, BigUint, Sign};
use num_traits::{One, ToPrimitive, Zero};

use crate::value::RefVec;

/// 2 進浮動小数点形式の 2 つのパラメタ。
struct Format {
    /// 仮数のビット数 (暗黙の 1 を含む)。f64 は 53、f32 は 24。
    precision: u64,
    /// 指数の最大値 (正規化数の最大指数)。f64 は 1023、f32 は 127。
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

/// 正の整数 `m` を形式 `fmt` に最近接偶数で丸める。結果は (仮数 q, 指数 e) で、値は `q * 2^e`。
/// q は `precision` ビット以下。オーバフローなら `None` (∞)。
fn round_to_format(m: &BigUint, fmt: &Format) -> Option<(u64, u64)> {
    let n = m.bits();
    if n <= fmt.precision {
        return Some((m.to_u64().expect("fits"), 0));
    }
    let shift = n - fmt.precision;
    let mut q: BigUint = m >> shift;
    let rem: BigUint = m - (&q << shift);
    let half = BigUint::one() << (shift - 1);
    let odd = q.bit(0);
    if rem > half || (rem == half && odd) {
        q += 1u32;
    }
    let mut e = shift;
    // 丸め上げで 2^precision になったら 1 桁ずらす。
    if q.bits() > fmt.precision {
        q >>= 1u32;
        e += 1;
    }
    // 最上位ビットの位置 (= 値の 2 進指数) が最大指数を超えたら ∞。
    if e + fmt.precision - 1 > fmt.max_exp {
        return None;
    }
    Some((q.to_u64().expect("fits"), e))
}

/// 整数の値 (X/Z は 0 として扱う) を、符号と大きさに分ける。
fn sign_and_magnitude(x: &RefVec, signed: bool) -> (bool, BigUint) {
    let v = x.to_bigint_xz_as_zero(signed);
    let neg = v.sign() == Sign::Minus;
    (neg, v.magnitude().clone())
}

/// 整数 → `real` (§6.12.1, §20.5 `$itor`)。X/Z のビットは 0 として扱う [要VCS確認 V-14]。
pub fn to_f64(x: &RefVec, signed: bool) -> f64 {
    let (neg, m) = sign_and_magnitude(x, signed);
    let mag = if m.is_zero() {
        0.0
    } else {
        match round_to_format(&m, &F64) {
            None => f64::INFINITY,
            Some((q, e)) => {
                // q は 53 ビット以下の整数で f64 に正確に入る。2^e は指数部を直接組んで作る (正確)。
                let p = f64::from_bits((e + 1023) << 52);
                q as f64 * p
            }
        }
    };
    if neg { -mag } else { mag }
}

/// 整数 → `shortreal` (§6.12.1)。f64 を経由せず 1 回だけ丸める (R-12)。
pub fn to_f32(x: &RefVec, signed: bool) -> f32 {
    let (neg, m) = sign_and_magnitude(x, signed);
    let mag = if m.is_zero() {
        0.0
    } else {
        match round_to_format(&m, &F32) {
            None => f32::INFINITY,
            Some((q, e)) => {
                let p = f32::from_bits(((e + 127) as u32) << 23);
                q as f32 * p
            }
        }
    };
    if neg { -mag } else { mag }
}

/// 実数 → 整数の変換の状態。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RefRealToIntStatus {
    /// 有限の値を変換した。
    Ok,
    /// NaN または ±∞。結果は 0 [要VCS確認 V-4]。
    NotFinite,
}

/// 丸め方。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Rounding {
    /// 最近接、半分は 0 から遠い方へ (§6.12.1)。
    HalfAwayFromZero,
    /// 0 方向へ切り捨て (§20.5 `$rtoi`)。
    TowardZero,
}

fn from_f64(width: usize, v: f64, mode: Rounding) -> (RefVec, RefRealToIntStatus) {
    if !v.is_finite() {
        return (RefVec::zeros(width), RefRealToIntStatus::NotFinite);
    }
    // IEEE 754 binary64 を分解する: 符号 1、指数 11、仮数 52 ビット。
    let bits = v.to_bits();
    let neg = bits >> 63 == 1;
    let biased = ((bits >> 52) & 0x7ff) as i64;
    let frac = bits & ((1u64 << 52) - 1);
    // 値 = mant * 2^exp (正確)。
    let (mant, exp) = if biased == 0 {
        (frac, -1074i64) // 非正規化数 (と 0)
    } else {
        (frac | (1u64 << 52), biased - 1075)
    };
    let mant = BigUint::from(mant);
    let mag: BigUint = if exp >= 0 {
        mant << (exp as u64)
    } else {
        let sh = (-exp) as u64;
        let q: BigUint = &mant >> sh;
        let rem: BigUint = &mant - (&q << sh);
        let half = BigUint::one() << (sh - 1);
        match mode {
            Rounding::HalfAwayFromZero if rem >= half => q + 1u32,
            _ => q,
        }
    };
    let signed = BigInt::from_biguint(if neg { Sign::Minus } else { Sign::Plus }, mag);
    (RefVec::from_bigint(width, &signed), RefRealToIntStatus::Ok)
}

/// 実数 → 整数 (暗黙の変換、キャスト、§6.12.1)。最も近い整数に丸め、ちょうど半分は 0 から遠い方へ。
/// 結果は幅 `width` の 2 の補数に切り詰める。
pub fn from_f64_round(width: usize, v: f64) -> (RefVec, RefRealToIntStatus) {
    from_f64(width, v, Rounding::HalfAwayFromZero)
}

/// `$rtoi` (§20.5)。0 方向へ切り捨てる。
pub fn from_f64_trunc(width: usize, v: f64) -> (RefVec, RefRealToIntStatus) {
    from_f64(width, v, Rounding::TowardZero)
}
