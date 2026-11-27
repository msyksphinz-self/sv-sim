//! 参照モデルのゴールデンテスト: シフト・幅の変更・連結・複製・選択・`$clog2`。

use svsim_logic_ref::*;

fn v(s: &str) -> RefVec {
    // 読みやすさのためテストでは '_' で区切って書き、渡す前に取り除く (bits 文字列は '_' を受けない、Q-22)。
    RefVec::from_bits_str(&s.replace('_', "")).unwrap()
}

fn amt(n: u64) -> RefVec {
    RefVec::from_u64(8, n)
}

// §11.4.10 の例: `start = 1; result = (start << 2);` → 4'b0100。
#[test]
fn s11_4_10_shift_left_example() {
    assert_eq!(shl(&v("0001"), &amt(2)).to_bits_string(), "0100");
}

// §11.4.10 の例: `reg signed [3:0] start = 4'b1000; result = (start >>> 2);` → 4'b1110。
#[test]
fn s11_4_10_arith_shift_example() {
    assert_eq!(ashr(&v("1000"), &amt(2), true).to_bits_string(), "1110");
    // 符号なしなら >> と同じ
    assert_eq!(ashr(&v("1000"), &amt(2), false).to_bits_string(), "0010");
}

// §11.4.10: x/z はそのまま移動する。空いたビットは 0。シフト量が幅以上なら全 0。
#[test]
fn s11_4_10_shift_rules() {
    assert_eq!(shl(&v("10xz"), &amt(1)).to_bits_string(), "0xz0");
    assert_eq!(shr(&v("10xz"), &amt(1)).to_bits_string(), "010x");
    assert_eq!(shr(&v("10xz"), &amt(0)).to_bits_string(), "10xz");
    assert_eq!(shl(&v("1111"), &amt(4)).to_bits_string(), "0000");
    assert_eq!(shr(&v("1111"), &amt(200)).to_bits_string(), "0000");
    // シフト量に未知があれば全 x。
    assert_eq!(shl(&v("1111"), &v("0x")).to_bits_string(), "xxxx");
    assert_eq!(ashr(&v("0111"), &v("z"), true).to_bits_string(), "xxxx");
    // 64 ビットを超えるシフト量 (上位ワードに 1) も幅以上として扱う。
    let mut huge = RefVec::zeros(100);
    huge.0[80] = RefLogic::One;
    assert_eq!(shl(&v("1111"), &huge).to_bits_string(), "0000");
}

// §11.4.10: >>> は符号ビットの値で埋める。x/z もそのまま [要VCS確認 V-3]。
#[test]
fn s11_4_10_arith_shift_unknown_sign_vcs_v3() {
    assert_eq!(ashr(&v("x000"), &amt(1), true).to_bits_string(), "xx00");
    assert_eq!(ashr(&v("z000"), &amt(1), true).to_bits_string(), "zz00");
    assert_eq!(ashr(&v("z000"), &amt(9), true).to_bits_string(), "zzzz");
    assert_eq!(ashr(&v("1010"), &amt(4), true).to_bits_string(), "1111");
}

// §11.6.1, §11.8.2, §6.24.1: 拡張は符号なしなら 0、符号付きなら符号ビット (x/z を含む)。
#[test]
fn s11_8_2_resize() {
    assert_eq!(resize(6, &v("1010"), false).to_bits_string(), "001010");
    assert_eq!(resize(6, &v("1010"), true).to_bits_string(), "111010");
    assert_eq!(resize(6, &v("x010"), true).to_bits_string(), "xxx010");
    assert_eq!(resize(6, &v("z010"), true).to_bits_string(), "zzz010");
    assert_eq!(resize(6, &v("z010"), false).to_bits_string(), "00z010");
    assert_eq!(resize(2, &v("10xz"), true).to_bits_string(), "xz");
    assert_eq!(resize(4, &v("10xz"), true).to_bits_string(), "10xz");
}

// §6.11.2: 4 値 → 2 値は x/z → 0。
#[test]
fn s6_11_2_to_two_state() {
    assert_eq!(to_two_state(&v("10xz")).to_bits_string(), "1000");
}

// §11.4.12: 連結。先頭の要素が MSB 側。
#[test]
fn s11_4_12_concat() {
    assert_eq!(
        concat(&[v("1"), v("0x"), v("z11")]).to_bits_string(),
        "10xz11"
    );
    assert_eq!(concat(&[]).width(), 0);
}

// §11.4.12.1 の例: `{4{w}}` は `{w, w, w, w}`、`{b, {3{a, b}}}` は `{b, a, b, a, b, a, b}`。
#[test]
fn s11_4_12_1_replication_examples() {
    let w = v("1x");
    assert_eq!(
        replicate(&w, 4),
        concat(&[w.clone(), w.clone(), w.clone(), w.clone()])
    );
    let (a, b) = (v("01"), v("z"));
    let ab = concat(&[a.clone(), b.clone()]);
    assert_eq!(
        concat(&[b.clone(), replicate(&ab, 3)]).to_bits_string(),
        "z01z01z01z"
    );
}

// §11.4.12.1: 0 回の複製は幅 0 で、連結の中では無視される。
#[test]
fn s11_4_12_1_zero_replication() {
    let z = replicate(&v("1"), 0);
    assert_eq!(z.width(), 0);
    assert_eq!(concat(&[z, v("1010")]).to_bits_string(), "1010");
}

// §11.5.1: 範囲外の読みは 4 値なら x。完全に範囲外も、x/z のインデックスも同じ。
#[test]
fn s11_5_1_select_out_of_range() {
    let a = v("1100_1010"); // logic [7:0] a
    assert_eq!(get_slice(1, &a, RefIndex::Value(1)).to_bits_string(), "1");
    assert_eq!(
        get_slice(4, &a, RefIndex::Value(2)).to_bits_string(),
        "0010"
    );
    // a[8] → x
    assert_eq!(get_slice(1, &a, RefIndex::Value(8)).to_bits_string(), "x");
    // a[9:6] → 上位 2 ビットが x
    assert_eq!(
        get_slice(4, &a, RefIndex::Value(6)).to_bits_string(),
        "xx11"
    );
    // 負のオフセット: a[1:-2] → 下位 2 ビットが x
    assert_eq!(
        get_slice(4, &a, RefIndex::Value(-2)).to_bits_string(),
        "10xx"
    );
    // 完全に範囲外
    assert_eq!(
        get_slice(3, &a, RefIndex::Value(100)).to_bits_string(),
        "xxx"
    );
    assert_eq!(
        get_slice(3, &a, RefIndex::Value(i64::MAX)).to_bits_string(),
        "xxx"
    );
    assert_eq!(
        get_slice(3, &a, RefIndex::Value(i64::MIN)).to_bits_string(),
        "xxx"
    );
    assert_eq!(get_slice(3, &a, RefIndex::Unknown).to_bits_string(), "xxx");
    // 2 値の選択 (範囲外は 0) は to_two_state で確かめる (§11.5.1, plan.md §3.5.1 規則 5)。
    assert_eq!(
        to_two_state(&get_slice(4, &a, RefIndex::Value(6))).to_bits_string(),
        "0011"
    );
}

// §11.5.1: 書き込みは範囲内のビットだけ。x/z のインデックスなら何も書かない。
#[test]
fn s11_5_1_set_slice() {
    let a = v("0000_0000");
    assert_eq!(
        set_slice(&a, RefIndex::Value(2), &v("1x1")).to_bits_string(),
        "0001x100"
    );
    assert_eq!(
        set_slice(&a, RefIndex::Value(6), &v("z11")).to_bits_string(),
        "11000000"
    );
    assert_eq!(
        set_slice(&a, RefIndex::Value(-1), &v("11z")).to_bits_string(),
        "00000011"
    );
    assert_eq!(set_slice(&a, RefIndex::Unknown, &v("11")), a);
    assert_eq!(set_slice(&a, RefIndex::Value(i64::MAX), &v("11")), a);
}

// §11.5.1: インデックスの解釈。
#[test]
fn s11_5_1_index_value() {
    assert_eq!(index_value(&v("0101"), false), RefIndex::Value(5));
    assert_eq!(index_value(&v("1111"), true), RefIndex::Value(-1));
    assert_eq!(index_value(&v("1111"), false), RefIndex::Value(15));
    assert_eq!(index_value(&v("01x1"), false), RefIndex::Unknown);
    let huge = RefVec::from_bits_str(&format!("1{}", "0".repeat(70))).unwrap();
    assert_eq!(index_value(&huge, false), RefIndex::Value(i64::MAX));
    assert_eq!(index_value(&huge, true), RefIndex::Value(i64::MIN));
}

// §20.8.1: $clog2。$clog2(0) = 0、$clog2(1) = 0、2 のべきの前後 [要VCS確認 V-10]。
#[test]
fn s20_8_1_clog2() {
    let c = |n: u64| clog2(&RefVec::from_u64(70, n));
    assert_eq!(c(0), Some(0));
    assert_eq!(c(1), Some(0));
    assert_eq!(c(2), Some(1));
    assert_eq!(c(3), Some(2));
    assert_eq!(c(4), Some(2));
    assert_eq!(c(5), Some(3));
    assert_eq!(c(255), Some(8));
    assert_eq!(c(256), Some(8));
    assert_eq!(c(257), Some(9));
    assert_eq!(c(1 << 63), Some(63));
    // 2^64 と 2^64 + 1 (多倍長)
    let mut p64 = RefVec::zeros(70);
    p64.0[64] = RefLogic::One;
    assert_eq!(clog2(&p64), Some(64));
    p64.0[0] = RefLogic::One;
    assert_eq!(clog2(&p64), Some(65));
    // 引数は符号なし
    assert_eq!(clog2(&v("1111")), Some(4));
    assert_eq!(clog2(&v("01x0")), None);
    assert_eq!(clog2(&v("z")), None);
}
