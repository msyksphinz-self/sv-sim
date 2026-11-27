//! 算術演算 (plan.md §3.5(c)、IEEE 1800-2023 §11.4.3)。
//!
//! どのオペランドにも X/Z が無ければ `BigInt` に変換して数学的に計算し、
//! `2^w` の剰余 (2 の補数) を取って幅 `w` に戻す。X/Z があれば全 X。

use num_bigint::{BigInt, BigUint};
use num_traits::{One, Signed, Zero};

use crate::value::RefVec;

fn check_widths(x: &RefVec, y: &RefVec) {
    assert_eq!(x.width(), y.width(), "operand widths must match (CD)");
}

/// 2 項の算術の共通部分。未知があれば全 X、`op` が `None` を返したら (0 除算) 全 X。
fn binary(
    x: &RefVec,
    y: &RefVec,
    signed: bool,
    op: impl FnOnce(&BigInt, &BigInt) -> Option<BigInt>,
) -> RefVec {
    check_widths(x, y);
    let w = x.width();
    match (x.to_bigint(signed), y.to_bigint(signed)) {
        (Some(a), Some(b)) => match op(&a, &b) {
            Some(r) => RefVec::from_bigint(w, &r),
            None => RefVec::all_x(w),
        },
        _ => RefVec::all_x(w),
    }
}

/// `x + y` (§11.4.3)。オーバフローは捨てる。
pub fn add(x: &RefVec, y: &RefVec) -> RefVec {
    binary(x, y, false, |a, b| Some(a + b))
}

/// `x - y` (§11.4.3)。
pub fn sub(x: &RefVec, y: &RefVec) -> RefVec {
    binary(x, y, false, |a, b| Some(a - b))
}

/// 単項 `-x` (§11.4.3) = `0 - x`。
pub fn neg(x: &RefVec) -> RefVec {
    sub(&RefVec::zeros(x.width()), x)
}

/// `x * y` (§11.4.3)。下位 `w` ビットは符号の有無に依らない。
pub fn mul(x: &RefVec, y: &RefVec) -> RefVec {
    binary(x, y, false, |a, b| Some(a * b))
}

/// `x / y` (§11.4.3)。除数 0 なら全 X。0 方向へ切り捨てる (`BigInt` の `/`)。
/// 符号付きの最小値 / -1 は、数学的な商 `2^(w-1)` を `2^w` で折り返すので最小値になる
/// [要VCS確認 V-1]。
pub fn div(x: &RefVec, y: &RefVec, signed: bool) -> RefVec {
    binary(
        x,
        y,
        signed,
        |a, b| if b.is_zero() { None } else { Some(a / b) },
    )
}

/// `x % y` (§11.4.3)。除数 0 なら全 X。結果の符号は第 1 オペランドの符号 (`BigInt` の `%`)。
pub fn rem(x: &RefVec, y: &RefVec, signed: bool) -> RefVec {
    binary(
        x,
        y,
        signed,
        |a, b| if b.is_zero() { None } else { Some(a % b) },
    )
}

/// `base ** exp` (§11.4.3 のべき乗の規則)。結果の幅は base の幅、exp は自己決定。
///
/// - どちらかに X/Z があれば全 X。
/// - exp が 0 なら 1 (base が 0 でも 1)。
/// - exp が負 (`exp_signed` で MSB が 1) のとき:
///   base = 0 なら全 X、base = 1 なら 1、base = -1 (`base_signed` のとき) なら
///   exp が偶数で 1・奇数で -1、|base| > 1 なら 0。
/// - exp が正なら `base^exp mod 2^w`。
pub fn pow(base: &RefVec, base_signed: bool, exp: &RefVec, exp_signed: bool) -> RefVec {
    let w = base.width();
    if w == 0 {
        // 幅 0 は連結の中でだけ使う値 (plan.md §3.2)。演算には来ないが、modpow の 0 剰余を避ける。
        return RefVec::zeros(0);
    }
    let (Some(b), Some(e)) = (base.to_bigint(base_signed), exp.to_bigint(exp_signed)) else {
        return RefVec::all_x(w);
    };
    if e.is_zero() {
        return RefVec::from_bigint(w, &BigInt::one());
    }
    if e.is_negative() {
        let minus_one = -BigInt::one();
        return if b.is_zero() {
            RefVec::all_x(w)
        } else if b.is_one() {
            RefVec::from_bigint(w, &BigInt::one())
        } else if b == minus_one {
            let odd = (&e % 2u32) != BigInt::zero();
            RefVec::from_bigint(w, &if odd { minus_one } else { BigInt::one() })
        } else {
            RefVec::zeros(w)
        };
    }
    // e > 0: 2^w の剰余環の上で計算する (base の剰余類は符号の解釈に依らない)。
    let modulus = BigUint::one() << w;
    let b_mod = base.to_biguint().expect("checked above");
    let e_u = e.to_biguint().expect("positive");
    let r = b_mod.modpow(&e_u, &modulus);
    RefVec::from_biguint(w, &r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> RefVec {
        // 読みやすさのためテストでは '_' で区切って書き、渡す前に取り除く (bits 文字列は '_' を受けない、Q-22)。
        RefVec::from_bits_str(&s.replace('_', "")).unwrap()
    }

    #[test]
    fn width_one_pow() {
        // 幅 1 の符号付き base: '1' は -1。
        assert_eq!(pow(&v("1"), true, &v("11"), true).to_bits_string(), "1"); // (-1)**(-1) = -1
        assert_eq!(pow(&v("1"), true, &v("10"), true).to_bits_string(), "1"); // (-1)**(-2) = 1 → 1 ビットでは '1'
        assert_eq!(pow(&v("0"), true, &v("1"), true).to_bits_string(), "x"); // 0**(-1)
    }
}
