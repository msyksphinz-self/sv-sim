//! 参照モデルのゴールデンテスト: 比較・等価・ワイルドカード・casez/casex・`?:`。
//! 期待値は IEEE 1800-2023 の条文と例から手で書いた。

use svsim_logic_ref::*;

use RefLogic::{One as I, X, Zero as O};

fn v(s: &str) -> RefVec {
    // 読みやすさのためテストでは '_' で区切って書き、渡す前に取り除く (bits 文字列は '_' を受けない、Q-22)。
    RefVec::from_bits_str(&s.replace('_', "")).unwrap()
}

// §11.4.5: 論理等価。既知の不一致があれば 0、なくて未知があれば x。
#[test]
fn s11_4_5_logical_equality() {
    assert_eq!(eq(&v("1010"), &v("1010")), I);
    assert_eq!(eq(&v("1010"), &v("1011")), O);
    assert_eq!(eq(&v("1x10"), &v("1010")), X);
    assert_eq!(eq(&v("1x10"), &v("0x10")), O); // 既知ビットの不一致が勝つ
    assert_eq!(eq(&v("z"), &v("z")), X); // == では z と z も x
    assert_eq!(ne(&v("1x10"), &v("1010")), X);
    assert_eq!(ne(&v("1x10"), &v("0010")), I);
    assert_eq!(ne(&v("0000"), &v("0000")), O);
}

// §11.4.5: case 等価。x は x とだけ、z は z とだけ一致する。
#[test]
fn s11_4_5_case_equality() {
    assert!(case_eq(&v("1x0z"), &v("1x0z")));
    assert!(!case_eq(&v("1x0z"), &v("1z0x")));
    assert!(!case_eq(&v("1x0z"), &v("1x00")));
    assert!(case_ne(&v("x"), &v("z")));
    assert!(!case_ne(&v("z"), &v("z")));
}

// §11.4.6: ワイルドカード等価。右の x/z はワイルドカード、左の x/z はそうではない。
#[test]
fn s11_4_6_wildcard_equality() {
    assert_eq!(wildcard_eq(&v("1010"), &v("10xz")), I);
    assert_eq!(wildcard_eq(&v("1110"), &v("10xz")), O);
    assert_eq!(wildcard_eq(&v("1x10"), &v("1010")), X); // 左の x は比較されて x
    assert_eq!(wildcard_eq(&v("1x10"), &v("1x10")), I); // 右の x の位置は無視
    assert_eq!(wildcard_eq(&v("0x10"), &v("1010")), O); // 既知の不一致が勝つ
    assert_eq!(wildcard_eq(&v("zzzz"), &v("xxxx")), I);
    assert_eq!(wildcard_ne(&v("1x10"), &v("1010")), X);
    assert_eq!(wildcard_ne(&v("1110"), &v("10xz")), I);
}

// §11.4.6 の 1 ビットの表 (行が左、列が右: 0, 1, x, z)。
#[test]
fn s11_4_6_wildcard_scalar_table() {
    let vals = ["0", "1", "x", "z"];
    let expected = ["1011", "0111", "xx11", "xx11"];
    for (i, row) in expected.iter().enumerate() {
        for (j, e) in row.chars().enumerate() {
            assert_eq!(
                wildcard_eq(&v(vals[i]), &v(vals[j])).to_char(),
                e,
                "{} ==? {}",
                vals[i],
                vals[j]
            );
        }
    }
}

// §12.5.1: casez は z (と `?`) を、casex は x と z を、どちら側にあっても無視する。
#[test]
fn s12_5_1_casez_casex() {
    // casez: 1z0? ≡ 1z0z
    assert!(casez_match(&v("1100"), &v("1z0z")));
    assert!(casez_match(&v("1z01"), &v("1101"))); // 左の z も無視
    assert!(!casez_match(&v("1x00"), &v("1100"))); // x は無視しない
    assert!(casez_match(&v("1x00"), &v("1x00"))); // x は x と一致
    assert!(!casez_match(&v("1x00"), &v("1z0x"))); // LSB の 0 と x は === で不一致
    assert!(!casez_match(&v("0000"), &v("1zzz")));
    // casex
    assert!(casex_match(&v("1x00"), &v("1100")));
    assert!(casex_match(&v("1100"), &v("1x0z")));
    assert!(!casex_match(&v("0100"), &v("1x0z")));
    assert!(casex_match(&v("xxxx"), &v("0101")));
}

// §12.5.1 の 1 ビットの表 (casez / casex)。
#[test]
fn s12_5_1_scalar_tables() {
    let vals = ["0", "1", "x", "z"];
    let casez = ["1001", "0101", "0011", "1111"];
    let casex = ["1011", "0111", "1111", "1111"];
    for i in 0..4 {
        for j in 0..4 {
            let (a, b) = (v(vals[i]), v(vals[j]));
            assert_eq!(
                casez_match(&a, &b),
                casez[i].as_bytes()[j] == b'1',
                "casez {} {}",
                vals[i],
                vals[j]
            );
            assert_eq!(
                casex_match(&a, &b),
                casex[i].as_bytes()[j] == b'1',
                "casex {} {}",
                vals[i],
                vals[j]
            );
        }
    }
}

// §11.4.4: 関係演算。未知があれば x。両方符号付きのときだけ符号付きで比べる (§11.8.1)。
#[test]
fn s11_4_4_relational() {
    // 4'b1111: 符号なしでは 15、符号付きでは -1。
    assert_eq!(lt(&v("1111"), &v("0001"), false), O);
    assert_eq!(lt(&v("1111"), &v("0001"), true), I);
    assert_eq!(gt(&v("1111"), &v("0001"), true), O);
    assert_eq!(le(&v("0011"), &v("0011"), false), I);
    assert_eq!(ge(&v("0011"), &v("0100"), false), O);
    assert_eq!(ge(&v("1000"), &v("0111"), true), O); // -8 >= 7
    assert_eq!(lt(&v("1x11"), &v("0001"), false), X);
    assert_eq!(gt(&v("0000"), &v("000z"), false), X);
    assert_eq!(le(&v("1000"), &v("1000"), true), I);
}

// §11.4.11: 条件が x のときのビットごとのマージ。両方 0 → 0、両方 1 → 1、それ以外は x。
#[test]
fn s11_4_11_cond_merge_table() {
    let vals = ["0", "1", "x", "z"];
    let expected = ["0xxx", "x1xx", "xxxx", "xxxx"];
    for (i, row) in expected.iter().enumerate() {
        for (j, e) in row.chars().enumerate() {
            assert_eq!(
                cond_merge(&v(vals[i]), &v(vals[j])).to_bits_string(),
                e.to_string()
            );
        }
    }
    assert_eq!(cond_merge(&v("0011"), &v("0101")).to_bits_string(), "0xx1");
}

// §11.4.11: `1'bx ? 1'bz : 1'bz` は表どおり x [要VCS確認 V-8] (Icarus は z)。
#[test]
fn s11_4_11_cond_x_z_z_vcs_v8() {
    assert_eq!(truth(&v("x")), X);
    assert_eq!(cond_merge(&v("z"), &v("z")).to_bits_string(), "x");
}
