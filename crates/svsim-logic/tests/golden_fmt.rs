//! 表示用フォーマットと itoa 系のゴールデンテスト (plan.md §3.9、IEEE 1800-2023 §21.2.1.2〜
//! §21.2.1.4、§6.16.11〜§6.16.14、test-plan TC45〜TC48, TC53)。
//!
//! 期待値は test-plan の表をそのまま写したもの。`[要VCS確認 V-n]` の印は暫定の挙動 (plan.md §11)。

use svsim_logic::fmt::{self, FieldWidth, FmtRadix};
use svsim_logic::{BitVec, Logic, LogicVec, Scratch};

use FmtRadix::{Bin, Dec, Hex, Oct};

const AUTO: FieldWidth = FieldWidth::Auto;

fn min(width: u32, zero_pad: bool) -> FieldWidth {
    FieldWidth::Min { width, zero_pad }
}

/// bits 文字列 (MSB が先頭、`_` は読みやすさのために除く)。
fn lv(s: &str) -> LogicVec {
    LogicVec::from_bits_str(&s.replace('_', "")).unwrap()
}

fn u(w: u32, v: u64) -> LogicVec {
    LogicVec::from_u64(w, v)
}

fn s(w: u32, v: i64) -> LogicVec {
    LogicVec::from_i64(w, v)
}

fn bit(w: u32, k: u32) -> LogicVec {
    let mut x = LogicVec::zero(w);
    x.set_bit(k, Logic::L1);
    x
}

/// 4 値で書式化する。2 値で表せる値なら、2 値版 (`format_int2`) も同じ結果になることを確かめる。
fn f(x: &LogicVec, radix: FmtRadix, signed: bool, width: FieldWidth) -> String {
    let mut out = String::new();
    fmt::format_int(
        &mut out,
        x.as_ref(),
        radix,
        signed,
        width,
        &mut Scratch::new(),
    );
    if !x.has_unknown() {
        let b: BitVec = x.to_bit_vec();
        let mut out2 = String::new();
        fmt::format_int2(
            &mut out2,
            b.as_ref(),
            radix,
            signed,
            width,
            &mut Scratch::new(),
        );
        assert_eq!(out2, out, "format_int2 {x:?} {radix:?} {signed} {width:?}");
    }
    out
}

/// TC45: `%b %o %h` の桁の規則 (§21.2.1.2〜§21.2.1.4、plan.md §3.9 規則 1, 3)。
#[test]
fn tc45_bin_oct_hex_21_2_1() {
    assert_eq!(f(&lv("10xz"), Bin, false, AUTO), "10xz");
    assert_eq!(f(&u(8, 0x05), Bin, false, AUTO), "00000101");
    assert_eq!(f(&lv("xxxx_01z0"), Hex, false, AUTO), "xZ");
    assert_eq!(f(&lv("zzzz_xxxx"), Hex, false, AUTO), "zx");
    assert_eq!(f(&lv("0x01_0000"), Hex, false, AUTO), "X0");
    // x と z が混在 → X 優先 [要VCS確認 V-13]。
    assert_eq!(f(&lv("xz01"), Hex, false, AUTO), "X");
    // 最上位の端数桁は存在するビットだけで判定する。
    assert_eq!(f(&lv("x_0000"), Hex, false, AUTO), "x0");
    assert_eq!(f(&lv("z_1111"), Hex, false, AUTO), "zf");
    assert_eq!(f(&u(8, 0xab), Hex, false, AUTO), "ab");
    assert_eq!(f(&u(8, 0x0f), Hex, false, AUTO), "0f");
    assert_eq!(f(&u(8, 0x00), Hex, false, AUTO), "00");
    assert_eq!(f(&lv("xxxxxxxx"), Hex, false, AUTO), "xx");
    assert_eq!(f(&lv("z"), Hex, false, AUTO), "z");
    assert_eq!(f(&bit(65, 64), Hex, false, AUTO), "10000000000000000");
    assert_eq!(f(&u(8, 0o377), Oct, false, AUTO), "377");
    assert_eq!(f(&u(9, 0x1ff), Oct, false, AUTO), "777");
    assert_eq!(f(&lv("1x01"), Oct, false, AUTO), "1X");
    assert_eq!(f(&lv("z000"), Oct, false, AUTO), "z0");
    assert_eq!(f(&lv("xz1"), Oct, false, AUTO), "X"); // [要VCS確認 V-13]
    assert_eq!(
        f(&u(64, u64::MAX), Oct, false, AUTO),
        "1777777777777777777777"
    );
    assert_eq!(
        f(&LogicVec::filled(64, Logic::X), Oct, false, AUTO),
        "xxxxxxxxxxxxxxxxxxxxxx"
    );
    // 符号は 2/8/16 進の表示に影響しない。
    assert_eq!(f(&u(8, 0xff), Hex, true, AUTO), "ff");
}

/// TC45: `%b`/`%o`/`%h` の自動幅の文字数 (plan.md §3.9 規則 3)。
#[test]
fn tc45_auto_width_table() {
    let table: [(u32, usize, usize, usize); 12] = [
        (1, 1, 1, 1),
        (8, 8, 3, 2),
        (31, 31, 11, 8),
        (32, 32, 11, 8),
        (33, 33, 11, 9),
        (63, 63, 21, 16),
        (64, 64, 22, 16),
        (65, 65, 22, 17),
        (127, 127, 43, 32),
        (128, 128, 43, 32),
        (129, 129, 43, 33),
        (1 << 20, 1 << 20, 349526, 262144),
    ];
    for (w, b, o, h) in table {
        let z = LogicVec::zero(w);
        assert_eq!(f(&z, Bin, false, AUTO).len(), b, "%b w={w}");
        assert_eq!(f(&z, Oct, false, AUTO).len(), o, "%o w={w}");
        assert_eq!(f(&z, Hex, false, AUTO).len(), h, "%h w={w}");
        assert!(fmt::format_len_bound(w, Bin) >= b);
        assert!(fmt::format_len_bound(w, Oct) >= o);
        assert!(fmt::format_len_bound(w, Hex) >= h);
    }
}

/// TC46: `%d` と自動幅 (plan.md §3.9 規則 2, 3)。
#[test]
fn tc46_decimal_auto_21_2_1() {
    assert_eq!(f(&lv("xxxxxxxx"), Dec, false, AUTO), "  x");
    assert_eq!(f(&lv("0000_000x"), Dec, false, AUTO), "  X");
    assert_eq!(f(&lv("zzzz_zzzz"), Dec, false, AUTO), "  z");
    assert_eq!(f(&lv("0000_000z"), Dec, false, AUTO), "  Z");
    assert_eq!(f(&lv("z000_000x"), Dec, false, AUTO), "  X"); // X 優先
    assert_eq!(f(&u(8, 0xff), Dec, false, AUTO), "255");
    assert_eq!(f(&u(8, 0xff), Dec, true, AUTO), "  -1");
    assert_eq!(f(&u(8, 0x80), Dec, true, AUTO), "-128");
    assert_eq!(f(&u(8, 0x00), Dec, true, AUTO), "   0");
    assert_eq!(f(&u(32, 1), Dec, true, AUTO), "          1");
    assert_eq!(f(&u(32, 1), Dec, false, AUTO), "         1");
    assert_eq!(f(&u(1, 1), Dec, false, AUTO), "1");
    assert_eq!(f(&u(1, 1), Dec, true, AUTO), "-1");
    assert_eq!(f(&u(1, 0), Dec, true, AUTO), " 0");
    assert_eq!(f(&u(4, 0b1000), Dec, true, AUTO), "-8");
    assert_eq!(
        f(&u(64, u64::MAX), Dec, false, AUTO),
        "18446744073709551615"
    );
    assert_eq!(f(&bit(64, 63), Dec, true, AUTO), "-9223372036854775808");
    assert_eq!(
        f(&LogicVec::filled(128, Logic::L1), Dec, false, AUTO),
        "340282366920938463463374607431768211455"
    );
    assert_eq!(
        f(&bit(128, 127), Dec, true, AUTO),
        "-170141183460469231731687303715884105728"
    );
    let all_x = f(&LogicVec::filled(129, Logic::X), Dec, true, AUTO);
    assert_eq!(all_x, format!("{}x", " ".repeat(39)));
}

/// TC46: `%d` の自動幅の文字数表 (U / S)。
#[test]
fn tc46_decimal_auto_width_table() {
    let table: [(u32, usize, usize); 15] = [
        (1, 1, 2),
        (2, 1, 2),
        (3, 1, 2),
        (4, 2, 2),
        (8, 3, 4),
        (16, 5, 6),
        (31, 10, 11),
        (32, 10, 11),
        (33, 10, 11),
        (63, 19, 20),
        (64, 20, 20),
        (65, 20, 21),
        (127, 39, 39),
        (128, 39, 40),
        (129, 39, 40),
    ];
    for (w, un, sg) in table {
        let z = LogicVec::zero(w);
        assert_eq!(f(&z, Dec, false, AUTO).len(), un, "%d U w={w}");
        assert_eq!(f(&z, Dec, true, AUTO).len(), sg, "%d S w={w}");
        assert!(fmt::format_len_bound(w, Dec) >= un.max(sg));
    }
}

/// TC47: フィールド幅の指定 (plan.md §3.9 規則 4, 5、Q-7, Q-8) [要VCS確認 V-7]。
#[test]
fn tc47_field_width_v7() {
    let m0 = min(0, false);
    assert_eq!(f(&s(32, -5), Dec, true, m0), "-5");
    assert_eq!(f(&u(32, 0), Dec, false, m0), "0");
    assert_eq!(f(&u(32, 0), Dec, true, m0), "0");
    assert_eq!(f(&lv("xxxxxxxx"), Dec, false, m0), "x");
    assert_eq!(f(&lv("0000_000z"), Dec, false, m0), "Z");
    assert_eq!(f(&u(16, 0x00ab), Hex, false, m0), "ab");
    assert_eq!(f(&u(8, 0x00), Hex, false, m0), "0");
    // x の桁は 0 ではないので、そこで止まる。
    assert_eq!(f(&lv("xxxx_0000_0001"), Hex, false, m0), "x01");
    assert_eq!(f(&lv("0000_xxxx_0001"), Hex, false, m0), "x1");
    assert_eq!(f(&u(8, 0x05), Bin, false, m0), "101");
    assert_eq!(f(&u(8, 0), Oct, false, m0), "0");
    assert_eq!(f(&u(32, 42), Dec, false, min(5, false)), "   42");
    assert_eq!(f(&s(32, -5), Dec, true, min(5, false)), "   -5");
    assert_eq!(f(&u(32, 42), Dec, false, min(5, true)), "00042");
    // 負数の 0 埋めは符号を先頭に置く (Q-7)。
    assert_eq!(f(&s(32, -5), Dec, true, min(5, true)), "-0005");
    assert_eq!(f(&lv("xxxxxxxx"), Dec, false, min(5, false)), "    x");
    assert_eq!(f(&lv("xxxxxxxx"), Dec, false, min(5, true)), "0000x"); // Q-7
    for zp in [false, true] {
        // %h は常に 0 で埋める。
        assert_eq!(f(&u(8, 0xab), Hex, false, min(5, zp)), "000ab");
        assert_eq!(f(&lv("xxxxxxxx"), Hex, false, min(5, zp)), "000xx"); // Q-7
        assert_eq!(f(&u(16, 0x1234), Hex, false, min(3, zp)), "1234");
        // 最小表示を元に N 文字まで埋め、自動幅には戻さない (Q-8)。
        assert_eq!(f(&u(8, 0x0a), Hex, false, min(1, zp)), "a");
        assert_eq!(f(&u(16, 0x00ab), Hex, false, min(2, zp)), "ab");
        assert_eq!(f(&u(8, 0x05), Bin, false, min(4, zp)), "0101");
    }
    // 切り詰めない。
    assert_eq!(f(&u(32, 12345), Dec, false, min(2, false)), "12345");
    assert_eq!(f(&u(8, 5), Dec, false, min(1, false)), "5"); // Q-8
    assert_eq!(f(&u(8, 5), Dec, false, min(3, false)), "  5");
}

/// TC48: `itoa` `hextoa` `octtoa` `bintoa` (§6.16.11〜§6.16.14)。
#[test]
fn tc48_itoa_6_16_11() {
    let mut sc = Scratch::new();
    let it = |x: &LogicVec, signed, r, sc: &mut Scratch| fmt::itoa(x.as_ref(), signed, r, sc);
    assert_eq!(it(&s(32, -5), true, Dec, &mut sc), b"-5");
    for signed in [false, true] {
        for r in [Dec, Hex, Oct, Bin] {
            assert_eq!(it(&u(32, 0), signed, r, &mut sc), b"0");
        }
    }
    assert_eq!(it(&u(32, 255), false, Hex, &mut sc), b"ff");
    assert_eq!(it(&u(32, 8), false, Oct, &mut sc), b"10");
    assert_eq!(it(&u(32, 5), false, Bin, &mut sc), b"101");
    assert_eq!(it(&u(32, 0xffff_ffff), true, Dec, &mut sc), b"-1");
    assert_eq!(it(&u(32, 0xffff_ffff), false, Dec, &mut sc), b"4294967295");
    let b = BitVec::from_u64(32, 255);
    assert_eq!(fmt::itoa2(b.as_ref(), false, Hex, &mut sc), b"ff");
}

/// TC53: 上限近くの幅 (2^20) の `%h` (R-27)。定期ジョブでだけ実行する。
#[test]
#[ignore]
fn tc53_hex_2_pow_20() {
    let w = 1u32 << 20;
    let msb = bit(w, w - 1);
    let h = f(&msb, Hex, false, AUTO);
    assert_eq!(h.len(), 262144);
    assert!(h.starts_with('8'));
    assert!(h[1..].bytes().all(|c| c == b'0'));
    let x = f(&LogicVec::filled(w, Logic::X), Hex, false, AUTO);
    assert_eq!(x.len(), 262144);
    assert!(x.bytes().all(|c| c == b'x'));
}
