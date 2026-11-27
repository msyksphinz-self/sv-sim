//! 中核 (構築・変換・bit の get/set・ビュー・DPI 形式との変換) の差分テストとゴールデンテスト
//! (plan.md §5.2、test-plan TC10〜TC12, TC50, TC51, TC56〜TC58。タスク M0-T3b)。
//!
//! 参照モデル `svsim-logic-ref` とは bits 文字列で値を受け渡す (plan.md §3.7)。

mod common;

use common::*;
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::TestRunner;
use svsim_logic::abi::{
    read_sv_bit_vec_val, read_sv_logic_vec_val, sv_bit_vec_val_len, sv_logic_vec_val_len,
    write_sv_bit_vec_val, write_sv_logic_vec_val,
};
use svsim_logic::{BitMut, BitRef, BitVec, L64, Logic, LogicMut, LogicRef, LogicVec, words_for};
use svsim_logic_ref::{RefEdge, RefLogic, RefVec};

const ALL_LOGIC: [Logic; 4] = [Logic::L0, Logic::L1, Logic::X, Logic::Z];

fn any_logic() -> impl Strategy<Value = Logic> {
    prop::sample::select(&ALL_LOGIC[..])
}

/// 幅 `w` の値とビット位置 `0..w`。
fn vec_and_index() -> impl Strategy<Value = (LogicVec, u32)> {
    width().prop_flat_map(|w| (logic_vec(w), 0..w))
}

/// 参照モデルの値から、`svLogicVecVal` の要素 `k` (ビット `32k..32k+31`) の aval/bval を作る
/// (Annex I: 0=(0,0)、1=(1,0)、z=(0,1)、x=(1,1))。
fn ref_sv_logic_elem(r: &RefVec, k: usize) -> (u32, u32) {
    let (mut a, mut b) = (0u32, 0u32);
    for j in 0..32 {
        let i = 32 * k + j;
        if i >= r.width() {
            break;
        }
        let (ab, bb) = match r.bit(i) {
            RefLogic::Zero => (0, 0),
            RefLogic::One => (1, 0),
            RefLogic::Z => (0, 1),
            RefLogic::X => (1, 1),
        };
        a |= ab << j;
        b |= bb << j;
    }
    (a, b)
}

/// 最上位要素の有効ビットのマスク (32 ビット単位)。
fn top_mask32(w: u32) -> u32 {
    match w % 32 {
        0 => !0,
        r => (1u32 << r) - 1,
    }
}

// ===========================================================================
// TC10: 構築と変換 (参照モデルとの差分)
// ===========================================================================

proptest! {
    /// `from_u64` / `from_i64` (2 の補数で幅に切り詰める/符号拡張する、§11.8.2) が参照モデルと一致する。
    #[test]
    fn from_u64_i64_match_ref(w in width(), v in any::<u64>(), sv in any::<i64>()) {
        let x = LogicVec::from_u64(w, v);
        check_eq_ref(x.as_ref(), &RefVec::from_u64(w as usize, v), "LogicVec::from_u64")?;
        let x = LogicVec::from_i64(w, sv);
        check_eq_ref(x.as_ref(), &RefVec::from_i64(w as usize, sv), "LogicVec::from_i64")?;
        let b = BitVec::from_u64(w, v);
        check_eq_ref2(b.as_ref(), &RefVec::from_u64(w as usize, v), "BitVec::from_u64")?;
        let b = BitVec::from_i64(w, sv);
        check_eq_ref2(b.as_ref(), &RefVec::from_i64(w as usize, sv), "BitVec::from_i64")?;
    }

    /// `zero` / `filled` が参照モデルと一致し、正規化されている。
    #[test]
    fn zero_filled_match_ref(w in width(), v in any_logic()) {
        check_eq_ref(LogicVec::zero(w).as_ref(), &RefVec::zeros(w as usize), "zero")?;
        check_eq_ref(
            LogicVec::filled(w, v).as_ref(),
            &RefVec::filled(w as usize, logic_to_ref(v)),
            "filled",
        )?;
        check_eq_ref2(BitVec::zero(w).as_ref(), &RefVec::zeros(w as usize), "BitVec::zero")?;
        check_eq_ref2(
            BitVec::filled(w, true).as_ref(),
            &RefVec::filled(w as usize, RefLogic::One),
            "BitVec::filled",
        )?;
    }

    /// bits 文字列 (Q-22): 本体と参照モデルが同じ文字列を受け、同じ小文字の文字列に戻す。
    #[test]
    fn bits_string_round_trip(s in "[01xXzZ]{0,1024}") {
        let x = LogicVec::from_bits_str(&s).expect("valid bits string");
        let r = RefVec::from_bits_str(&s).expect("valid bits string");
        prop_assert_eq!(x.width() as usize, s.len());
        prop_assert_eq!(x.to_bits_string(), s.to_ascii_lowercase());
        check_eq_ref(x.as_ref(), &r, "from_bits_str")?;
    }

    /// 不正な文字を含む bits 文字列は、本体も参照モデルもエラーにする (Q-22)。
    #[test]
    fn bits_string_rejects_invalid(
        pre in "[01xz]{0,10}",
        bad in "[^01xXzZ]",
        post in "[01xz]{0,10}",
    ) {
        let s = format!("{pre}{bad}{post}");
        prop_assert!(LogicVec::from_bits_str(&s).is_err(), "LogicVec accepted {:?}", s);
        prop_assert!(RefVec::from_bits_str(&s).is_err(), "RefVec accepted {:?}", s);
        prop_assert!(BitVec::from_bits_str(&s).is_err(), "BitVec accepted {:?}", s);
    }

    /// 参照モデルとの往復 (`to_ref` → `from_ref`) で値が変わらない。
    #[test]
    fn ref_round_trip(x in any_logic_vec(), b in any_bit_vec()) {
        let back = from_ref(&to_ref(&x));
        check_eq(back.as_ref(), x.as_ref(), "from_ref(to_ref(x))")?;
        let back = from_ref_bits(&bits_to_ref(&b));
        check_eq2(back.as_ref(), b.as_ref(), "from_ref_bits(bits_to_ref(b))")?;
    }

    /// `to_u64` / `to_i64` が参照モデルの整数値 (X/Z があれば無し) と一致する。
    #[test]
    fn to_u64_i64_match_ref(x in any_logic_vec(), signed in any::<bool>()) {
        let r = to_ref(&x);
        let want_u = r.to_biguint().and_then(|v| v.to_string().parse::<u64>().ok());
        prop_assert_eq!(x.to_u64(), want_u, "to_u64 of {:?}", x);
        let want_i = r.to_bigint(signed).and_then(|v| v.to_string().parse::<i64>().ok());
        prop_assert_eq!(x.to_i64(signed), want_i, "to_i64({}) of {:?}", signed, x);
        let b = x.to_bit_vec();
        let rb = bits_to_ref(&b);
        let want_u = rb.to_biguint().and_then(|v| v.to_string().parse::<u64>().ok());
        prop_assert_eq!(b.to_u64(), want_u, "BitVec::to_u64 of {:?}", b);
        let want_i = rb.to_bigint(signed).and_then(|v| v.to_string().parse::<i64>().ok());
        prop_assert_eq!(b.to_i64(signed), want_i, "BitVec::to_i64 of {:?}", b);
    }

    /// 2 値/4 値の変換 (§6.11.2: X/Z → 0、2 値 → 4 値は bval = 0) が参照モデルと一致する。
    #[test]
    fn two_state_conversion_match_ref(x in any_logic_vec()) {
        let b = x.to_bit_vec();
        check_eq_ref2(b.as_ref(), &svsim_logic_ref::to_two_state(&to_ref(&x)), "to_bit_vec")?;
        let back = LogicVec::from(&b);
        check_eq_ref(back.as_ref(), &bits_to_ref(&b), "LogicVec::from(&BitVec)")?;
        prop_assert!(!back.has_unknown());
        check_eq(b.to_logic_vec().as_ref(), back.as_ref(), "to_logic_vec")?;
    }

    /// `has_unknown` が参照モデルと一致する。
    #[test]
    fn has_unknown_matches_ref(x in any_logic_vec()) {
        prop_assert_eq!(x.has_unknown(), to_ref(&x).has_unknown());
        prop_assert_eq!(x.as_ref().has_unknown(), to_ref(&x).has_unknown());
    }
}

// ===========================================================================
// TC10/TC11: bit の get/set とビュー
// ===========================================================================

proptest! {
    /// `bit(i)` が参照モデルの `bit(i)` と一致する (層 3、層 2 の両方)。
    #[test]
    fn bit_get_matches_ref((x, i) in vec_and_index()) {
        let r = to_ref(&x);
        let want = r.bit(i as usize);
        check_logic_eq_reflogic(x.bit(i), want, "LogicVec::bit")?;
        check_logic_eq_reflogic(x.as_ref().bit(i), want, "LogicRef::bit")?;
        let split = SplitLogic::from_vec(&x);
        check_logic_eq_reflogic(split.as_ref().bit(i), want, "LogicRef::bit (split)")?;
        let b = x.to_bit_vec();
        check_bool_eq_ref(b.bit(i), ref_to_two_state_bool(want), "BitVec::bit")?;
        check_bool_eq_ref(b.as_ref().bit(i), ref_to_two_state_bool(want), "BitRef::bit")?;
    }

    /// `set_bit(i, v)` が参照モデルで 1 ビットを書き換えたものと一致し、他のビットを変えない。
    #[test]
    fn bit_set_matches_ref((x, i) in vec_and_index(), v in any_logic()) {
        let mut r = to_ref(&x);
        r.0[i as usize] = logic_to_ref(v);

        let mut y = x.clone();
        y.set_bit(i, v);
        check_eq_ref(y.as_ref(), &r, "LogicVec::set_bit")?;

        let mut split = SplitLogic::from_vec(&x);
        split.as_mut().set_bit(i, v);
        check_eq_ref(split.as_ref(), &r, "LogicMut::set_bit (split)")?;
        check_logic_eq_reflogic(split.as_mut().bit(i), logic_to_ref(v), "LogicMut::bit")?;

        // 2 値: X/Z を書けないので bool で書く。期待値は参照モデルを 2 値化してから書き換えたもの。
        let bv = v == Logic::L1;
        let mut rb = svsim_logic_ref::to_two_state(&to_ref(&x));
        rb.0[i as usize] = RefLogic::from_bool(bv);
        let mut b = x.to_bit_vec();
        b.set_bit(i, bv);
        check_eq_ref2(b.as_ref(), &rb, "BitVec::set_bit")?;
        let mut sb = SplitBits::from_vec(&x.to_bit_vec());
        sb.as_mut().set_bit(i, bv);
        check_eq_ref2(sb.as_ref(), &rb, "BitMut::set_bit (split)")?;
    }

    /// プレーン分離配置 (`from_planar`) と、プレーンが隣接しないビュー (`new`) が同じ結果を出す (TC11)。
    #[test]
    fn planar_and_split_views_agree(x in any_logic_vec()) {
        let split = SplitLogic::from_vec(&x);
        let mut planar = split.a.clone();
        planar.extend_from_slice(&split.b);
        let p = LogicRef::from_planar(x.width(), &planar);
        let s = split.as_ref();
        prop_assert_eq!(p.aval(), s.aval());
        prop_assert_eq!(p.bval(), s.bval());
        prop_assert_eq!(p.has_unknown(), s.has_unknown());
        prop_assert_eq!(p.to_bits_string(), s.to_bits_string());
        check_eq_ref(p, &to_ref(&x), "from_planar")?;
        check_eq(LogicVec::from_ref(p).as_ref(), x.as_ref(), "LogicVec::from_ref")?;

        // LogicMut::from_planar で書いても同じ。
        let mut buf = vec![!0u64; 2 * words_for(x.width())];
        let mut m = LogicMut::from_planar(x.width(), &mut buf);
        m.copy_from(x.as_ref());
        check_eq(m.as_ref(), x.as_ref(), "LogicMut::from_planar + copy_from")?;
    }

    /// `fill` が参照モデルの `filled` と一致し、正規化されている (CR-2)。
    #[test]
    fn fill_matches_ref(x in any_logic_vec(), v in any_logic()) {
        let w = x.width();
        let mut split = SplitLogic::from_vec(&x);
        split.as_mut().fill(v);
        check_eq_ref(split.as_ref(), &RefVec::filled(w as usize, logic_to_ref(v)), "LogicMut::fill")?;
        let mut sb = SplitBits::from_vec(&x.to_bit_vec());
        sb.as_mut().fill(true);
        check_eq_ref2(sb.as_ref(), &RefVec::filled(w as usize, RefLogic::One), "BitMut::fill")?;
    }
}

/// TC11: 幅 129 の境界のビットに 0/1/x/z を書いて読み戻す。他のビットは変わらない。
#[test]
fn bit_set_get_at_word_boundaries() {
    let w = 129;
    for &i in &[0u32, 63, 64, 127, 128] {
        for &v in &ALL_LOGIC {
            let base = LogicVec::from_bits_str(&"01xz".repeat(33)[..129]).unwrap();
            let mut x = base.clone();
            x.set_bit(i, v);
            assert_eq!(x.bit(i), v, "bit {i}");
            for j in (0..w).filter(|&j| j != i) {
                assert_eq!(x.bit(j), base.bit(j), "bit {j} changed by set_bit({i})");
            }
            assert!(x.as_ref().is_normalized());
        }
    }
    // fill(X) (w = 65): 全 x、bit 65 以上は 0 (CR-2)。
    let mut x = LogicVec::zero(65);
    x.as_mut().fill(Logic::X);
    assert_eq!(x.as_ref().aval(), &[!0, 1]);
    assert_eq!(x.as_ref().bval(), &[!0, 1]);
    // has_unknown: 全 0 → false、bit 128 だけ z → true。
    let mut y = LogicVec::zero(129);
    assert!(!y.has_unknown());
    y.set_bit(128, Logic::Z);
    assert!(y.has_unknown());
}

// ===========================================================================
// TC10: ゴールデン
// ===========================================================================

/// test-plan TC10 の表 (構築と変換)。
#[test]
fn construction_golden() {
    assert_eq!(LogicVec::from_u64(4, 0x1f).to_bits_string(), "1111");
    let x = LogicVec::from_u64(128, 0xdead);
    assert_eq!(x.as_ref().aval(), &[0xdead, 0]);
    assert_eq!(LogicVec::from_i64(4, -1).to_bits_string(), "1111");
    assert_eq!(LogicVec::from_i64(4, 8).to_bits_string(), "1000");
    assert_eq!(LogicVec::from_i64(65, -1).to_bits_string(), "1".repeat(65));
    let x = LogicVec::from_i64(63, -1);
    assert_eq!(
        x.as_ref().aval(),
        &[u64::MAX >> 1],
        "bit 63 以上は 0 (CR-2)"
    );
    for w in [1u32, 64, 65, 129] {
        assert!(LogicVec::zero(w).as_ref().is_normalized());
        assert_eq!(
            LogicVec::filled(w, Logic::X).to_bits_string(),
            "x".repeat(w as usize)
        );
        assert_eq!(
            LogicVec::filled(w, Logic::Z).to_bits_string(),
            "z".repeat(w as usize)
        );
        assert!(LogicVec::filled(w, Logic::X).as_ref().is_normalized());
        assert!(LogicVec::filled(w, Logic::Z).as_ref().is_normalized());
    }
    assert_eq!(
        LogicVec::from_bits_str("10XZ01").unwrap().to_bits_string(),
        "10xz01"
    );
    assert_eq!(LogicVec::from_bits_str("").unwrap().width(), 0);
    for bad in ["10xz_01", "102", "1 0", "?"] {
        assert!(LogicVec::from_bits_str(bad).is_err(), "{bad:?}");
    }
    // to_u64: 65 ビットで bit 64=0・下位=5 / bit 64=1 / X を含む。
    assert_eq!(LogicVec::from_u64(65, 5).to_u64(), Some(5));
    let mut x = LogicVec::from_u64(65, 5);
    x.set_bit(64, Logic::L1);
    assert_eq!(x.to_u64(), None);
    x.set_bit(64, Logic::X);
    assert_eq!(x.to_u64(), None);
    let ff = LogicVec::from_u64(8, 0xff);
    assert_eq!(ff.to_i64(true), Some(-1));
    assert_eq!(ff.to_i64(false), Some(255));
    assert_eq!(LogicVec::from_i64(65, -1).to_i64(true), Some(-1));
    // §6.11.2: X/Z → 0
    let v = LogicVec::from_bits_str("1xz0").unwrap();
    assert_eq!(v.to_bit_vec().to_bits_string(), "1000");
    assert_eq!(
        format!("{:?}", LogicVec::from_bits_str("10xz0001").unwrap()),
        "8'b10xz_0001"
    );
    assert_ne!(LogicVec::from_u64(4, 1), LogicVec::from_u64(8, 1));
}

// ===========================================================================
// TC12: 正規化の検査器の自己テストと、違反しやすい構築
// ===========================================================================

/// TC12.1: 検査器は、最上位ワードの未使用ビットに 1 がある値 (aval 側、bval 側) を違反として検出する。
#[test]
fn normalization_checker_detects_violations() {
    for w in [1u32, 63, 65, 127, 129] {
        let n = words_for(w);
        let good_a = vec![0u64; n];
        let good_b = vec![0u64; n];
        let mut bad = vec![0u64; n];
        if w % 64 != 0 {
            bad[n - 1] = 1 << (w % 64);
        }
        assert!(check_normalized(LogicRef::new(w, &good_a, &good_b), "good").is_ok());
        assert!(check_normalized(LogicRef::new(w, &bad, &good_b), "bad aval").is_err());
        assert!(check_normalized(LogicRef::new(w, &good_a, &bad), "bad bval").is_err());
        assert!(check_normalized2(BitRef::new(w, &good_a), "good").is_ok());
        assert!(check_normalized2(BitRef::new(w, &bad), "bad").is_err());
        let r = RefVec::zeros(w as usize);
        assert!(check_eq_ref(LogicRef::new(w, &bad, &good_b), &r, "bad").is_err());
    }
    assert!(check_normalized_l64(L64::new(0b1000, 0), 3, "bad").is_err());
    assert!(check_normalized_l64(L64::new(0, 0b1000), 3, "bad").is_err());
    assert!(check_normalized_l64(L64::new(0b111, 0b111), 3, "good").is_ok());
}

/// TC12.2 (T3a/T3b の範囲): 違反しやすい構築・変換の出力が正規化されている。
#[test]
fn normalization_of_risky_constructions() {
    for w in [1u32, 63, 65, 127, 129] {
        let mut x = LogicVec::zero(w);
        x.as_mut().fill(Logic::X);
        assert!(x.as_ref().is_normalized(), "fill(X) width {w}");
        assert!(
            LogicVec::from_i64(w, -1).as_ref().is_normalized(),
            "from_i64(-1) width {w}"
        );
        assert!(
            BitVec::from_i64(w, -1).as_ref().is_normalized(),
            "BitVec::from_i64(-1) width {w}"
        );
        assert!(
            LogicVec::filled(w, Logic::Z).as_ref().is_normalized(),
            "filled(Z) width {w}"
        );
        let garbage = vec![0xffff_ffffu32; sv_logic_vec_val_len(w)];
        read_sv_logic_vec_val(&garbage, x.as_mut());
        assert!(
            x.as_ref().is_normalized(),
            "read_sv_logic_vec_val width {w}"
        );
        assert_eq!(x, LogicVec::filled(w, Logic::X));
        let mut b = BitVec::zero(w);
        read_sv_bit_vec_val(&vec![!0u32; sv_bit_vec_val_len(w)], b.as_mut());
        assert!(b.as_ref().is_normalized());
        assert_eq!(b, BitVec::filled(w, true));
    }
}

// ===========================================================================
// TC50: DPI 形式 (Annex I, svdpi.h) のゴールデン
// ===========================================================================

fn sv_logic(x: &LogicVec) -> Vec<u32> {
    let mut buf = vec![0xdead_beefu32; sv_logic_vec_val_len(x.width())];
    write_sv_logic_vec_val(x.as_ref(), &mut buf);
    buf
}

/// test-plan TC50: `write_sv_logic_vec_val` の出力。
#[test]
fn sv_logic_vec_val_golden() {
    let b = |s: &str| LogicVec::from_bits_str(s).unwrap();
    assert_eq!(sv_logic(&b("x")), [1, 1]);
    assert_eq!(sv_logic(&b("z")), [0, 1]);
    assert_eq!(
        sv_logic(&LogicVec::from_u64(32, 0x89ab_cdef)),
        [0x89ab_cdef, 0]
    );
    // 40 ビット: bit 39..32 = x0z1_0000、下位 = 32'h89ab_cdef
    let mut v = LogicVec::from_u64(40, 0x89ab_cdef);
    for (j, c) in "x0z10000".chars().rev().enumerate() {
        v.set_bit(32 + j as u32, Logic::from_char(c).unwrap());
    }
    assert_eq!(sv_logic(&v), [0x89ab_cdef, 0x0, 0x90, 0xa0]);
    // 64 ビット: bit 63 だけ z
    let mut v = LogicVec::zero(64);
    v.set_bit(63, Logic::Z);
    assert_eq!(sv_logic(&v), [0, 0, 0, 0x8000_0000]);
    assert_eq!(sv_logic(&LogicVec::filled(64, Logic::L1)), [!0, 0, !0, 0]);
    // 65 ビット: bit 64 だけ 1
    let mut v = LogicVec::zero(65);
    v.set_bit(64, Logic::L1);
    assert_eq!(sv_logic(&v), [0, 0, 0, 0, 1, 0]);
    // 129 ビット全 x
    let mut want = vec![0xffff_ffffu32; 8];
    want.extend([1, 1]);
    assert_eq!(sv_logic(&LogicVec::filled(129, Logic::X)), want);
}

/// test-plan TC50: 読み込み時のマスクと 2 値の形式、配列の長さ。
#[test]
fn sv_vec_val_read_mask_and_lengths_golden() {
    let mut x = LogicVec::zero(33);
    read_sv_logic_vec_val(&[0xffff_ffff, 0, 0xffff_ffff, 0xffff_fffe], x.as_mut());
    // bit 32: a=1, b=0 → 1。上位は読み捨てる。
    assert_eq!(x, LogicVec::from_u64(33, 0x1_ffff_ffff));
    assert!(x.as_ref().is_normalized());

    let mut x = LogicVec::zero(1);
    read_sv_logic_vec_val(&[0xffff_fffe, 0xffff_ffff], x.as_mut());
    assert_eq!(x.to_bits_string(), "z");

    let mut x = LogicVec::zero(64);
    read_sv_logic_vec_val(&[!0; 4], x.as_mut());
    assert_eq!(x, LogicVec::filled(64, Logic::X));

    let b = BitVec::from_u64(33, 0x1_1234_5678);
    let mut buf = [0u32; 2];
    write_sv_bit_vec_val(b.as_ref(), &mut buf);
    assert_eq!(buf, [0x1234_5678, 0x1]);
    let mut c = BitVec::zero(33);
    read_sv_bit_vec_val(&[0x1234_5678, 0xffff_ffff], c.as_mut());
    assert_eq!(c, b);

    for w in [1u32, 31, 32, 33, 63, 64, 65, 127, 128, 129] {
        let n = ((w + 31) >> 5) as usize;
        assert_eq!(sv_bit_vec_val_len(w), n);
        assert_eq!(sv_logic_vec_val_len(w), 2 * n);
    }
}

// ===========================================================================
// TC51: DPI 変換の往復とゼロコピー同値性
// ===========================================================================

/// 幅 `w` の u32 配列 (未使用ビットにも乱数を入れたもの)。
fn sv_words(len: usize) -> impl Strategy<Value = Vec<u32>> {
    prop::collection::vec(
        prop_oneof![Just(0u32), Just(!0u32), any::<u32>(), Just(1 << 31)],
        len,
    )
}

fn width_and_sv_logic() -> impl Strategy<Value = (u32, Vec<u32>)> {
    width().prop_flat_map(|w| (Just(w), sv_words(sv_logic_vec_val_len(w))))
}

fn width_and_sv_bits() -> impl Strategy<Value = (u32, Vec<u32>)> {
    width().prop_flat_map(|w| (Just(w), sv_words(sv_bit_vec_val_len(w))))
}

proptest! {
    /// TC51.1: `read(write(v)) == v` (4 値、2 値)。プレーンが隣接しないビューでも同じ。
    #[test]
    fn sv_round_trip(x in any_logic_vec()) {
        let w = x.width();
        let buf = sv_logic(&x);
        let mut back = SplitLogic::dirty(w);
        read_sv_logic_vec_val(&buf, back.as_mut());
        check_eq(back.as_ref(), x.as_ref(), "read(write(v))")?;

        let b = x.to_bit_vec();
        let mut buf2 = vec![0xdead_beefu32; sv_bit_vec_val_len(w)];
        write_sv_bit_vec_val(b.as_ref(), &mut buf2);
        let mut back2 = SplitBits::dirty(w);
        read_sv_bit_vec_val(&buf2, back2.as_mut());
        check_eq2(back2.as_ref(), b.as_ref(), "read(write(b))")?;
    }

    /// TC51.2: 任意の u32 配列 a で `write(read(a))` は a の未使用ビットを 0 にしたもの。`read(a)` は正規化済み。
    #[test]
    fn sv_logic_read_masks((w, a) in width_and_sv_logic()) {
        let mut x = SplitLogic::dirty(w);
        read_sv_logic_vec_val(&a, x.as_mut());
        check_normalized(x.as_ref(), "read_sv_logic_vec_val")?;
        let mut out = vec![0u32; a.len()];
        write_sv_logic_vec_val(x.as_ref(), &mut out);
        let mut want = a.clone();
        let n = want.len();
        if n > 0 {
            want[n - 2] &= top_mask32(w);
            want[n - 1] &= top_mask32(w);
        }
        prop_assert_eq!(out, want);
    }

    /// TC51.2 (2 値)。
    #[test]
    fn sv_bit_read_masks((w, a) in width_and_sv_bits()) {
        let mut x = SplitBits::dirty(w);
        read_sv_bit_vec_val(&a, x.as_mut());
        check_normalized2(x.as_ref(), "read_sv_bit_vec_val")?;
        let mut out = vec![0u32; a.len()];
        write_sv_bit_vec_val(x.as_ref(), &mut out);
        let mut want = a.clone();
        if let Some(top) = want.last_mut() {
            *top &= top_mask32(w);
        }
        prop_assert_eq!(out, want);
    }

    /// TC51.3: 2 値の u64 ワード列の LE バイト列の先頭 `4·ceil(w/32)` バイトが、
    /// `write_sv_bit_vec_val` の出力の LE バイト列と一致する (input 引数のゼロコピーの根拠、plan.md §3.2)。
    #[test]
    fn sv_bit_zero_copy_equivalence(b in any_bit_vec()) {
        let w = b.width();
        let mut buf = vec![0u32; sv_bit_vec_val_len(w)];
        write_sv_bit_vec_val(b.as_ref(), &mut buf);
        let from_u64: Vec<u8> = b.as_ref().words().iter().flat_map(|x| x.to_le_bytes()).collect();
        let from_u32: Vec<u8> = buf.iter().flat_map(|x| x.to_le_bytes()).collect();
        prop_assert!(from_u32.len() <= from_u64.len());
        prop_assert_eq!(&from_u64[..from_u32.len()], &from_u32[..]);
    }

    /// TC51.4: `write_sv_logic_vec_val` の要素 `2k` / `2k+1` が、参照モデルのビット `32k..32k+31` の
    /// aval / bval と一致する。
    #[test]
    fn sv_logic_layout_matches_ref(x in any_logic_vec()) {
        let buf = sv_logic(&x);
        let r = to_ref(&x);
        for k in 0..sv_bit_vec_val_len(x.width()) {
            let (a, b) = ref_sv_logic_elem(&r, k);
            prop_assert_eq!(buf[2 * k], a, "aval of element {}", k);
            prop_assert_eq!(buf[2 * k + 1], b, "bval of element {}", k);
        }
    }
}

// ===========================================================================
// TC56: proptest の入力分布 (tests/common/mod.rs の戦略)
// ===========================================================================

fn sample<S: Strategy>(runner: &mut TestRunner, s: &S) -> S::Value {
    s.new_tree(runner).expect("strategy").current()
}

/// TC56: 固定シードで 10,000 個を生成し、境界が出ていることを確かめる。
#[test]
fn strategy_distribution() {
    let mut runner = TestRunner::deterministic();
    const N: usize = 10_000;

    // 幅
    let ws = width();
    let mut seen = [false; BOUNDARY_WIDTHS.len()];
    let mut wide = 0usize;
    for _ in 0..N {
        let w = sample(&mut runner, &ws);
        assert!((1..=1024).contains(&w));
        if let Some(i) = BOUNDARY_WIDTHS.iter().position(|&b| b == w) {
            seen[i] = true;
        }
        if w >= 201 {
            wide += 1;
        }
    }
    for (i, s) in seen.iter().enumerate() {
        assert!(s, "boundary width {} never generated", BOUNDARY_WIDTHS[i]);
    }
    let ratio = wide as f64 / N as f64;
    assert!((0.10..=0.30).contains(&ratio), "201..=1024 ratio {ratio}");

    // X/Z の密度
    let vs = any_logic_vec();
    let (mut none, mut single, mut all, mut has_x, mut has_z) = (false, false, false, false, false);
    for _ in 0..N {
        let v = sample(&mut runner, &vs);
        let r = v.as_ref();
        let unk: u32 = r.bval().iter().map(|w| w.count_ones()).sum();
        none |= unk == 0;
        single |= unk == 1;
        all |= unk == v.width();
        let s = v.to_bits_string();
        has_x |= s.contains('x');
        has_z |= s.contains('z');
    }
    assert!(
        none && single && all,
        "density: none {none} single {single} all {all}"
    );
    assert!(has_x && has_z, "x {has_x} z {has_z}");

    // ワードの模様 (幅 128 の 2 値)
    let bs = bit_vec(128);
    let (mut zero, mut ones, mut sign) = (false, false, false);
    for _ in 0..N {
        let b = sample(&mut runner, &bs);
        for &w in b.as_ref().words() {
            zero |= w == 0;
            ones |= w == u64::MAX;
            sign |= w == 1 << 63;
        }
    }
    assert!(
        zero && ones && sign,
        "words: zero {zero} ones {ones} sign {sign}"
    );

    // シフト量 (データ幅 100)
    let w = 100u64;
    let ss = shift_amount(w as u32);
    let (mut s0, mut sw, mut sw1, mut big, mut sx) = (false, false, false, false, false);
    for _ in 0..N {
        let a = sample(&mut runner, &ss);
        if a.has_unknown() {
            sx = true;
            continue;
        }
        match a.to_u64() {
            Some(0) => s0 = true,
            Some(v) if v == w => sw = true,
            Some(v) if v == w + 1 => sw1 = true,
            Some(v) if v > 64 => big |= v > u32::MAX as u64,
            None => big = true,
            _ => {}
        }
    }
    assert!(
        s0 && sw && sw1 && big && sx,
        "shift: 0 {s0} w {sw} w+1 {sw1} big {big} x {sx}"
    );

    // 選択のオフセット
    let os = select_offset(100, 8);
    let (mut neg, mut min, mut max, mut unk) = (false, false, false, false);
    for _ in 0..N {
        match sample(&mut runner, &os) {
            None => unk = true,
            Some(i64::MIN) => min = true,
            Some(i64::MAX) => max = true,
            Some(v) if v < 0 => neg = true,
            _ => {}
        }
    }
    assert!(
        neg && min && max && unk,
        "offset: neg {neg} min {min} max {max} unknown {unk}"
    );
}

// ===========================================================================
// TC57: panic の規約 (API の誤用は panic、release ビルドでも)
// ===========================================================================

#[test]
#[should_panic(expected = "LogicRef::from_planar: planar length must be 2*words_for(65) (got 3)")]
fn from_planar_rejects_len_3() {
    let w = [0u64; 3];
    let _ = LogicRef::from_planar(65, &w);
}

#[test]
#[should_panic(expected = "LogicMut::from_planar: planar length must be 2*words_for(65) (got 5)")]
fn from_planar_rejects_len_5() {
    let mut w = [0u64; 5];
    let _ = LogicMut::from_planar(65, &mut w);
}

#[test]
#[should_panic(expected = "LogicVec::bit: bit index 129 out of range for width 129")]
fn bit_rejects_out_of_range() {
    let _ = LogicVec::zero(129).bit(129);
}

#[test]
#[should_panic(expected = "LogicVec::set_bit: bit index 129 out of range for width 129")]
fn set_bit_rejects_out_of_range() {
    LogicVec::zero(129).set_bit(129, Logic::L1);
}

#[test]
#[should_panic(expected = "BitMut::set_bit: bit index 129 out of range for width 129")]
fn bit2_rejects_out_of_range() {
    let mut a = [0u64; 3];
    BitMut::new(129, &mut a).set_bit(129, true);
}

#[test]
#[should_panic(expected = "LogicMut::copy_from: width mismatch (dst 9, src 8)")]
fn copy_from_rejects_width_mismatch() {
    let src = LogicVec::zero(8);
    let mut dst = LogicVec::zero(9);
    dst.as_mut().copy_from(src.as_ref());
}

#[test]
#[should_panic(expected = "BitMut::copy_from: width mismatch (dst 9, src 8)")]
fn copy_from2_rejects_width_mismatch() {
    let src = BitVec::zero(8);
    let mut dst = BitVec::zero(9);
    dst.as_mut().copy_from(src.as_ref());
}

#[test]
#[should_panic(expected = "BitRef::new: plane length must be words_for(65) (got 1)")]
fn bit_ref_new_rejects_bad_length() {
    let _ = BitRef::new(65, &[0u64]);
}

#[test]
#[should_panic(expected = "LogicMut::new: plane length must be words_for(65) (got 3)")]
fn logic_mut_new_rejects_bad_length() {
    let (mut a, mut b) = ([0u64; 2], [0u64; 3]);
    let _ = LogicMut::new(65, &mut a, &mut b);
}

#[test]
#[should_panic(expected = "BitVec::bit: bit index 8 out of range for width 8")]
fn bit_vec_bit_rejects_out_of_range() {
    let _ = BitVec::zero(8).bit(8);
}

#[test]
#[should_panic(expected = "LogicVec::zero: width")]
fn logic_vec_zero_rejects_too_wide() {
    let _ = LogicVec::zero(svsim_logic::MAX_WIDTH + 1);
}

#[test]
#[should_panic(
    expected = "read_sv_logic_vec_val: src length (u32 count) must be 4 for width 33, got 2"
)]
fn read_sv_logic_rejects_bad_length() {
    let mut x = LogicVec::zero(33);
    read_sv_logic_vec_val(&[0, 0], x.as_mut());
}

#[test]
#[should_panic(
    expected = "write_sv_bit_vec_val: dst length (u32 count) must be 2 for width 33, got 3"
)]
fn write_sv_bit_rejects_bad_length() {
    let b = BitVec::zero(33);
    write_sv_bit_vec_val(b.as_ref(), &mut [0; 3]);
}

// ===========================================================================
// TC58: 決定性 (中核の範囲)
// ===========================================================================

/// 1 組の入力に対する中核の操作の結果 (bits 文字列の列)。
fn core_ops(seed: u64) -> Vec<String> {
    let mut rng = SplitMix64(seed);
    let w = 1 + (rng.next_u64() % 300) as u32;
    let a: Vec<u64> = (0..words_for(w)).map(|_| rng.next_u64()).collect();
    let mut a = a;
    if let Some(t) = a.last_mut() {
        *t &= svsim_logic::top_mask(w);
    }
    let x = inject_unknowns(w, &a, UnknownDensity::Half, XzRatio::Mixed, rng.next_u64());
    let mut out = vec![x.to_bits_string()];
    let i = (rng.next_u64() % w as u64) as u32;
    let mut y = x.clone();
    y.set_bit(i, ALL_LOGIC[(rng.next_u64() % 4) as usize]);
    out.push(y.to_bits_string());
    out.push(x.to_bit_vec().to_bits_string());
    let buf = sv_logic(&x);
    out.push(format!("{buf:x?}"));
    let mut back = LogicVec::zero(w);
    read_sv_logic_vec_val(&buf, back.as_mut());
    out.push(back.to_bits_string());
    out.push(format!("{:?}", x.to_u64()));
    out.push(LogicVec::from_i64(w, rng.next_u64() as i64).to_bits_string());
    out
}

/// TC58: 1 スレッドで順に実行した結果と、8 スレッドに分けて実行した結果が完全に一致する。
#[test]
fn deterministic_across_threads() {
    const N: u64 = 1000;
    let serial: Vec<Vec<String>> = (0..N).map(core_ops).collect();
    let mut parallel: Vec<Vec<String>> = vec![Vec::new(); N as usize];
    std::thread::scope(|s| {
        for (t, chunk) in parallel.chunks_mut((N / 8) as usize).enumerate() {
            s.spawn(move || {
                for (j, slot) in chunk.iter_mut().enumerate() {
                    *slot = core_ops((t * (N as usize / 8) + j) as u64);
                }
            });
        }
    });
    assert_eq!(serial, parallel);
}

// ===========================================================================
// tests/common の比較補助の自己テスト
// ===========================================================================

#[test]
fn common_helpers_self_test() {
    for &v in &ALL_LOGIC {
        let r = logic_to_ref(v);
        assert_eq!(logic_from_ref(r), v);
        assert!(check_logic_eq_reflogic(v, r, "same").is_ok());
        assert_eq!(ref_to_two_state_bool(r), v == Logic::L1);
    }
    assert!(check_logic_eq_reflogic(Logic::X, RefLogic::Z, "x vs z").is_err());
    assert!(check_bool_eq_ref(true, true, "same").is_ok());
    assert!(check_bool_eq_ref(true, false, "diff").is_err());

    // 本体の Edge 型 (M0-T4) と同じ名前のバリアントを持つ型で、RefEdge と比べられる。
    #[derive(Debug)]
    enum LocalEdge {
        None,
        Pos,
        Neg,
    }
    assert!(check_edge_eq_ref(LocalEdge::Pos, RefEdge::Pos, "pos").is_ok());
    assert!(check_edge_eq_ref(LocalEdge::Neg, RefEdge::Neg, "neg").is_ok());
    assert!(check_edge_eq_ref(LocalEdge::None, RefEdge::None, "none").is_ok());
    assert!(check_edge_eq_ref(LocalEdge::Pos, RefEdge::Neg, "diff").is_err());
    assert!(check_value_eq(3, 3, "same").is_ok());
    assert!(check_value_eq(3, 4, "diff").is_err());
}
