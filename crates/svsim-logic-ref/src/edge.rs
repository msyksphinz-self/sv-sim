//! エッジ検出 (plan.md §3.5(k)、IEEE 1800-2023 §9.4.2)。

use crate::value::{RefLogic, RefVec};

/// 遷移の種類。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RefEdge {
    /// エッジではない。
    None,
    /// posedge。
    Pos,
    /// negedge。
    Neg,
}

use RefEdge::{Neg as N, None as E, Pos as P};

/// §9.4.2 の遷移表。行が旧値、列が新値 (0, 1, x, z)。
/// posedge は 0→1/x/z と x/z→1、negedge は 1→0/x/z と x/z→0。x↔z はエッジではない。
const EDGE_TABLE: [[RefEdge; 4]; 4] = [
    //    0  1  x  z
    [E, P, P, P], // 0
    [N, E, N, N], // 1
    [N, P, E, E], // x
    [N, P, E, E], // z
];

/// 1 ビットの遷移 `old → new` のエッジ (§9.4.2) [要VCS確認 V-12]。
pub fn edge(old: RefLogic, new: RefLogic) -> RefEdge {
    EDGE_TABLE[old.idx()][new.idx()]
}

/// 式の値の LSB の遷移でエッジを判定する (§9.4.2)。
pub fn edge_lsb(old: &RefVec, new: &RefVec) -> RefEdge {
    edge(old.bit(0), new.bit(0))
}
