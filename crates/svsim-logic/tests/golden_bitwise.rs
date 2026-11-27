//! M0-T4: ビット演算・リダクション・論理・比較・`?:`・エッジのゴールデンテスト (固定値)。
//!
//! 期待値は IEEE 1800-2023 の表と例、および test-plan.md の M0-TC12〜TC21 の表。
//! 暫定期待値には `[要VCS確認 V-n]` を付けた (plan.md §11)。

use svsim_logic::ops::edge::{Edge, edge, edge_lsb, edge2};
use svsim_logic::ops::reduce::LogicSet;
use svsim_logic::ops::{bitwise, compare, cond, edge as edge_mod, logical};
use svsim_logic::{BitVec, L64, Logic, LogicVec};

use Logic::{L0 as O, L1 as I, X, Z};

fn v(s: &str) -> LogicVec {
    LogicVec::from_bits_str(s).unwrap()
}

fn b(s: &str) -> BitVec {
    BitVec::from_bits_str(s).unwrap()
}

fn lv(s: &str) -> Logic {
    Logic::from_char(s.chars().next().unwrap()).unwrap()
}

/// 幅 `w` の全 `fill` で、指定ビットだけ `val` にした値。
fn with_bit(w: u32, fill: Logic, i: u32, val: Logic) -> LogicVec {
    let mut x = LogicVec::filled(w, fill);
    x.set_bit(i, val);
    x
}

/// 決まった 0/1 の模様 (幅 `w`)。
fn pattern(w: u32) -> LogicVec {
    let mut x = LogicVec::zero(w);
    for i in 0..w {
        if (i * 7 + 3) % 5 < 2 {
            x.set_bit(i, I);
        }
    }
    x
}

// ===========================================================================
// M0-TC13 ビット演算 (§11.4.8)
// ===========================================================================

/// IEEE 1800-2023 §11.4.8 の表 (1 ビット × 16 通り)。z の入力は x と同じ行で、結果に z は出ない。
#[test]
fn tc13_tables_11_4_8() {
    let order = [O, I, X, Z];
    let and_t = [[O, O, O, O], [O, I, X, X], [O, X, X, X], [O, X, X, X]];
    let or_t = [[O, I, X, X], [I, I, I, I], [X, I, X, X], [X, I, X, X]];
    let xor_t = [[O, I, X, X], [I, O, X, X], [X, X, X, X], [X, X, X, X]];
    let xnor_t = [[I, O, X, X], [O, I, X, X], [X, X, X, X], [X, X, X, X]];
    let not_t = [I, O, X, X];
    for (i, &x) in order.iter().enumerate() {
        let xv = LogicVec::from(x);
        assert_eq!(xv.not().bit(0), not_t[i], "~{x}");
        assert_eq!(bitwise::not_l64(L64::filled(x, 1), 1).bit(0), not_t[i]);
        for (j, &y) in order.iter().enumerate() {
            let yv = LogicVec::from(y);
            assert_eq!(xv.and(&yv).bit(0), and_t[i][j], "{x}&{y}");
            assert_eq!(xv.or(&yv).bit(0), or_t[i][j], "{x}|{y}");
            assert_eq!(xv.xor(&yv).bit(0), xor_t[i][j], "{x}^{y}");
            assert_eq!(xv.xnor(&yv).bit(0), xnor_t[i][j], "{x}~^{y}");
        }
    }
}

/// §11.4.8 のベクタ例 (test-plan M0-TC13)。
#[test]
fn tc13_vectors_11_4_8() {
    let x = v("0101xz10");
    let y = v("00110zz1");
    assert_eq!(x.and(&y).to_bits_string(), "00010xx0");
    assert_eq!(x.or(&y).to_bits_string(), "0111xx11"); // bit 1 は 1 | z = 1
    assert_eq!(x.xor(&y).to_bits_string(), "0110xxx1");

    let p = BitVec::from_u64(8, 0xf0);
    let q = BitVec::from_u64(8, 0x3c);
    assert_eq!(p.and(&q).to_u64(), Some(0x30));
    assert_eq!(p.or(&q).to_u64(), Some(0xfc));
    assert_eq!(p.xor(&q).to_u64(), Some(0xcc));
    assert_eq!(p.xnor(&q).to_u64(), Some(0x33));
    assert_eq!(p.not().to_u64(), Some(0x0f));
}

// ===========================================================================
// M0-TC12 正規化不変条件 (違反しやすい演算の固定テスト)
// ===========================================================================

/// plan.md §3.2: `not`、`xnor`、`cond_merge` (x の生成) の出力が正規化されている。
#[test]
fn tc12_normalized_outputs_bitwise() {
    for w in [1, 63, 65, 127, 129] {
        let z = LogicVec::zero(w);
        let n = z.not();
        assert!(n.as_ref().is_normalized(), "not w={w}");
        assert_eq!(n, LogicVec::filled(w, I));
        let xn = z.xnor(&z);
        assert!(xn.as_ref().is_normalized(), "xnor w={w}");
        assert_eq!(xn, LogicVec::filled(w, I));
        let m = z.cond_merge(&LogicVec::filled(w, I));
        assert!(m.as_ref().is_normalized(), "cond_merge w={w}");
        assert_eq!(m, LogicVec::filled(w, X));
        let mb = BitVec::zero(w).cond_merge_logic(&BitVec::filled(w, true));
        assert!(mb.as_ref().is_normalized(), "cond_merge_bits w={w}");
        let mut a = z.clone();
        a.not_assign();
        assert!(a.as_ref().is_normalized(), "not_assign w={w}");
        let nb = BitVec::zero(w).not();
        assert!(nb.as_ref().is_normalized(), "not2 w={w}");
        assert_eq!(nb, BitVec::filled(w, true));
        assert!(
            BitVec::zero(w)
                .xnor(&BitVec::zero(w))
                .as_ref()
                .is_normalized()
        );
    }
    // 層 1
    assert_eq!(bitwise::not_l64(L64::ZERO, 63), L64::new(u64::MAX >> 1, 0));
    assert_eq!(bitwise::xnor_l64(L64::ZERO, L64::ZERO, 1), L64::new(1, 0));
    assert_eq!(bitwise::not2_l64(0, 7), 0x7f);
}

// ===========================================================================
// M0-TC14 リダクションと $countbits 系 (§11.4.9, §20.9)
// ===========================================================================

/// IEEE 1800-2023 §11.4.9 のリダクションの例と test-plan M0-TC14 の表。列は `& ~& | ~| ^ ~^`。
#[test]
fn tc14_reductions_11_4_9() {
    let rows: &[(LogicVec, [Logic; 6])] = &[
        (v("1111"), [I, O, I, O, O, I]),
        (v("0000"), [O, I, O, I, O, I]),
        (v("1011"), [O, I, I, O, I, O]),
        (v("1x11"), [X, X, I, O, X, X]),
        (v("0x11"), [O, I, I, O, X, X]),
        (v("000x"), [O, I, X, X, X, X]),
        (v("z111"), [X, X, I, O, X, X]),
        (v("000z"), [O, I, X, X, X, X]),
        (v("1"), [I, O, I, O, I, O]),
        (v("z"), [X, X, X, X, X, X]),
        (LogicVec::filled(129, I), [I, O, I, O, I, O]),
        (LogicVec::filled(128, I), [I, O, I, O, O, I]),
        (with_bit(129, I, 128, O), [O, I, I, O, O, I]),
        (with_bit(129, I, 128, X), [X, X, I, O, X, X]),
        (with_bit(129, O, 128, I), [O, I, I, O, I, O]),
    ];
    for (x, want) in rows {
        let got = [
            x.reduce_and(),
            x.reduce_nand(),
            x.reduce_or(),
            x.reduce_nor(),
            x.reduce_xor(),
            x.reduce_xnor(),
        ];
        assert_eq!(&got, want, "{x:?}");
        if !x.has_unknown() {
            let bx = x.to_bit_vec();
            let got2 = [
                bx.reduce_and(),
                bx.reduce_nand(),
                bx.reduce_or(),
                bx.reduce_nor(),
                bx.reduce_xor(),
                bx.reduce_xnor(),
            ]
            .map(Logic::from);
            assert_eq!(&got2, want, "2-state {x:?}");
        }
    }
}

/// IEEE 1800-2023 §20.9 の `$countbits` / `$countones` / `$onehot` / `$onehot0` / `$isunknown`。
#[test]
fn tc14_countbits_20_9() {
    let x = v("10xz01xz");
    assert_eq!(x.count_bits(LogicSet::of(&[I])), 2);
    assert_eq!(x.count_bits(LogicSet::of(&[O])), 2);
    assert_eq!(x.count_bits(LogicSet::of(&[X])), 2);
    assert_eq!(x.count_bits(LogicSet::of(&[Z])), 2);
    assert_eq!(x.count_bits(LogicSet::of(&[X, Z])), 4);
    assert_eq!(x.count_bits(LogicSet::of(&[O, I, X, Z])), 8);
    // 空の集合は 0 (決定 Q-2)
    assert_eq!(x.count_bits(LogicSet::EMPTY), 0);
    assert_eq!(x.count_ones(), 2);

    assert!(v("00010000").onehot());
    assert!(!v("00000000").onehot());
    assert!(!v("00010001").onehot());
    assert!(v("0001x000").onehot()); // $countbits(e,'1) == 1 の定義どおり
    assert!(v("00000000").onehot0());
    assert!(v("00000001").onehot0());
    assert!(!v("00000011").onehot0());
    assert!(v("0000000z").is_unknown());
    assert!(!v("00000000").is_unknown());
    assert!(with_bit(129, O, 128, I).onehot());

    let bx = b("10110000");
    assert_eq!(bx.count_bits(LogicSet::ZERO), 5);
    assert_eq!(bx.count_bits(LogicSet::UNKNOWN), 0);
    assert_eq!(bx.count_ones(), 3);
    assert!(!bx.onehot());
}

// ===========================================================================
// M0-TC15 真理値と論理演算 (§11.4.7, §12.4)
// ===========================================================================

/// IEEE 1800-2023 §12.4 の真理値、§11.4.7 の `!`。
#[test]
fn tc15_truth_12_4() {
    assert_eq!(v("0000").truth(), O);
    assert_eq!(v("0100").truth(), I);
    assert_eq!(v("000x").truth(), X);
    assert_eq!(v("0z00").truth(), X);
    assert_eq!(v("100x").truth(), I);
    assert_eq!(with_bit(129, O, 128, I).truth(), I);
    assert_eq!(v("000x").logical_not(), X);
    assert_eq!(v("0100").logical_not(), O);
    assert_eq!(v("0000").logical_not(), I);
    assert_eq!(
        logical::truth_l64(L64::load(v("0z00").as_ref()), 4),
        X,
        "layer 1"
    );
    assert!(b("0100").truth());
    assert!(b("0000").logical_not());
}

/// IEEE 1800-2023 §11.4.7 の `&& || -> <->` (オペランドは真理値)。
#[test]
fn tc15_logical_11_4_7() {
    // (a, b, &&, ||, ->, <->)
    let t = [
        ("0", "0", "0", "0", "1", "1"),
        ("0", "1", "0", "1", "1", "0"),
        ("0", "x", "0", "x", "1", "x"),
        ("1", "0", "0", "1", "0", "0"),
        ("1", "1", "1", "1", "1", "1"),
        ("1", "x", "x", "1", "x", "x"),
        ("x", "0", "0", "x", "x", "x"),
        ("x", "1", "x", "1", "1", "x"),
        ("x", "x", "x", "x", "x", "x"),
    ];
    for (a, bb, and, or, imp, eqv) in t {
        let (a, bb) = (lv(a), lv(bb));
        assert_eq!(a.and(bb), lv(and), "{a}&&{bb}");
        assert_eq!(a.or(bb), lv(or), "{a}||{bb}");
        assert_eq!(a.implies(bb), lv(imp), "{a}->{bb}");
        assert_eq!(a.equiv(bb), lv(eqv), "{a}<->{bb}");
    }
    // z は truth を通ると x
    assert_eq!(v("z").truth().and(v("1").truth()), X);
}

// ===========================================================================
// M0-TC16 論理等価・ケース等価 (§11.4.5)
// ===========================================================================

/// IEEE 1800-2023 §11.4.5。列は `== != === !==`。
#[test]
fn tc16_equality_11_4_5() {
    let rows = [
        ("1010", "1010", I, O, true),
        ("1010", "1011", O, I, false),
        ("1x00", "0x00", O, I, false), // [要VCS確認 V-15] 暫定: 既知ビットの不一致で 0
        ("1x00", "1000", X, X, false), // [要VCS確認 V-15] 暫定
        ("1z00", "1z00", X, X, true),
        ("1z00", "1x00", X, X, false),
        ("xxxx", "xxxx", X, X, true),
    ];
    for (x, y, eq, ne, ceq) in rows {
        let (x, y) = (v(x), v(y));
        assert_eq!(x.eq(&y), eq, "{x:?}=={y:?}");
        assert_eq!(x.ne(&y), ne, "{x:?}!={y:?}");
        assert_eq!(x.case_eq(&y), ceq, "{x:?}==={y:?}");
        assert_eq!(x.case_ne(&y), !ceq, "{x:?}!=={y:?}");
    }
    // 129 ビット: 同値で bit 128 だけ異なる
    let p = pattern(129);
    let mut q = p.clone();
    q.set_bit(128, if p.bit(128) == I { O } else { I });
    assert_eq!(p.eq(&q), O);
    assert_eq!(p.ne(&q), I);
    assert!(!p.case_eq(&q));
    assert!(p.case_ne(&q));
    // 129 ビット: bit 128 が x (両方)、他の既知ビットで 1 か所不一致 [要VCS確認 V-15]
    let mut p = pattern(129);
    p.set_bit(128, X);
    let mut q = p.clone();
    q.set_bit(5, if p.bit(5) == I { O } else { I });
    assert_eq!(p.eq(&q), O);
    assert_eq!(p.ne(&q), I);
    assert!(!p.case_eq(&q));
    assert!(p.case_ne(&q));
    // 2 値
    assert!(b("1010").eq(&b("1010")));
    assert!(b("1010").ne(&b("1011")));
}

// ===========================================================================
// M0-TC17 ワイルドカード等価 (§11.4.6)
// ===========================================================================

/// IEEE 1800-2023 §11.4.6 [要VCS確認 V-16] 暫定 (表全体)。
#[test]
fn tc17_wildcard_11_4_6() {
    let rows = [
        ("1010", "1x1z", I),
        ("1x10", "1x1z", I), // 左の x はワイルドカード位置
        ("1x10", "1010", X),
        ("0x10", "1010", O),
        ("1010", "10zz", I),
        ("1z10", "1010", X),
        ("1x1z", "1010", X), // 左右を入れ替えると対称でない
        ("0000", "xxxx", I),
    ];
    for (x, y, want) in rows {
        let (x, y) = (v(x), v(y));
        assert_eq!(x.wildcard_eq(&y), want, "{x:?}==?{y:?}");
        assert_eq!(x.wildcard_ne(&y), want.not(), "{x:?}!=?{y:?}");
        let (lx, ly) = (L64::load(x.as_ref()), L64::load(y.as_ref()));
        assert_eq!(compare::wildcard_eq_l64(lx, ly, 4), want, "layer 1");
    }
}

// ===========================================================================
// M0-TC18 casez / casex の照合 (§12.5.1)
// ===========================================================================

/// IEEE 1800-2023 §12.5.1 [要VCS確認 V-17] 暫定 (表全体)。
#[test]
fn tc18_casez_casex_12_5_1() {
    let rows = [
        ("1z01", "1101", true, true),
        ("1101", "1z01", true, true),
        ("1x01", "1101", false, true),
        ("1x01", "1x01", true, true),
        ("0x01", "1101", false, false),
        ("0000", "zzzz", true, true),
        ("0000", "xxxx", false, true),
        ("1100", "1100", true, true),
    ];
    for (x, y, z, xx) in rows {
        let (x, y) = (v(x), v(y));
        assert_eq!(x.casez_match(&y), z, "casez {x:?} {y:?}");
        assert_eq!(x.casex_match(&y), xx, "casex {x:?} {y:?}");
        let (lx, ly) = (L64::load(x.as_ref()), L64::load(y.as_ref()));
        assert_eq!(compare::casez_match_l64(lx, ly, 4), z, "layer 1");
        assert_eq!(compare::casex_match_l64(lx, ly, 4), xx, "layer 1");
    }
}

// ===========================================================================
// M0-TC19 関係演算 (§11.4.4, §11.8.1)
// ===========================================================================

fn rel(x: &LogicVec, y: &LogicVec, s: bool) -> [Logic; 4] {
    [x.lt(y, s), x.le(y, s), x.gt(y, s), x.ge(y, s)]
}

/// IEEE 1800-2023 §11.4.4。符号付きで比べるのは両方が符号付きのとき (§11.8.1、`signed` 引数)。
#[test]
fn tc19_relational_11_4_4() {
    let h = |w: u32, n: u64| LogicVec::from_u64(w, n);
    assert_eq!(rel(&h(8, 0x80), &h(8, 0x7f), true), [I, I, O, O]);
    assert_eq!(rel(&h(8, 0x80), &h(8, 0x7f), false), [O, O, I, I]);
    assert_eq!(rel(&h(8, 0xff), &h(8, 0x00), true), [I, I, O, O]);
    for s in [false, true] {
        assert_eq!(rel(&h(8, 5), &h(8, 5), s), [O, I, O, I]);
    }
    assert_eq!(rel(&v("0001"), &v("001x"), false), [X, X, X, X]);
    assert_eq!(rel(&v("z000"), &v("0000"), true), [X, X, X, X]);

    // 65 ビット: x = 2^64, y = 2^64 - 1 (符号なし)
    let x = with_bit(65, O, 64, I);
    let y = h(65, u64::MAX);
    assert_eq!(rel(&x, &y, false), [O, O, I, I]);
    // 65 ビット: x = -2^64 (bit 64 だけ 1), y = 0 (符号付き)
    assert_eq!(rel(&x, &h(65, 0), true), [I, I, O, O]);
    // 128 ビット: x = 2^127, y = 1
    let x = with_bit(128, O, 127, I);
    assert_eq!(rel(&x, &h(128, 1), true), [I, I, O, O]);
    assert_eq!(rel(&x, &h(128, 1), false), [O, O, I, I]);

    // 2 値
    let p = BitVec::from_u64(8, 0x80);
    let q = BitVec::from_u64(8, 0x7f);
    assert!(p.lt(&q, true));
    assert!(!p.lt(&q, false));
    assert!(compare::lt2_l64(0x80, 0x7f, 8, true));
    assert!(compare::ge2_l64(0x80, 0x7f, 8, false));
}

// ===========================================================================
// M0-TC20 条件演算子の X マージ (§11.4.11)
// ===========================================================================

/// IEEE 1800-2023 §11.4.11。両方 0 → 0、両方 1 → 1、それ以外 → x。
#[test]
fn tc20_cond_merge_11_4_11() {
    assert_eq!(v("0110").cond_merge(&v("0101")).to_bits_string(), "01xx");
    // x,x → x、z,z → x [要VCS確認 V-8] 暫定
    assert_eq!(v("1x0z").cond_merge(&v("1x0z")).to_bits_string(), "1x0x");
    // [要VCS確認 V-8] 暫定: 1'bx ? 1'bz : 1'bz は x (Icarus は z。既知の差)
    assert_eq!(v("z").cond_merge(&v("z")).to_bits_string(), "x");
    assert_eq!(v("z").cond_merge(&v("0")).to_bits_string(), "x");
    assert_eq!(v("1").cond_merge(&v("z")).to_bits_string(), "x");

    // 129 ビット
    let p = pattern(129);
    assert_eq!(p.cond_merge(&p), p);
    let z = LogicVec::zero(129);
    let f = with_bit(129, O, 128, I);
    assert_eq!(z.cond_merge(&f), with_bit(129, O, 128, X));

    // 層 1 (plan.md §3.4 の式)
    let l = cond::cond_merge_l64(L64::new(0b0110, 0), L64::new(0b0101, 0), 4);
    assert_eq!(l, L64::load(v("01xx").as_ref()));

    // 条件の判定は truth で行う (評価器の責務)
    assert_eq!(v("z").truth(), X); // → マージ
    assert_eq!(v("01x0").truth(), I); // → t を選ぶ
}

/// 混在カーネル `cond_merge_bits` (plan.md §3.5.1、§11.4.11)。結果は 4 値。
#[test]
fn tc20_cond_merge_bits_3_5_1() {
    let t = b("11001010");
    let f = b("10101010");
    let m = t.cond_merge_logic(&f);
    assert_eq!(m.to_bits_string(), "1xx01010");
    // 2 値の変数に代入すると x は 0 になる (§6.11.2)
    assert_eq!(m.to_bit_vec().to_bits_string(), "10001010");
    let mut d = LogicVec::zero(8);
    cond::cond_merge_bits(d.as_mut(), t.as_ref(), f.as_ref());
    assert_eq!(d, m);
    assert_eq!(
        cond::cond_merge_bits_l64(0b1100_1010, 0b1010_1010, 8),
        L64::load(m.as_ref())
    );
}

// ===========================================================================
// M0-TC21 エッジ検出 (§9.4.2) — 4×4 の全 16 通り
// ===========================================================================

/// IEEE 1800-2023 §9.4.2 の遷移表 (16 通り)。[要VCS確認 V-12] 暫定 (x↔z、z→1、1→z を含む表全体)。
#[test]
fn tc21_edge_table_9_4_2_all_16() {
    use Edge::{Neg as N, None as E, Pos as P};
    let table = [
        (O, O, E), // 1
        (O, I, P), // 2
        (O, X, P), // 3
        (O, Z, P), // 4
        (I, O, N), // 5
        (I, I, E), // 6
        (I, X, N), // 7
        (I, Z, N), // 8 [要VCS確認 V-12]
        (X, O, N), // 9
        (X, I, P), // 10
        (X, X, E), // 11
        (X, Z, E), // 12 [要VCS確認 V-12]
        (Z, O, N), // 13
        (Z, I, P), // 14 [要VCS確認 V-12]
        (Z, X, E), // 15 [要VCS確認 V-12]
        (Z, Z, E), // 16
    ];
    assert_eq!(table.len(), 16);
    for (old, new, want) in table {
        assert_eq!(edge(old, new), want, "{old}->{new}");
        // 層 1 (scalar 版)
        let got1 = edge_mod::edge_l64(L64::filled(old, 1), L64::filled(new, 1));
        assert_eq!(got1, want, "l64 {old}->{new}");
        // 層 2 (幅 1)
        let (o, n) = (LogicVec::from(old), LogicVec::from(new));
        assert_eq!(edge_lsb(o.as_ref(), n.as_ref()), want, "lsb {old}->{new}");
        // 上位ビットに別の値があっても LSB だけを見る
        let o64 = L64::new(old.aval() as u64 | 0xF0, old.bval() as u64 | 0x0C);
        let n64 = L64::new(new.aval() as u64 | 0x30, new.bval() as u64 | 0xA0);
        assert_eq!(edge_mod::edge_l64(o64, n64), want, "l64 upper {old}->{new}");
    }
    // 性質: edge(a, a) は常に None。Pos と Neg は排他 (列挙型なので自明)。
    for a in Logic::ALL {
        assert_eq!(edge(a, a), E);
    }
}

/// 2 値のエッジ (§9.4.2)。
#[test]
fn tc21_edge2_9_4_2() {
    assert_eq!(edge2(false, true), Edge::Pos);
    assert_eq!(edge2(true, false), Edge::Neg);
    assert_eq!(edge2(false, false), Edge::None);
    assert_eq!(edge2(true, true), Edge::None);
    assert_eq!(edge_mod::edge2_l64(0b10, 0b01), Edge::Pos);
    assert_eq!(edge_mod::edge2_l64(0b01, 0b11), Edge::None);
    assert_eq!(
        edge_mod::edge_lsb2(b("0001").as_ref(), b("1110").as_ref()),
        Edge::Neg
    );
}

/// 多ビットの式は LSB の遷移だけを見る (§9.4.2) [要VCS確認 V-12]。
#[test]
fn tc21_edge_lsb_9_4_2() {
    let e = |o: &LogicVec, n: &LogicVec| edge_lsb(o.as_ref(), n.as_ref());
    assert_eq!(e(&v("0000"), &v("1110")), Edge::None);
    assert_eq!(e(&v("1110"), &v("0001")), Edge::Pos);
    assert_eq!(e(&v("xxxxxxx1"), &v("00000000")), Edge::Neg);
    // 幅 129 で bit 0 だけ 0→x、他は全部変化 → Pos
    let old = LogicVec::zero(129);
    let mut new = LogicVec::filled(129, I);
    new.set_bit(0, X);
    assert_eq!(e(&old, &new), Edge::Pos);
    assert!(Edge::Pos.is_edge() && Edge::Neg.is_edge() && !Edge::None.is_edge());
}

// ===========================================================================
// M0-TC57 panic の規約 (幅の不一致は関数名と幅を含むメッセージで panic)
// ===========================================================================

#[test]
#[should_panic(expected = "eq: width mismatch (64 vs 65)")]
fn tc57_eq_width_mismatch_panics() {
    let _ = LogicVec::zero(64).eq(&LogicVec::zero(65));
}

#[test]
#[should_panic(expected = "cond_merge: width mismatch (8 vs 9)")]
fn tc57_cond_merge_width_mismatch_panics() {
    let _ = LogicVec::zero(8).cond_merge(&LogicVec::zero(9));
}

#[test]
#[should_panic(expected = "and2: width mismatch (8 vs 9)")]
fn tc57_and2_width_mismatch_panics() {
    let _ = BitVec::zero(8).and(&BitVec::zero(9));
}

#[test]
#[should_panic(expected = "lt: width mismatch (8 vs 9)")]
fn tc57_lt_width_mismatch_panics() {
    let _ = LogicVec::zero(8).lt(&LogicVec::zero(9), true);
}
