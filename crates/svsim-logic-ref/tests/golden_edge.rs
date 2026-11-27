//! 参照モデルのゴールデンテスト: エッジ検出 (IEEE 1800-2023 §9.4.2) [要VCS確認 V-12]。

use svsim_logic_ref::*;

use RefLogic::{One, X, Z, Zero};

// §9.4.2 の遷移表: posedge は 0→1, 0→x, 0→z, x→1, z→1。negedge は 1→0, 1→x, 1→z, x→0, z→0。
#[test]
fn s9_4_2_edge_table() {
    let pos = [(Zero, One), (Zero, X), (Zero, Z), (X, One), (Z, One)];
    let neg = [(One, Zero), (One, X), (One, Z), (X, Zero), (Z, Zero)];
    for old in RefLogic::ALL {
        for new in RefLogic::ALL {
            let expected = if pos.contains(&(old, new)) {
                RefEdge::Pos
            } else if neg.contains(&(old, new)) {
                RefEdge::Neg
            } else {
                RefEdge::None
            };
            assert_eq!(edge(old, new), expected, "{old:?} -> {new:?}");
        }
    }
    // x↔z はエッジではない
    assert_eq!(edge(X, Z), RefEdge::None);
    assert_eq!(edge(Z, X), RefEdge::None);
}

// §9.4.2: 多ビットの式では LSB の遷移だけを見る。
#[test]
fn s9_4_2_edge_lsb() {
    let v = |s: &str| RefVec::from_bits_str(s).unwrap();
    assert_eq!(edge_lsb(&v("1110"), &v("0001")), RefEdge::Pos);
    assert_eq!(edge_lsb(&v("0001"), &v("111x")), RefEdge::Neg);
    assert_eq!(edge_lsb(&v("0000"), &v("1110")), RefEdge::None);
}
