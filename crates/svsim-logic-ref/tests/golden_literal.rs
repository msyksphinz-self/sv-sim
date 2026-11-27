//! 参照モデルのゴールデンテスト: 整数リテラル (IEEE 1800-2023 §5.7.1) と文字列の変換。

use svsim_logic_ref::*;

fn lit(size: Option<u32>, signed: bool, radix: RefRadix, digits: &str) -> RefLiteral {
    parse_based(size, signed, radix, digits).unwrap()
}

fn bits(size: Option<u32>, radix: RefRadix, digits: &str) -> String {
    lit(size, false, radix, digits).value.to_bits_string()
}

use RefRadix::{Bin, Dec, Hex, Oct};

// §5.7.1 の例 (サイズ付き)。
#[test]
fn s5_7_1_sized_examples() {
    assert_eq!(bits(Some(4), Bin, "1001"), "1001"); // 4'b1001
    assert_eq!(bits(Some(5), Dec, "3"), "00011"); // 5'D 3
    assert_eq!(bits(Some(3), Bin, "01x"), "01x"); // 3'b01x
    assert_eq!(bits(Some(12), Hex, "x"), "x".repeat(12)); // 12'hx
    assert_eq!(bits(Some(16), Hex, "z"), "z".repeat(16)); // 16'hz
    assert_eq!(bits(Some(8), Hex, "AF"), "10101111"); // 8'h AF
    assert_eq!(bits(Some(12), Hex, "???"), "z".repeat(12)); // 12'h??? は z
    assert_eq!(bits(Some(6), Oct, "7x"), "111xxx");
    let l = lit(Some(4), true, Hex, "f"); // 4'shf
    assert_eq!(l.value.to_bits_string(), "1111");
    assert!(l.signed && l.sized && !l.truncated && !l.xz_extend);
}

// §5.7.1: 桁が少なければ左を 0 で、最左が x/z ならその値で埋める。
#[test]
fn s5_7_1_left_padding() {
    assert_eq!(bits(Some(8), Hex, "3x"), "0011xxxx");
    assert_eq!(bits(Some(12), Hex, "z3"), "zzzzzzzz0011");
    assert_eq!(bits(Some(12), Hex, "0z3"), "0000zzzz0011");
    assert_eq!(bits(Some(8), Bin, "x1"), "xxxxxxx1");
    assert_eq!(bits(Some(8), Bin, "0x1"), "000000x1");
    // 符号付きでもリテラルの段階では 0 で埋める (R-11): 4'sb1 → 0001
    assert_eq!(lit(Some(4), true, Bin, "1").value.to_bits_string(), "0001");
}

// §5.7.1: 桁が多ければ左を切り詰める。警告フラグ `truncated` は、落ちるビットに 0 以外
// (1/x/z) があるときだけ立てる (マネージャ決定 Q-3。Verilator と同じ挙動。V-リストに追加予定)。
#[test]
fn s5_7_1_truncation_flag_q3() {
    let t = |size, radix, d| lit(Some(size), false, radix, d);
    // 落ちるビットが 0 だけなら立たない
    assert_eq!(t(3, Hex, "7").value.to_bits_string(), "111");
    assert!(!t(3, Hex, "7").truncated);
    assert!(!t(4, Hex, "0F").truncated);
    assert!(!t(4, Oct, "17").truncated); // 001_111 → 1111
    assert_eq!(t(4, Oct, "17").value.to_bits_string(), "1111");
    // 落ちるビットに 1 があれば立つ
    let l = t(4, Oct, "37"); // 011_111 → 1111、落ちるのは 01
    assert_eq!(l.value.to_bits_string(), "1111");
    assert!(l.truncated);
    // 落ちる部分が x/z なら立つ
    let l = t(4, Hex, "xF");
    assert_eq!(l.value.to_bits_string(), "1111");
    assert!(l.truncated);
    assert!(t(2, Hex, "z").truncated); // 4'hz の上位 2 ビットの z が落ちる
    assert!(!t(6, Oct, "?7").truncated); // 6 ビットちょうど
    assert!(!t(8, Hex, "0x").truncated); // 2 桁 = 8 ビット、落ちるビットなし
}

#[test]
fn s5_7_1_truncation() {
    let l = lit(Some(4), false, Hex, "AB");
    assert_eq!(l.value.to_bits_string(), "1011");
    assert!(l.truncated);
    // 10 進は値で判定する (R-11): 4'd20 → 4
    let l = lit(Some(4), false, Dec, "20");
    assert_eq!(l.value.to_bits_string(), "0100");
    assert!(l.truncated);
    let l = lit(Some(4), false, Dec, "0015");
    assert_eq!(l.value.to_bits_string(), "1111");
    assert!(!l.truncated);
}

// §5.7.1: 10 進の x/z は 1 桁 (後ろに `_` 可) のときだけ (R-11)。
#[test]
fn s5_7_1_decimal_xz() {
    assert_eq!(bits(Some(8), Dec, "x_"), "x".repeat(8)); // 8'dx_
    assert_eq!(bits(Some(8), Dec, "?"), "z".repeat(8)); // 8'd?
    assert_eq!(bits(Some(16), Dec, "Z"), "z".repeat(16)); // 16'sd? 相当
    assert!(parse_based(Some(8), false, Dec, "1x").is_err());
    assert!(parse_based(Some(8), false, Dec, "x1").is_err());
    let l = lit(None, false, Dec, "x"); // 'dx
    assert_eq!(l.value.to_bits_string(), "x".repeat(32));
    assert!(l.xz_extend);
}

// §5.7.1: サイズなしは 32 ビット以上。最左が x/z なら xz_extend (文脈幅への拡張は sema)。
#[test]
fn s5_7_1_unsized() {
    let l = lit(None, false, Hex, "837FF"); // 'h 837FF
    assert_eq!(l.value.width(), 32);
    assert_eq!(
        l.value,
        RefVec::from_u64(32, 0x837FF),
        "{}",
        l.value.to_bits_string()
    );
    assert!(!l.sized && !l.xz_extend && !l.widened_unsized);
    let l = lit(None, false, Oct, "7460"); // 'o7460
    assert_eq!(l.value, RefVec::from_u64(32, 0o7460));
    // 'h x → 32 ビットの x、xz_extend
    let l = lit(None, false, Hex, "x");
    assert_eq!(l.value.to_bits_string(), "x".repeat(32));
    assert!(l.xz_extend);
    // 'h 3x → 03x (左は 0)
    let l = lit(None, false, Hex, "3x");
    assert_eq!(
        l.value.to_bits_string(),
        format!("{}0011xxxx", "0".repeat(24))
    );
    assert!(!l.xz_extend);
    // 'h z3 → zz3 (左は z)
    let l = lit(None, false, Hex, "z3");
    assert_eq!(l.value.to_bits_string(), format!("{}0011", "z".repeat(28)));
    assert!(l.xz_extend);
}

// 32 ビットを超えるサイズなしリテラルは必要な幅まで広げる [要VCS確認 V-11]。
#[test]
fn s5_7_1_unsized_widen_vcs_v11() {
    let l = lit(None, false, Hex, "1_0000_0000"); // 'h1_0000_0000
    assert_eq!(l.value.width(), 33);
    assert!(l.widened_unsized);
    // 先頭の 0 の桁だけで 32 ビットを超えても、値が収まれば 32 ビット
    let l = lit(None, false, Hex, "0_FFFF_FFFF");
    assert_eq!(l.value.width(), 32);
    assert!(!l.widened_unsized);
    let l = parse_unsized_decimal("4294967296").unwrap();
    assert_eq!(l.value.width(), 33);
    assert!(l.widened_unsized && l.signed && !l.sized);
}

// §5.7.1: 基数なしの 10 進は符号付きで 32 ビット。
#[test]
fn s5_7_1_unsized_decimal() {
    let l = parse_unsized_decimal("1_234").unwrap();
    assert_eq!(l.value, RefVec::from_u64(32, 1234));
    assert!(l.signed);
    assert!(parse_unsized_decimal("x").is_err());
    assert!(parse_unsized_decimal("").is_err());
}

// §5.7.1: 誤り。
#[test]
fn s5_7_1_errors() {
    assert!(parse_based(Some(0), false, Bin, "1").is_err());
    assert!(parse_based(Some(4), false, Bin, "102").is_err());
    assert!(parse_based(Some(4), false, Oct, "8").is_err());
    assert!(parse_based(Some(4), false, Hex, "g").is_err());
    assert!(parse_based(Some(4), false, Hex, "").is_err());
    assert!(parse_based(Some(4), false, Hex, "_1").is_err());
    assert_eq!(bits(Some(8), Hex, "f_f"), "11111111");
}

// §5.9, §11.10: 文字列リテラル → 整数。最後の文字が LSB 側。
#[test]
fn s11_10_string_to_integer() {
    assert_eq!(from_string_bytes(16, b"ab"), RefVec::from_u64(16, 0x6162));
    assert_eq!(from_string_bytes(24, b"ab"), RefVec::from_u64(24, 0x6162)); // 左を 0 で埋める
    assert_eq!(from_string_bytes(8, b"ab"), RefVec::from_u64(8, 0x62)); // 左を切り詰める
    assert_eq!(from_string_bytes(12, b"ab"), RefVec::from_u64(12, 0x162));
    // §11.10.3: "" は "\0"
    assert_eq!(from_string_bytes(8, b""), RefVec::zeros(8));
    // §11.10 の例: bit [8*14:1] stringvar = "Hello world" → 先頭 3 バイトが 0
    let s = from_string_bytes(8 * 14, b"Hello world");
    assert_eq!(to_string_bytes(&s), b"Hello world");
    assert!(s.0[8 * 11..].iter().all(|b| *b == RefLogic::Zero));
}

// §6.16, §6.24.1: 整数 → string。0 のバイトは捨てる。x/z は 0 として扱う [要VCS確認 V-5]。
#[test]
fn s6_16_integer_to_string() {
    assert_eq!(to_string_bytes(&RefVec::from_u64(16, 0x0041)), b"A");
    assert_eq!(to_string_bytes(&RefVec::from_u64(12, 0x441)), [0x04, 0x41]);
    assert_eq!(to_string_bytes(&RefVec::from_u64(24, 0x410042)), b"AB");
    let v = RefVec::from_bits_str("01000001xxxxzzzz").unwrap();
    assert_eq!(to_string_bytes(&v), b"A");
    let v = RefVec::from_bits_str("010000010x1z0010").unwrap(); // 0x1z0010 → 0010_0010
    assert_eq!(to_string_bytes(&v), [0x41, 0x22]);
}

// §6.16.9: atoi 系。符号・空白・接頭辞は解釈しない [要VCS確認 V-6]。
#[test]
fn s6_16_9_atoi() {
    let a = |s: &str, r| atoi(s.as_bytes(), r);
    assert_eq!(a("123abc", Dec), RefVec::from_u64(32, 123));
    assert_eq!(a("1_000", Dec), RefVec::from_u64(32, 1000));
    assert_eq!(a("-12", Dec), RefVec::zeros(32));
    assert_eq!(a(" 12", Dec), RefVec::zeros(32));
    assert_eq!(a("", Dec), RefVec::zeros(32));
    assert_eq!(a("0x1F", Hex), RefVec::zeros(32)); // '0' の後の 'x' で止まる
    assert_eq!(a("ff_FF", Hex), RefVec::from_u64(32, 0xffff));
    assert_eq!(a("778", Oct), RefVec::from_u64(32, 0o77));
    assert_eq!(a("1012", Bin), RefVec::from_u64(32, 0b101));
    // 32 ビットを超える分は切り詰める
    assert_eq!(a("4294967297", Dec), RefVec::from_u64(32, 1));
    assert_eq!(a("123456789abcdef", Hex), RefVec::from_u64(32, 0x89abcdef));
}
