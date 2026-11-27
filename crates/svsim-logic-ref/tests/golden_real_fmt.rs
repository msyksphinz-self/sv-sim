//! 参照モデルのゴールデンテスト: 実数との変換 (IEEE 1800-2023 §6.12.1, §20.5) と
//! 表示用フォーマット (§21.2.1、plan.md §3.9)。

use num_bigint::BigUint;
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

// §6.12.1: 実数 → 整数は最も近い整数へ、ちょうど半分は 0 から遠い方へ。
#[test]
fn s6_12_1_real_to_int_rounding() {
    let r = |x: f64| {
        let (v, st) = from_f64_round(32, x);
        assert_eq!(st, RefRealToIntStatus::Ok);
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
    assert_eq!(st, RefRealToIntStatus::Ok);
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
