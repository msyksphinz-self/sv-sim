//! 参照モデルのゴールデンテスト: 実数との変換 (IEEE 1800-2023 §6.12.1, §20.5) と
//! 表示用フォーマット (§21.2.1、plan.md §3.9)。

use num_bigint::{BigInt, BigUint};
use svsim_logic_ref::*;

fn v(s: &str) -> RefVec {
    // 読みやすさのためテストでは '_' で区切って書き、渡す前に取り除く (bits 文字列は '_' を受けない、Q-22)。
    RefVec::from_bits_str(&s.replace('_', "")).unwrap()
}

fn from_big(width: usize, x: &BigUint) -> RefVec {
    RefVec::from_biguint(width, x)
}

fn pow2(n: u32) -> BigUint {
    BigUint::from(1u32) << n
}

use RefRealToIntStatus::{Exact, NotFinite, Rounded, Wrapped};

/// `from_f64_round` の (値, 状態) を、期待値の (2 の補数の符号付き整数, 状態) と比べる。
#[track_caller]
fn check_round(width: usize, x: f64, value: i128, st: RefRealToIntStatus) {
    let (got, got_st) = from_f64_round(width, x);
    assert_eq!(
        got,
        RefVec::from_bigint(width, &BigInt::from(value)),
        "round({width}, {x}) の値"
    );
    assert_eq!(got_st, st, "round({width}, {x}) の状態");
}

#[track_caller]
fn check_trunc(width: usize, x: f64, value: i128, st: RefRealToIntStatus) {
    let (got, got_st) = from_f64_trunc(width, x);
    assert_eq!(
        got,
        RefVec::from_bigint(width, &BigInt::from(value)),
        "trunc({width}, {x}) の値"
    );
    assert_eq!(got_st, st, "trunc({width}, {x}) の状態");
}

// §6.12.1: 実数 → 整数は最も近い整数へ、ちょうど半分は 0 から遠い方へ。
#[test]
fn s6_12_1_real_to_int_rounding() {
    let r = |x: f64| {
        let (v, st) = from_f64_round(32, x);
        let expected = if x.fract() == 0.0 { Exact } else { Rounded };
        assert_eq!(st, expected, "{x}");
        v
    };
    assert_eq!(r(35.7), RefVec::from_i64(32, 36));
    assert_eq!(r(35.5), RefVec::from_i64(32, 36));
    assert_eq!(r(35.2), RefVec::from_i64(32, 35));
    assert_eq!(r(-1.5), RefVec::from_i64(32, -2));
    assert_eq!(r(1.5), RefVec::from_i64(32, 2));
    assert_eq!(r(-0.4), RefVec::from_i64(32, 0));
    assert_eq!(r(-0.5), RefVec::from_i64(32, -1));
    assert_eq!(r(0.0), RefVec::from_i64(32, 0));
    assert_eq!(r(f64::MIN_POSITIVE / 4.0), RefVec::from_i64(32, 0)); // 非正規化数
    // 幅で 2 の補数として切り詰める
    assert_eq!(from_f64_round(4, 17.0).0, RefVec::from_i64(4, 1));
    assert_eq!(from_f64_round(4, -1.0).0.to_bits_string(), "1111");
}

// §20.5: $rtoi は 0 方向へ切り捨てる。
#[test]
fn s20_5_rtoi() {
    let t = |x: f64| from_f64_trunc(32, x).0;
    assert_eq!(t(35.7), RefVec::from_i64(32, 35));
    assert_eq!(t(-1.5), RefVec::from_i64(32, -1));
    assert_eq!(t(-0.9), RefVec::from_i64(32, 0));
    assert_eq!(t(123.0), RefVec::from_i64(32, 123));
}

// 64 ビットを超える幅への正確な変換。2^100 は f64 で正確に表せる。
#[test]
fn s6_12_1_real_to_wide_int() {
    let (x, st) = from_f64_round(128, 2f64.powi(100));
    assert_eq!(st, Exact);
    assert_eq!(x, from_big(128, &pow2(100)));
    let (x, _) = from_f64_round(200, -(2f64.powi(100)));
    assert_eq!(
        x.to_bigint(true).unwrap(),
        -num_bigint::BigInt::from(pow2(100))
    );
}

// NaN / ±∞ → 0 で、状態を返す [要VCS確認 V-4]。
#[test]
fn real_to_int_not_finite_vcs_v4() {
    for x in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let (v, st) = from_f64_round(8, x);
        assert_eq!(st, RefRealToIntStatus::NotFinite);
        assert_eq!(v, RefVec::zeros(8));
        assert_eq!(from_f64_trunc(8, x).1, RefRealToIntStatus::NotFinite);
    }
}

// test-plan TC43 の表 (§6.12.1 の丸め、§20.5 の $rtoi、plan.md §12 Q-6 の状態)。
#[test]
fn tc43_s6_12_1_real_to_int_status_table() {
    check_round(32, 35.5, 36, Rounded);
    check_round(32, 35.4, 35, Rounded);
    check_round(32, 36.0, 36, Exact);
    // 0 から遠い方へ (偶数丸めではない)
    check_round(32, -1.5, -2, Rounded);
    check_round(32, -2.5, -3, Rounded);
    check_round(32, 2.5, 3, Rounded);
    check_round(32, 0.5, 1, Rounded);
    check_round(32, -0.5, -1, Rounded);
    // floor(x + 0.5) の落とし穴
    check_round(32, 0.499_999_999_999_999_94, 0, Rounded);
    check_round(32, 1.499_999_999_999_999_8, 1, Rounded);
    check_round(32, -0.0, 0, Exact);
    check_round(32, 5e-324, 0, Rounded);
    check_round(64, 4_503_599_627_370_495.5, 4_503_599_627_370_496, Rounded);
    // 符号付きの範囲 / 符号なしの範囲のどちらかに収まれば Exact
    check_round(8, -1.0, -1, Exact);
    check_round(8, 255.0, 255, Exact);
    check_round(8, -128.0, -128, Exact);
    check_round(8, 254.6, 255, Rounded);
    // 丸めた後に溢れたら Wrapped (Rounded より優先。Q-6)
    check_round(8, 255.5, 0, Wrapped);
    check_round(8, 300.0, 44, Wrapped);
    check_round(8, -129.0, 0x7f, Wrapped);
    check_round(64, 1e20, 7_766_279_631_452_241_920, Wrapped);
    check_round(128, 1e20, 100_000_000_000_000_000_000, Exact);
    let (x, st) = from_f64_round(128, 2f64.powi(100));
    assert_eq!((x, st), (from_big(128, &pow2(100)), Exact));
    let (x, st) = from_f64_round(128, -(2f64.powi(100)));
    assert_eq!((x, st), (from_big(128, &(pow2(128) - pow2(100))), Exact));
    // 1e300 の正確な整数値 (997 ビット)
    let (x, st) = from_f64_round(1100, 1e300);
    assert_eq!(st, Exact);
    let n = x.to_biguint().unwrap();
    assert_eq!(n.bits(), 997);
    assert_eq!(to_f64(&x, false), 1e300);
    for nf in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        check_round(32, nf, 0, NotFinite);
    }
    // $rtoi: 小数部を捨てたものは Rounded (STATUS の決定)
    check_trunc(32, 35.9, 35, Rounded);
    check_trunc(32, -35.9, -35, Rounded);
    check_trunc(32, -0.5, 0, Rounded);
    check_trunc(32, 7.0, 7, Exact);
    check_trunc(8, 256.7, 0, Wrapped);
    check_trunc(32, f64::NAN, 0, NotFinite);
}

// Q-6 の範囲の境界: 幅 1。範囲は −1 ≤ n ≤ 1。
#[test]
fn q6_real_to_int_status_width1() {
    check_round(1, 0.0, 0, Exact);
    check_round(1, 1.0, 1, Exact);
    check_round(1, -1.0, -1, Exact);
    check_round(1, 0.5, 1, Rounded);
    check_round(1, -0.5, -1, Rounded);
    check_round(1, 2.0, 0, Wrapped);
    check_round(1, -2.0, 0, Wrapped);
    check_round(1, 1.5, 0, Wrapped);
    check_round(1, -1.5, 0, Wrapped);
    check_trunc(1, 1.9, 1, Rounded);
    check_trunc(1, -1.9, -1, Rounded);
    check_trunc(1, 2.5, 0, Wrapped);
    check_trunc(1, -2.5, 0, Wrapped);
}

// Q-6 の範囲の境界: 幅 64。範囲は −2^63 ≤ n ≤ 2^64 − 1。
#[test]
fn q6_real_to_int_status_width64() {
    let p64 = 2f64.powi(64);
    let p63 = 2f64.powi(63);
    // 2^64 より小さい最大の f64 は 2^64 − 2048
    check_round(64, p64 - 2048.0, (1i128 << 64) - 2048, Exact);
    check_round(64, p64, 0, Wrapped);
    check_trunc(64, p64, 0, Wrapped);
    check_round(64, -p63, -(1i128 << 63), Exact);
    // −2^63 より小さい最大の f64 は −2^63 − 2048。切り詰めると 2^63 − 2048
    check_round(64, -p63 - 2048.0, (1i128 << 63) - 2048, Wrapped);
    check_round(64, -0.75, -1, Rounded);
    check_trunc(64, 1e19, 10_000_000_000_000_000_000, Exact);
}

// Q-6 の範囲の境界: 幅 65 以上 (多倍長)。幅 65 の範囲は −2^64 ≤ n ≤ 2^65 − 1。
#[test]
fn q6_real_to_int_status_width65_and_wider() {
    let p64 = 2f64.powi(64);
    let p65 = 2f64.powi(65);
    check_round(65, p64, 1i128 << 64, Exact);
    check_round(65, p65 - 4096.0, (1i128 << 65) - 4096, Exact);
    check_round(65, p65, 0, Wrapped);
    check_round(65, -p64, -(1i128 << 64), Exact);
    // −2^64 − 4096 → 2^65 − 2^64 − 4096 = 2^64 − 4096
    check_round(65, -p64 - 4096.0, (1i128 << 64) - 4096, Wrapped);
    // 幅 128: 2^127 は符号なしの範囲、−2^127 は符号付きの範囲
    let p127 = 2f64.powi(127);
    let p128 = 2f64.powi(128);
    let (x, st) = from_f64_round(128, p127);
    assert_eq!((x, st), (from_big(128, &pow2(127)), Exact));
    let (x, st) = from_f64_round(128, -p127);
    assert_eq!((x, st), (from_big(128, &pow2(127)), Exact));
    let (x, st) = from_f64_round(128, p128);
    assert_eq!((x, st), (RefVec::zeros(128), Wrapped));
    let (x, st) = from_f64_trunc(129, p128 + 2f64.powi(76));
    assert_eq!((x, st), (from_big(129, &(pow2(128) + pow2(76))), Exact));
    // 小さい値の丸めは幅によらない
    check_round(200, -2.5, -3, Rounded);
    check_trunc(200, -2.5, -2, Rounded);
}

// 幅 0 の dst: 0 だけが収まる (Q-6 の延長。plan に記載なし)。
#[test]
fn q6_real_to_int_status_width0() {
    assert_eq!(from_f64_round(0, 0.0), (RefVec::zeros(0), Exact));
    assert_eq!(from_f64_round(0, 0.4), (RefVec::zeros(0), Rounded));
    assert_eq!(from_f64_round(0, 1.0), (RefVec::zeros(0), Wrapped));
    assert_eq!(from_f64_trunc(0, -0.9), (RefVec::zeros(0), Rounded));
}

// §6.12.1, §20.5: 整数 → 実数。最近接偶数丸めで 1 回だけ丸める。
#[test]
fn s6_12_1_int_to_real() {
    assert_eq!(to_f64(&v("1111_1111"), true), -1.0);
    assert_eq!(to_f64(&v("1111_1111"), false), 255.0);
    assert_eq!(to_f64(&v("1000_0000"), true), -128.0);
    assert_eq!(to_f64(&RefVec::zeros(10), true), 0.0);
    // x/z は 0 として扱う [要VCS確認 V-14]: 8'b1x01 → 4'b1001 = 9
    assert_eq!(to_f64(&v("0000_1x01"), false), 9.0);
    // 2^53 + 1 → 2^53 (偶数へ)、2^53 + 3 → 2^53 + 4
    let w = 80;
    let p53 = pow2(53);
    assert_eq!(to_f64(&from_big(w, &(&p53 + 1u32)), false), 2f64.powi(53));
    assert_eq!(
        to_f64(&from_big(w, &(&p53 + 3u32)), false),
        2f64.powi(53) + 4.0
    );
    // 2^1024 - 1 は丸めで 2^1024 になり ∞、2^1024 - 2^971 は f64 の最大値
    let w = 1100;
    let p1024 = pow2(1024);
    assert_eq!(to_f64(&from_big(w, &(&p1024 - 1u32)), false), f64::INFINITY);
    assert_eq!(to_f64(&from_big(w, &(&p1024 - pow2(971))), false), f64::MAX);
    let minus = RefVec::from_bigint(w, &-num_bigint::BigInt::from(&p1024 - 1u32));
    assert_eq!(to_f64(&minus, true), f64::NEG_INFINITY);
    // 2^1024 - 2^970 は f64::MAX と 2^1024 のちょうど中間で、偶数丸めにより ∞
    assert_eq!(
        to_f64(&from_big(w, &(&p1024 - pow2(970))), false),
        f64::INFINITY
    );
}

// R-12: shortreal も整数から直接 1 回だけ丸める (f64 を経由すると二重丸めになる例)。
#[test]
fn s6_12_1_int_to_shortreal_single_rounding() {
    // m = 2^60 + 2^36 + 1。
    // 直接 24 ビットに丸めると、捨てる部分が半分 (2^36) より大きいので切り上げ: 2^60 + 2^37。
    // f64 を経由すると 2^60 + 2^36 になり、次に f32 でちょうど半分の偶数丸めで 2^60 になる。
    let m = pow2(60) + pow2(36) + 1u32;
    let x = from_big(64, &m);
    let expected = 2f32.powi(60) + 2f32.powi(37);
    assert_eq!(to_f32(&x, false), expected);
    assert_ne!(to_f64(&x, false) as f32, expected); // 二重丸めの結果とは違う
    assert_eq!(to_f32(&v("1111_1111"), true), -1.0f32);
    assert_eq!(to_f32(&from_big(200, &pow2(128)), false), f32::INFINITY);
    assert_eq!(
        to_f32(&RefVec::from_u64(32, (1 << 24) + 1), false),
        16_777_216.0
    );
}

fn f(x: &RefVec, radix: RefFmtRadix, signed: bool, width: RefFieldWidth) -> String {
    format_int(x, radix, signed, width)
}

use RefFieldWidth::{Auto, Min};
use RefFmtRadix::{Bin, Dec, Hex, Oct};

// plan.md §3.9 の例 (§21.2.1)。
#[test]
fn s21_2_1_format_examples() {
    assert_eq!(f(&v("10xz"), Bin, false, Auto), "10xz");
    assert_eq!(f(&v("xxxx_01z0"), Hex, false, Auto), "xZ");
    assert_eq!(f(&v("zzzz_xxxx"), Hex, false, Auto), "zx");
    assert_eq!(f(&v("0x01_0000"), Hex, false, Auto), "X0");
    assert_eq!(f(&RefVec::all_x(8), Dec, false, Auto), "  x");
    assert_eq!(f(&v("1111_1111"), Dec, true, Auto), "  -1");
    assert_eq!(f(&RefVec::from_u64(32, 1), Dec, true, Auto), "          1");
    let zero = Min {
        width: 0,
        zero_pad: false,
    };
    assert_eq!(f(&RefVec::from_i64(32, -5), Dec, true, zero), "-5");
}

// §21.2.1: x と z が混在する桁は X [要VCS確認 V-13]。最上位の端数桁。
#[test]
fn s21_2_1_mixed_digits_vcs_v13() {
    assert_eq!(f(&v("xz01"), Hex, false, Auto), "X");
    assert_eq!(f(&v("z_z0z1"), Hex, false, Auto), "zZ");
    assert_eq!(f(&v("1_1111"), Hex, false, Auto), "1f");
    assert_eq!(f(&v("10_101"), Oct, false, Auto), "25");
    assert_eq!(f(&v("x_000"), Oct, false, Auto), "x0");
    assert_eq!(f(&v("zxz"), Oct, false, Auto), "X");
    assert_eq!(f(&v("z1z"), Oct, false, Auto), "Z");
    assert_eq!(f(&v("1z"), Dec, false, Auto), "Z");
    assert_eq!(f(&v("xz"), Dec, false, Auto), "X");
    assert_eq!(f(&v("zz"), Dec, false, Auto), "z");
}

// plan.md §3.9 規則 3: 自動幅。
#[test]
fn s21_2_1_auto_width() {
    assert_eq!(f(&v("0000_0101"), Hex, false, Auto), "05");
    assert_eq!(f(&v("0_0101"), Bin, false, Auto), "00101");
    assert_eq!(f(&v("0000_0101"), Dec, false, Auto), "  5");
    assert_eq!(f(&v("0000_0101"), Dec, true, Auto), "   5");
    assert_eq!(f(&v("1"), Dec, true, Auto), "-1");
    assert_eq!(f(&v("1"), Dec, false, Auto), "1");
    assert_eq!(
        f(&RefVec::from_u64(64, u64::MAX), Dec, false, Auto),
        "18446744073709551615"
    );
    assert_eq!(
        f(&RefVec::from_u64(64, 0), Dec, true, Auto),
        format!("{}0", " ".repeat(19))
    );
}

// plan.md §3.9 規則 4, 5: %0d 等と %5d 等 [要VCS確認 V-7]。
#[test]
fn s21_2_1_min_width_vcs_v7() {
    let m = |width, zero_pad| Min { width, zero_pad };
    assert_eq!(f(&v("0000_0101"), Hex, false, m(0, false)), "5");
    assert_eq!(f(&v("0000_0000"), Hex, false, m(0, false)), "0");
    assert_eq!(f(&v("000x_0101"), Hex, false, m(0, false)), "X5");
    assert_eq!(f(&v("0000_z101"), Bin, false, m(0, false)), "z101");
    assert_eq!(f(&v("0000_0101"), Dec, false, m(5, false)), "    5");
    assert_eq!(f(&v("0000_0101"), Dec, false, m(5, true)), "00005");
    assert_eq!(f(&v("0000_0101"), Hex, false, m(5, false)), "00005");
    assert_eq!(f(&v("1111_1011"), Dec, true, m(5, false)), "   -5");
    assert_eq!(f(&v("1111_1011"), Dec, true, m(1, false)), "-5"); // 切り詰めない
    assert_eq!(f(&RefVec::all_x(8), Dec, false, m(0, false)), "x");
    assert_eq!(f(&RefVec::all_x(8), Dec, false, m(4, false)), "   x");
    // 暫定: 負の値の 0 埋めは符号の後ろを埋める
    assert_eq!(f(&v("1111_1011"), Dec, true, m(5, true)), "-0005");
}

// §6.16.11〜§6.16.14: itoa 系は %0d %0h %0o %0b と同じ。
#[test]
fn s6_16_11_itoa() {
    assert_eq!(itoa(&RefVec::from_i64(32, -42), true, Dec), b"-42");
    assert_eq!(itoa(&RefVec::from_u64(32, 255), false, Hex), b"ff");
    assert_eq!(itoa(&RefVec::from_u64(32, 8), false, Oct), b"10");
    assert_eq!(itoa(&RefVec::from_u64(32, 5), false, Bin), b"101");
}

// 多倍長の 10 進表示。
#[test]
fn wide_decimal() {
    let x = from_big(130, &pow2(128));
    assert_eq!(
        f(
            &x,
            Dec,
            false,
            Min {
                width: 0,
                zero_pad: false
            }
        ),
        "340282366920938463463374607431768211456"
    );
}
