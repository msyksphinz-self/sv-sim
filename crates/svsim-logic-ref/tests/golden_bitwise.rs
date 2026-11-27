//! 参照モデルのゴールデンテスト: ビット演算・リダクション・論理演算・ビット数え。
//! 期待値は IEEE 1800-2023 の表と例から手で書いた (実装の表を写していない)。

use svsim_logic_ref::*;

fn v(s: &str) -> RefVec {
    // 読みやすさのためテストでは '_' で区切って書き、渡す前に取り除く (bits 文字列は '_' を受けない、Q-22)。
    RefVec::from_bits_str(&s.replace('_', "")).unwrap()
}

const VALS: [&str; 4] = ["0", "1", "x", "z"];

/// 4×4 の表を「行ごとに 4 文字、空白区切り」の文字列で照合する。行が左、列が右 (0, 1, x, z)。
fn check_table(name: &str, expected: &str, f: impl Fn(&RefVec, &RefVec) -> String) {
    let rows: Vec<&str> = expected.split_whitespace().collect();
    assert_eq!(rows.len(), 4);
    for (i, row) in rows.iter().enumerate() {
        for (j, exp) in row.chars().enumerate() {
            let got = f(&v(VALS[i]), &v(VALS[j]));
            assert_eq!(
                got,
                exp.to_string(),
                "{name}: {} op {} expected {exp}, got {got}",
                VALS[i],
                VALS[j]
            );
        }
    }
}

// §11.4.8: ビット演算の表 (AND, OR, XOR, XNOR, NOT)。
#[test]
fn s11_4_8_bitwise_and_table() {
    check_table("&", "0000 01xx 0xxx 0xxx", |a, b| {
        and(a, b).to_bits_string()
    });
}

#[test]
fn s11_4_8_bitwise_or_table() {
    check_table("|", "01xx 1111 x1xx x1xx", |a, b| or(a, b).to_bits_string());
}

#[test]
fn s11_4_8_bitwise_xor_table() {
    check_table("^", "01xx 10xx xxxx xxxx", |a, b| {
        xor(a, b).to_bits_string()
    });
}

#[test]
fn s11_4_8_bitwise_xnor_table() {
    check_table("~^", "10xx 01xx xxxx xxxx", |a, b| {
        xnor(a, b).to_bits_string()
    });
}

#[test]
fn s11_4_8_bitwise_not_table() {
    let got: Vec<String> = VALS.iter().map(|s| not(&v(s)).to_bits_string()).collect();
    assert_eq!(got, ["1", "0", "x", "x"]);
}

#[test]
fn s11_4_8_bitwise_multibit() {
    assert_eq!(
        and(&v("10xz_01xz"), &v("1111_0000")).to_bits_string(),
        "10xx0000"
    );
    assert_eq!(
        or(&v("10xz_01xz"), &v("1111_0000")).to_bits_string(),
        "111101xx"
    );
    assert_eq!(not(&v("10xz")).to_bits_string(), "01xx");
}

// §11.4.9: リダクションの例 (表の 4 行)。各行は & ~& | ~| ^ ~^ の順。
#[test]
fn s11_4_9_reduction_examples() {
    let cases = [
        ("0000", "010101"),
        ("1111", "101001"),
        ("0110", "011001"),
        ("1000", "011010"),
    ];
    for (x, exp) in cases {
        let x = v(x);
        let got: String = [
            reduce_and(&x),
            reduce_nand(&x),
            reduce_or(&x),
            reduce_nor(&x),
            reduce_xor(&x),
            reduce_xnor(&x),
        ]
        .iter()
        .map(|b| b.to_char())
        .collect();
        assert_eq!(got, exp, "reduction of {}", x.to_bits_string());
    }
}

// §11.4.9: x/z を含むリダクション。AND は既知の 0、OR は既知の 1 が勝つ。XOR は未知があれば x。
#[test]
fn s11_4_9_reduction_unknown() {
    assert_eq!(reduce_and(&v("1x0z")), RefLogic::Zero);
    assert_eq!(reduce_and(&v("1x1z")), RefLogic::X);
    assert_eq!(reduce_nand(&v("1x1z")), RefLogic::X);
    assert_eq!(reduce_or(&v("0x1z")), RefLogic::One);
    assert_eq!(reduce_or(&v("0x0z")), RefLogic::X);
    assert_eq!(reduce_nor(&v("0z00")), RefLogic::X);
    assert_eq!(reduce_xor(&v("1101")), RefLogic::One);
    assert_eq!(reduce_xor(&v("110z")), RefLogic::X);
    assert_eq!(reduce_xnor(&v("11x1")), RefLogic::X);
    assert_eq!(reduce_and(&v("z")), RefLogic::X);
}

// §11.4.7, §12.4: 真理値と論理演算。
#[test]
fn s11_4_7_truth_and_logical() {
    assert_eq!(truth(&v("0000")), RefLogic::Zero);
    assert_eq!(truth(&v("00x0")), RefLogic::X);
    assert_eq!(truth(&v("0z10")), RefLogic::One);
    assert_eq!(logical_not(&v("0000")), RefLogic::One);
    assert_eq!(logical_not(&v("0z00")), RefLogic::X);
    assert_eq!(logical_not(&v("0100")), RefLogic::Zero);
    // && / || は AND / OR の表を真理値に適用する。
    assert_eq!(logical_and(&v("0"), &v("x")), RefLogic::Zero);
    assert_eq!(logical_and(&v("10"), &v("x")), RefLogic::X);
    assert_eq!(logical_or(&v("x"), &v("01")), RefLogic::One);
    assert_eq!(logical_or(&v("x"), &v("00")), RefLogic::X);
    // a -> b = !a || b
    assert_eq!(logical_implies(&v("0"), &v("x")), RefLogic::One);
    assert_eq!(logical_implies(&v("1"), &v("0")), RefLogic::Zero);
    assert_eq!(logical_implies(&v("x"), &v("1")), RefLogic::One);
    assert_eq!(logical_implies(&v("x"), &v("0")), RefLogic::X);
    // a <-> b
    assert_eq!(logical_equiv(&v("0"), &v("0")), RefLogic::One);
    assert_eq!(logical_equiv(&v("1"), &v("0")), RefLogic::Zero);
    assert_eq!(logical_equiv(&v("10"), &v("01")), RefLogic::One);
    assert_eq!(logical_equiv(&v("x"), &v("1")), RefLogic::X);
}

// §11.4.7 の 1 ビット版の表 (&&, ||, ->, <->)。
#[test]
fn s11_4_7_logic_scalar_tables() {
    check_table("&&", "0000 01xx 0xxx 0xxx", |a, b| {
        logical_and(a, b).to_char().to_string()
    });
    check_table("||", "01xx 1111 x1xx x1xx", |a, b| {
        logical_or(a, b).to_char().to_string()
    });
    check_table("->", "1111 01xx x1xx x1xx", |a, b| {
        logical_implies(a, b).to_char().to_string()
    });
    check_table("<->", "10xx 01xx xxxx xxxx", |a, b| {
        logical_equiv(a, b).to_char().to_string()
    });
}

// §20.9: $countbits, $countones, $onehot, $onehot0, $isunknown。
#[test]
fn s20_9_bit_counting() {
    let x = v("10xz_z110");
    assert_eq!(count_bits(&x, &[RefLogic::One]), 3);
    assert_eq!(count_bits(&x, &[RefLogic::Zero]), 2);
    assert_eq!(count_bits(&x, &[RefLogic::X, RefLogic::Z]), 3);
    assert_eq!(count_bits(&x, &[RefLogic::Z]), 2);
    assert_eq!(count_bits(&x, &[]), 0);
    assert_eq!(count_bits(&x, &RefLogic::ALL), 8);
    assert!(onehot(&v("0100")));
    assert!(!onehot(&v("0000")));
    assert!(!onehot(&v("0110")));
    assert!(onehot(&v("01x0"))); // x は '1 として数えない
    assert!(onehot0(&v("0000")));
    assert!(onehot0(&v("1000")));
    assert!(!onehot0(&v("1001")));
    assert!(is_unknown(&v("000z")));
    assert!(is_unknown(&v("x000")));
    assert!(!is_unknown(&v("1010")));
}
