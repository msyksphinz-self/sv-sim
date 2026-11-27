//! 参照モデルのゴールデンテスト: 算術 (IEEE 1800-2023 §11.4.3)。

use svsim_logic_ref::*;

fn v(s: &str) -> RefVec {
    // 読みやすさのためテストでは '_' で区切って書き、渡す前に取り除く (bits 文字列は '_' を受けない、Q-22)。
    RefVec::from_bits_str(&s.replace('_', "")).unwrap()
}

fn i(w: usize, x: i64) -> RefVec {
    RefVec::from_i64(w, x)
}

// §11.4.3: 加減乗算は幅 w で 2 の補数として切り詰める。未知があれば全 x。
#[test]
fn s11_4_3_add_sub_mul() {
    assert_eq!(add(&v("1111"), &v("0001")).to_bits_string(), "0000");
    assert_eq!(add(&v("0101"), &v("0011")).to_bits_string(), "1000");
    assert_eq!(sub(&v("0000"), &v("0001")).to_bits_string(), "1111");
    assert_eq!(neg(&v("0001")).to_bits_string(), "1111");
    assert_eq!(neg(&v("1000")).to_bits_string(), "1000");
    assert_eq!(mul(&v("0111"), &v("0011")).to_bits_string(), "0101"); // 21 mod 16
    assert_eq!(mul(&i(8, -3), &i(8, 5)), i(8, -15));
    assert_eq!(add(&v("0001"), &v("000z")).to_bits_string(), "xxxx");
    assert_eq!(mul(&v("x000"), &v("0000")).to_bits_string(), "xxxx");
    assert_eq!(neg(&v("1x")).to_bits_string(), "xx");
}

// §11.4.3 の除算・剰余の例。剰余の符号は第 1 オペランドの符号。
#[test]
fn s11_4_3_modulus_examples() {
    let w = 32;
    assert_eq!(rem(&i(w, 10), &i(w, 3), true), i(w, 1));
    assert_eq!(rem(&i(w, 11), &i(w, 3), true), i(w, 2));
    assert_eq!(rem(&i(w, 12), &i(w, 3), true), i(w, 0));
    assert_eq!(rem(&i(w, -10), &i(w, 3), true), i(w, -1));
    assert_eq!(rem(&i(w, 11), &i(w, -3), true), i(w, 2));
    // -4'd12 % 3: 4 ビット符号なしで -12 は 4。4 % 3 = 1。
    assert_eq!(rem(&neg(&i(4, 12)), &i(4, 3), false), i(4, 1));
}

// §11.4.3 (整数の算術の例): -12 / 3 等。
#[test]
fn s11_4_3_division_examples() {
    let w = 32;
    // integer: -12 / 3 = -4
    assert_eq!(div(&i(w, -12), &i(w, 3), true), i(w, -4));
    // -'d12 / 3: 32 ビット符号なし。(2^32 - 12) / 3 = 1431655761
    assert_eq!(
        div(&neg(&i(w, 12)), &i(w, 3), false),
        RefVec::from_u64(w, 1_431_655_761)
    );
    // -'sd12 / 3 = -4
    assert_eq!(div(&neg(&i(w, 12)), &i(w, 3), true), i(w, -4));
    // -4'sd12 / 3: 4'sd12 は -4、その符号反転は 4、4 / 3 = 1
    assert_eq!(div(&neg(&v("1100")), &i(4, 3), true), i(4, 1));
    // 0 方向への切り捨て
    assert_eq!(div(&i(8, -7), &i(8, 2), true), i(8, -3));
    assert_eq!(div(&i(8, 7), &i(8, -2), true), i(8, -3));
    assert_eq!(rem(&i(8, -7), &i(8, 2), true), i(8, -1));
}

// §11.4.3: 除数 0 は全 x。
#[test]
fn s11_4_3_divide_by_zero() {
    assert_eq!(div(&v("0101"), &v("0000"), false).to_bits_string(), "xxxx");
    assert_eq!(rem(&v("0101"), &v("0000"), true).to_bits_string(), "xxxx");
    assert_eq!(div(&v("0101"), &v("00x1"), false).to_bits_string(), "xxxx");
}

// §11.4.3: 符号付きの最小値 / -1 は 2 の補数で折り返して最小値、剰余は 0 [要VCS確認 V-1]。
#[test]
fn s11_4_3_min_div_minus_one_vcs_v1() {
    assert_eq!(
        div(&v("1000_0000"), &i(8, -1), true).to_bits_string(),
        "10000000"
    );
    assert_eq!(
        rem(&v("1000_0000"), &i(8, -1), true).to_bits_string(),
        "00000000"
    );
}

// §11.4.3 のべき乗の規則 (op1 が <-1, -1, 0, 1, >1 × op2 が正, 0, 負)。
#[test]
fn s11_4_3_power_rules() {
    let w = 8;
    let p = |b: i64, e: i64| pow(&i(w, b), true, &i(w, e), true);
    // op2 が正: op1 ** op2
    assert_eq!(p(-2, 3), i(w, -8));
    assert_eq!(p(-1, 3), i(w, -1));
    assert_eq!(p(0, 3), i(w, 0));
    assert_eq!(p(1, 3), i(w, 1));
    assert_eq!(p(2, 3), i(w, 8));
    assert_eq!(p(3, 5), i(w, 243 - 256)); // 243 mod 256 (符号付きで -13)
    // op2 が 0: 1
    for b in [-2, -1, 0, 1, 2] {
        assert_eq!(p(b, 0), i(w, 1), "{b} ** 0");
    }
    // op2 が負
    assert_eq!(p(-2, -1), i(w, 0));
    assert_eq!(p(-1, -1), i(w, -1)); // 奇数 → -1
    assert_eq!(p(-1, -2), i(w, 1)); // 偶数 → 1
    assert_eq!(p(0, -1).to_bits_string(), "xxxxxxxx");
    assert_eq!(p(1, -3), i(w, 1));
    assert_eq!(p(2, -1), i(w, 0));
}

// §11.4.3: べき乗の符号の扱い。指数が符号なしなら負にならない。base が符号なしなら -1 にならない。
#[test]
fn s11_4_3_power_signedness() {
    // 指数 8'hff: 符号なしなら 255、符号付きなら -1。
    assert_eq!(
        pow(&i(8, 2), false, &v("1111_1111"), false),
        i(8, 0) // 2**255 mod 256
    );
    assert_eq!(pow(&i(8, 3), false, &v("1111_1111"), true), i(8, 0)); // 3 ** -1 = 0
    // base 8'hff: 符号なしなら 255 (|base|>1)、符号付きなら -1。
    assert_eq!(pow(&v("1111_1111"), false, &i(4, -1), true), i(8, 0));
    assert_eq!(pow(&v("1111_1111"), true, &i(4, -1), true), i(8, -1));
    // 3 ** 4 = 81、指数の幅は base と別 (自己決定)
    assert_eq!(pow(&i(8, 3), false, &v("100"), false), i(8, 81));
    // 未知
    assert_eq!(
        pow(&i(4, 2), false, &v("1x"), false).to_bits_string(),
        "xxxx"
    );
    assert_eq!(
        pow(&v("00z1"), false, &v("00"), false).to_bits_string(),
        "xxxx"
    );
}

// 幅 64 を超える算術 (多倍長)。
#[test]
fn wide_arith() {
    let w = 130;
    let big = RefVec::from_bits_str(&"1".repeat(w)).unwrap(); // 2^130 - 1 (符号付きでは -1)
    assert_eq!(add(&big, &RefVec::from_u64(w, 1)), RefVec::zeros(w));
    assert_eq!(mul(&big, &big), RefVec::from_u64(w, 1)); // (-1)*(-1)
    assert_eq!(div(&big, &RefVec::from_u64(w, 2), true), RefVec::zeros(w)); // -1 / 2 = 0
    let mut two_pow_100 = RefVec::zeros(w);
    two_pow_100.0[100] = RefLogic::One;
    assert_eq!(
        pow(&RefVec::from_u64(w, 2), false, &v("1100100"), false),
        two_pow_100
    );
}
