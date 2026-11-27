//! リテラル解析・文字列変換・atoi 系のゴールデンテスト (規格の例と境界を固定値で書く。plan.md §5.4、
//! test-plan TC37, TC38, TC40, TC41)。
//!
//! テスト名とコメントには IEEE 1800-2023 の条番号を書く。`[要VCS確認 V-n]` の印は、暫定の挙動
//! (plan.md §11) を固定しているテストに付ける。

use svsim_logic::literal::{Literal, LiteralError, Radix, parse_based, parse_unsized_decimal};
use svsim_logic::string::{
    atoi, atoi_u32, from_string_bytes, from_string_bytes2, to_string_bytes, to_string_bytes2,
};
use svsim_logic::{BitVec, Logic, LogicVec};

const U: bool = false;
const S: bool = true;

fn lv(s: &str) -> LogicVec {
    LogicVec::from_bits_str(s).unwrap_or_else(|e| panic!("{s:?}: {e}"))
}

fn ok(size: Option<u32>, signed: bool, radix: Radix, digits: &str) -> Literal {
    let l = parse_based(size, signed, radix, digits)
        .unwrap_or_else(|e| panic!("parse_based({size:?}, {signed}, {radix:?}, {digits:?}): {e}"));
    assert!(l.value.as_ref().is_normalized(), "not normalized: {l:?}");
    assert_eq!(l.signed, signed);
    assert_eq!(l.sized, size.is_some());
    if let Some(s) = size {
        assert_eq!(l.value.width(), s);
    }
    l
}

/// サイズ付きで、値 (bits 文字列) と `truncated` を確かめる。
fn sized(size: u32, radix: Radix, digits: &str, bits: &str, truncated: bool) {
    let l = ok(Some(size), U, radix, digits);
    assert_eq!(
        l.value.to_bits_string(),
        bits,
        "{size}'{radix:?} {digits:?}"
    );
    assert_eq!(
        l.truncated, truncated,
        "{size}'{radix:?} {digits:?}: truncated"
    );
    assert!(!l.xz_extend && !l.widened_unsized);
}

fn err(size: Option<u32>, radix: Radix, digits: &str) -> LiteralError {
    match parse_based(size, U, radix, digits) {
        Ok(l) => panic!("parse_based({size:?}, {radix:?}, {digits:?}) = Ok({l:?})"),
        Err(e) => e,
    }
}

// ===========================================================================
// §5.7.1 基数付きリテラル (2/8/16 進) (TC37)
// ===========================================================================

#[test]
fn s5_7_1_bin_left_fill_zero_or_xz() {
    // 最左が 1 なので 0 で埋める。
    sized(8, Radix::Bin, "1x", "0000001x", false);
    // 最左が x/z ならその値で埋める。
    sized(8, Radix::Bin, "x1", "xxxxxxx1", false);
    sized(8, Radix::Bin, "z", "zzzzzzzz", false);
    sized(8, Radix::Hex, "x", "xxxxxxxx", false);
    sized(12, Radix::Hex, "x1", "xxxxxxxx0001", false);
}

#[test]
fn s5_7_1_question_mark_is_z() {
    sized(8, Radix::Hex, "?", "zzzzzzzz", false);
    sized(2, Radix::Bin, "??", "zz", false);
}

#[test]
fn s5_7_1_case_insensitive_digits() {
    sized(8, Radix::Hex, "XZ", "xxxxzzzz", false);
    sized(8, Radix::Hex, "xz", "xxxxzzzz", false);
    sized(8, Radix::Hex, "AB", "10101011", false);
    sized(8, Radix::Hex, "ab", "10101011", false);
}

#[test]
fn s5_7_1_underscore_ignored() {
    sized(8, Radix::Bin, "1_0_1", "00000101", false);
    sized(8, Radix::Bin, "1__0_", "00000010", false);
}

#[test]
fn s5_7_1_truncate_left_q3() {
    // 1 が落ちる → truncated。
    sized(4, Radix::Bin, "1_0101", "0101", true);
    sized(4, Radix::Hex, "fff", "1111", true);
    sized(4, Radix::Hex, "ff", "1111", true);
    sized(3, Radix::Oct, "7", "111", false);
    // 落ちるのが 0 だけなら立てない (決定 Q-3) [要VCS確認 V-18]
    sized(4, Radix::Oct, "17", "1111", false);
    sized(8, Radix::Hex, "0ff", "11111111", false);
    sized(4, Radix::Bin, "0000_1111", "1111", false);
    // x/z が落ちれば立てる [要VCS確認 V-18]
    sized(4, Radix::Hex, "xx", "xxxx", true);
    sized(4, Radix::Hex, "z0", "0000", true);
    sized(4, Radix::Hex, "1f", "1111", true);
    sized(4, Radix::Oct, "37", "1111", true);
    sized(4, Radix::Hex, "xF", "1111", true);
}

#[test]
fn s5_7_1_wide_sized_hex() {
    let l = ok(Some(65), U, Radix::Hex, "1_0000_0000_0000_0000");
    let mut want = LogicVec::zero(65);
    want.set_bit(64, Logic::L1);
    assert_eq!(l.value, want);
    assert!(!l.truncated);
    // 8 進の桁がワード境界をまたぐ (ビット 63..65)。
    let l = ok(Some(70), U, Radix::Oct, "5_000000000000000000000");
    assert_eq!(l.value.bit(65), Logic::L1);
    assert_eq!(l.value.bit(64), Logic::L0);
    assert_eq!(l.value.bit(63), Logic::L1);
    // x の最左で 64 ビットを超えて埋める。
    let l = ok(Some(200), U, Radix::Hex, "z_f");
    assert_eq!(l.value.bit(199), Logic::Z);
    assert_eq!(l.value.bit(4), Logic::Z);
    assert_eq!(l.value.bit(3), Logic::L1);
}

#[test]
fn s5_7_1_signed_sized_fills_zero() {
    // リテラルの段階では 0 で埋める。符号拡張は文脈で行う (R-11)。
    let l = ok(Some(4), S, Radix::Bin, "1");
    assert_eq!(l.value, lv("0001"));
    assert!(l.signed);
    let l = ok(Some(4), S, Radix::Bin, "1111");
    assert_eq!(l.value, lv("1111"));
    assert!(l.signed);
}

#[test]
fn s5_7_1_unsized_based() {
    let l = ok(None, U, Radix::Hex, "x");
    assert_eq!(l.value, LogicVec::filled(32, Logic::X));
    assert!(l.xz_extend && !l.sized && !l.widened_unsized);
    let l = ok(None, U, Radix::Bin, "z1");
    assert_eq!(l.value.width(), 32);
    assert_eq!(l.value.bit(31), Logic::Z);
    assert_eq!(l.value.bit(0), Logic::L1);
    assert!(l.xz_extend);
    let l = ok(None, U, Radix::Bin, "1");
    assert_eq!(l.value, LogicVec::from_u64(32, 1));
    assert!(!l.xz_extend);
    // 先頭の 0 は幅に数えない。
    let l = ok(None, U, Radix::Hex, "0000_0000_ffff_ffff");
    assert_eq!(l.value.width(), 32);
    assert!(!l.widened_unsized);
}

#[test]
fn s5_7_1_unsized_wider_than_32_v11() {
    // [要VCS確認 V-11] 必要な幅まで広げて警告フラグ。
    let l = ok(None, U, Radix::Hex, "1_0000_0000");
    let mut want = LogicVec::zero(33);
    want.set_bit(32, Logic::L1);
    assert_eq!(l.value, want);
    assert!(l.widened_unsized && !l.truncated);
    // x/z も 0 でないビットとして幅に数える。
    let l = ok(None, U, Radix::Hex, "x_0000_0000");
    assert_eq!(l.value.width(), 36);
    assert!(l.widened_unsized && l.xz_extend);
    let l = ok(None, U, Radix::Hex, "z_0000_0000");
    assert_eq!(l.value.width(), 36);
}

#[test]
fn s5_7_1_based_errors() {
    assert_eq!(err(Some(0), Radix::Hex, "1"), LiteralError::ZeroSize);
    assert_eq!(err(Some(0), Radix::Dec, "1"), LiteralError::ZeroSize);
    for (r, d) in [(Radix::Bin, "102"), (Radix::Oct, "8"), (Radix::Hex, "g")] {
        assert!(
            matches!(err(Some(8), r, d), LiteralError::InvalidDigit { radix, .. } if radix == r),
            "{r:?} {d:?}"
        );
    }
    assert_eq!(err(Some(8), Radix::Hex, ""), LiteralError::NoDigits);
    // 先頭の `_` は文法どおりエラー (マネージャ決定)。
    assert_eq!(
        err(Some(8), Radix::Hex, "_f"),
        LiteralError::LeadingUnderscore
    );
    assert_eq!(
        err(Some(8), Radix::Bin, "_"),
        LiteralError::LeadingUnderscore
    );
    assert_eq!(err(None, Radix::Dec, "_1"), LiteralError::LeadingUnderscore);
    // 非 ASCII の文字もエラー (panic しない)。
    assert!(matches!(
        err(Some(8), Radix::Hex, "fé"),
        LiteralError::InvalidDigit { ch: 'é', .. }
    ));
}

// ===========================================================================
// §5.7.1 10 進リテラル・サイズなし整数 (TC38)
// ===========================================================================

#[test]
fn s5_7_1_decimal_value_truncation() {
    let l = ok(Some(4), U, Radix::Dec, "20");
    assert_eq!(l.value.to_u64(), Some(4));
    assert!(l.truncated);
    let l = ok(Some(8), U, Radix::Dec, "0255");
    assert_eq!(l.value.to_u64(), Some(255));
    assert!(!l.truncated);
    let l = ok(Some(8), U, Radix::Dec, "256");
    assert_eq!(l.value.to_u64(), Some(0));
    assert!(l.truncated);
    let l = ok(Some(8), U, Radix::Dec, "1_000");
    assert_eq!(l.value.to_u64(), Some(232));
    assert!(l.truncated);
}

#[test]
fn s5_7_1_decimal_xz_single_digit_r11() {
    for (d, v) in [
        ("x", Logic::X),
        ("x_", Logic::X),
        ("?", Logic::Z),
        ("z", Logic::Z),
        ("X", Logic::X),
        ("Z__", Logic::Z),
    ] {
        let l = ok(Some(8), U, Radix::Dec, d);
        assert_eq!(l.value, LogicVec::filled(8, v), "{d:?}");
        assert!(!l.truncated && !l.xz_extend);
    }
    // サイズなしの 'dx は幅 32 の全 x で、文脈幅へ拡張する。
    let l = ok(None, U, Radix::Dec, "x");
    assert_eq!(l.value, LogicVec::filled(32, Logic::X));
    assert!(l.xz_extend);
    // x/z は 1 桁だけ。
    assert!(matches!(
        err(Some(8), Radix::Dec, "1x"),
        LiteralError::DecimalUnknownNotAlone { .. }
    ));
    assert!(matches!(
        err(Some(8), Radix::Dec, "xx"),
        LiteralError::DecimalUnknownNotAlone { .. }
    ));
    assert!(matches!(
        err(Some(8), Radix::Dec, "x1"),
        LiteralError::DecimalUnknownNotAlone { .. }
    ));
    assert!(matches!(
        err(Some(8), Radix::Dec, "1a"),
        LiteralError::InvalidDigit { .. }
    ));
}

#[test]
fn s5_7_1_decimal_wide() {
    let l = ok(Some(64), U, Radix::Dec, "18446744073709551615");
    assert_eq!(l.value, LogicVec::filled(64, Logic::L1));
    assert!(!l.truncated);
    let l = ok(Some(65), U, Radix::Dec, "36893488147419103231");
    assert_eq!(l.value, LogicVec::filled(65, Logic::L1));
    assert!(!l.truncated);
    let l = ok(Some(64), U, Radix::Dec, "18446744073709551616");
    assert_eq!(l.value, LogicVec::zero(64));
    assert!(l.truncated);
    let l = ok(
        Some(128),
        U,
        Radix::Dec,
        "340282366920938463463374607431768211455",
    );
    assert_eq!(l.value, LogicVec::filled(128, Logic::L1));
    assert!(!l.truncated);
    let l = ok(
        Some(128),
        U,
        Radix::Dec,
        "340282366920938463463374607431768211456",
    );
    assert_eq!(l.value, LogicVec::zero(128));
    assert!(l.truncated);
}

#[test]
fn s5_7_1_unsized_decimal() {
    let l = parse_unsized_decimal("123").unwrap();
    assert_eq!(l.value, LogicVec::from_u64(32, 123));
    assert!(l.signed && !l.sized && !l.widened_unsized && !l.xz_extend);
    let l = parse_unsized_decimal("0").unwrap();
    assert_eq!(l.value, LogicVec::zero(32));
    // 2^31: 32 ビットに収まるので幅 32 (符号付きとしては負。暫定) [要VCS確認 V-11]
    let l = parse_unsized_decimal("2147483648").unwrap();
    assert_eq!(l.value, LogicVec::from_u64(32, 0x8000_0000));
    assert!(l.signed && !l.widened_unsized);
    assert_eq!(l.value.to_i64(true), Some(-2_147_483_648));
    // 2^32: 幅 33 [要VCS確認 V-11]
    let l = parse_unsized_decimal("4294967296").unwrap();
    let mut want = LogicVec::zero(33);
    want.set_bit(32, Logic::L1);
    assert_eq!(l.value, want);
    assert!(l.widened_unsized);
    // 'd のサイズなしは符号の指定に従う。
    let l = ok(None, U, Radix::Dec, "4294967295");
    assert_eq!(l.value, LogicVec::filled(32, Logic::L1));
    assert!(!l.signed && !l.widened_unsized);
    let l = ok(
        None,
        S,
        Radix::Dec,
        "00000000000000000000000000000000000000000001",
    );
    assert_eq!(l.value, LogicVec::from_u64(32, 1));
    // エラー
    assert_eq!(
        parse_unsized_decimal("x"),
        Err(LiteralError::UnknownInUnbasedDecimal)
    );
    assert_eq!(parse_unsized_decimal(""), Err(LiteralError::NoDigits));
    assert_eq!(
        parse_unsized_decimal("_1"),
        Err(LiteralError::LeadingUnderscore)
    );
    assert!(matches!(
        parse_unsized_decimal("12a"),
        Err(LiteralError::InvalidDigit { index: 2, .. })
    ));
}

// ===========================================================================
// §5.9, §11.10.3, §6.16 文字列 ⇔ 整数 (TC40)
// ===========================================================================

fn from_str_hex(width: u32, s: &[u8]) -> u64 {
    let v = LogicVec::from_string_bytes(width, s);
    assert!(v.as_ref().is_normalized());
    let b = BitVec::from_string_bytes(width, s);
    assert_eq!(LogicVec::from(&b), v);
    v.to_u64().expect("known and ≤ 64 bits")
}

#[test]
fn s11_10_3_string_literal_to_integer() {
    assert_eq!(from_str_hex(16, b"AB"), 0x4142);
    assert_eq!(from_str_hex(24, b"AB"), 0x00_4142);
    assert_eq!(from_str_hex(8, b"AB"), 0x42);
    assert_eq!(from_str_hex(20, b"AB"), 0x0_4142);
    assert_eq!(from_str_hex(12, b"AB"), 0x142);
    // 空の文字列は "\0" として扱う (§11.10.3)。
    assert_eq!(from_str_hex(8, b""), 0);
    assert_eq!(from_str_hex(16, b"\x00\xff"), 0x00ff);
    // §11.10.3 の例: "Hello world" を 8*14 ビットに入れると左が 0。
    let v = LogicVec::from_string_bytes(8 * 14, b"Hello world");
    assert_eq!(v.to_string_bytes(), b"Hello world");
    assert_eq!(v.bit(8 * 11), Logic::L0);
}

#[test]
fn s11_10_3_from_string_overwrites_dirty_dst() {
    let mut dst = LogicVec::filled(100, Logic::X);
    from_string_bytes(dst.as_mut(), b"A");
    assert_eq!(dst, LogicVec::from_string_bytes(100, b"A"));
    assert!(!dst.has_unknown());
    let mut dst2 = BitVec::filled(70, true);
    from_string_bytes2(dst2.as_mut(), b"");
    assert_eq!(dst2, BitVec::zero(70));
}

#[test]
fn s6_16_integer_to_string() {
    let h = |w: u32, v: u64| LogicVec::from_u64(w, v);
    assert_eq!(h(16, 0x4142).to_string_bytes(), b"AB");
    assert_eq!(h(12, 0x041).to_string_bytes(), b"A");
    assert_eq!(h(24, 0x41_0042).to_string_bytes(), b"AB");
    assert_eq!(h(8, 0).to_string_bytes(), b"");
    assert_eq!(
        to_string_bytes2(BitVec::from_u64(16, 0x4142).as_ref()),
        b"AB"
    );
}

#[test]
fn s6_16_integer_to_string_xz_as_zero_v5() {
    // [要VCS確認 V-5] X/Z は 0 として扱う。
    assert_eq!(to_string_bytes(lv("01000001xxxxxxxx").as_ref()), b"A");
    assert_eq!(to_string_bytes(lv("01000001010000z1").as_ref()), b"AA");
}

// ===========================================================================
// §6.16.9 atoi / atohex / atooct / atobin (TC41)
// ===========================================================================

#[test]
fn s6_16_9_atoi() {
    for (s, v) in [
        ("123", 123u32),
        ("12_3", 123),
        ("12a3", 12),
        ("", 0),
        ("abc", 0),
    ] {
        assert_eq!(atoi_u32(s.as_bytes(), Radix::Dec), v, "{s:?}");
        assert_eq!(
            atoi(s.as_bytes(), Radix::Dec),
            BitVec::from_u64(32, v.into())
        );
    }
}

#[test]
fn s6_16_9_atoi_sign_space_v6() {
    // [要VCS確認 V-6] 符号と空白は解釈しない (Verilator は -12 / 5 / 12)。
    assert_eq!(atoi_u32(b"-12", Radix::Dec), 0);
    assert_eq!(atoi_u32(b"+5", Radix::Dec), 0);
    assert_eq!(atoi_u32(b" 12", Radix::Dec), 0);
}

#[test]
fn s6_16_9_atoi_underscore_q4_v6() {
    // [要VCS確認 V-6] 先頭の `_` も読み飛ばす (決定 Q-4)。
    assert_eq!(atoi_u32(b"_12", Radix::Dec), 12);
    assert_eq!(atoi_u32(b"_", Radix::Dec), 0);
    assert_eq!(atoi_u32(b"__", Radix::Dec), 0);
    assert_eq!(atoi_u32(b"1__2", Radix::Dec), 12);
}

#[test]
fn s6_16_9_atoi_truncate_32_v6() {
    // [要VCS確認 V-6] 32 ビットを超えた分は切り詰める。
    assert_eq!(atoi_u32(b"4294967295", Radix::Dec), 0xffff_ffff);
    assert_eq!(atoi_u32(b"4294967296", Radix::Dec), 0);
    assert_eq!(atoi_u32(b"4294967297", Radix::Dec), 1);
}

#[test]
fn s6_16_9_atohex_atooct_atobin() {
    assert_eq!(atoi_u32(b"ff", Radix::Hex), 255);
    assert_eq!(atoi_u32(b"FF", Radix::Hex), 255);
    assert_eq!(atoi_u32(b"1g", Radix::Hex), 1);
    // [要VCS確認 V-6] `0x` 接頭辞は解釈しない (`0` を読み、`x` で止まる)。
    assert_eq!(atoi_u32(b"0xff", Radix::Hex), 0);
    assert_eq!(atoi_u32(b"1_0000_0001", Radix::Hex), 1);
    assert_eq!(atoi_u32(b"17", Radix::Oct), 15);
    assert_eq!(atoi_u32(b"18", Radix::Oct), 1);
    assert_eq!(atoi_u32(b"1012", Radix::Bin), 5);
    assert_eq!(atoi_u32(b"2", Radix::Bin), 0);
    // 非 ASCII のバイトで止まる。
    assert_eq!(atoi_u32(b"7\xff1", Radix::Dec), 7);
}
