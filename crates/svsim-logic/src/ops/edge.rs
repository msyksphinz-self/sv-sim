//! エッジ検出 (IEEE 1800-2023 §9.4.2)。タスク M0-T4。
//!
//! §9.4.2 の遷移表 (行が旧値、列が新値) [要VCS確認 V-12]:
//!
//! | old \ new | 0 | 1 | x | z |
//! |---|---|---|---|---|
//! | 0 | None | Pos | Pos | Pos |
//! | 1 | Neg | None | Neg | Neg |
//! | x | Neg | Pos | None | None |
//! | z | Neg | Pos | None | None |
//!
//! posedge は 0→1/x/z と x/z→1、negedge は 1→0/x/z と x/z→0。x↔z の遷移はエッジではない。
//! `edge` イベント (§9.4.2) は「Pos または Neg」で、評価器が判定する ([`Edge::is_edge`])。
//! 多ビットの式では LSB の遷移だけを見る (§9.4.2)。

use crate::logic::Logic;
use crate::scalar::L64;
use crate::view::{BitRef, LogicRef};

/// 1 ビットの遷移の種類 (§9.4.2)。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Edge {
    /// エッジではない。
    None,
    /// posedge。
    Pos,
    /// negedge。
    Neg,
}

impl Edge {
    /// posedge なら真。
    #[inline]
    pub const fn is_pos(self) -> bool {
        matches!(self, Edge::Pos)
    }

    /// negedge なら真。
    #[inline]
    pub const fn is_neg(self) -> bool {
        matches!(self, Edge::Neg)
    }

    /// `edge` イベント (posedge または negedge) なら真 (§9.4.2)。
    #[inline]
    pub const fn is_edge(self) -> bool {
        !matches!(self, Edge::None)
    }
}

/// aval/bval のビット (下位 1 ビットだけを見る) からエッジを求める式。層 1 の仕様 (JIT 用)。
///
/// - pos = (old が 0 かつ new が 0 でない) または (old が未知かつ new が 1)
/// - neg = (old が 1 かつ new が 1 でない) または (old が未知かつ new が 0)
#[inline(always)]
const fn edge_ab(oa: u64, ob: u64, na: u64, nb: u64) -> Edge {
    let old0 = !oa & !ob;
    let old1 = oa & !ob;
    let new0 = !na & !nb;
    let new1 = na & !nb;
    let pos = (old0 & !new0) | (ob & new1);
    let neg = (old1 & !new1) | (ob & new0);
    if pos & 1 != 0 {
        Edge::Pos
    } else if neg & 1 != 0 {
        Edge::Neg
    } else {
        Edge::None
    }
}

/// 1 ビットの遷移 `old → new` のエッジ (§9.4.2) [要VCS確認 V-12]。
#[inline]
pub const fn edge(old: Logic, new: Logic) -> Edge {
    edge_ab(
        old.aval() as u64,
        old.bval() as u64,
        new.aval() as u64,
        new.bval() as u64,
    )
}

/// 2 値の遷移のエッジ (§9.4.2)。0→1 は Pos、1→0 は Neg。
#[inline]
pub const fn edge2(old: bool, new: bool) -> Edge {
    match (old, new) {
        (false, true) => Edge::Pos,
        (true, false) => Edge::Neg,
        _ => Edge::None,
    }
}

/// 層 1: 幅 ≤ 64 の値の LSB の遷移 (§9.4.2)。
#[inline]
pub const fn edge_l64(old: L64, new: L64) -> Edge {
    edge_ab(old.a, old.b, new.a, new.b)
}

/// 層 1: 2 値 (幅 ≤ 64) の LSB の遷移 (§9.4.2)。
#[inline]
pub const fn edge2_l64(old: u64, new: u64) -> Edge {
    edge2(old & 1 != 0, new & 1 != 0)
}

/// 式の値の LSB の遷移でエッジを判定する (§9.4.2) [要VCS確認 V-12]。
///
/// `old` と `new` の幅は違ってもよい (LSB だけを見る)。
///
/// # Panics
/// どちらかの幅が 0 の場合。
pub fn edge_lsb(old: LogicRef<'_>, new: LogicRef<'_>) -> Edge {
    assert!(
        old.width() != 0 && new.width() != 0,
        "edge_lsb: width 0 ({} -> {})",
        old.width(),
        new.width()
    );
    edge_ab(old.aval()[0], old.bval()[0], new.aval()[0], new.bval()[0])
}

/// 2 値の式の値の LSB の遷移 (§9.4.2)。
///
/// # Panics
/// どちらかの幅が 0 の場合。
pub fn edge_lsb2(old: BitRef<'_>, new: BitRef<'_>) -> Edge {
    assert!(
        old.width() != 0 && new.width() != 0,
        "edge_lsb2: width 0 ({} -> {})",
        old.width(),
        new.width()
    );
    edge2_l64(old.words()[0], new.words()[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use Edge::{Neg as N, None as E, Pos as P};
    use Logic::{L0 as O, L1 as I, X, Z};

    /// §9.4.2 の 16 通り [要VCS確認 V-12]。
    #[test]
    fn table_9_4_2() {
        let order = [O, I, X, Z];
        let t = [[E, P, P, P], [N, E, N, N], [N, P, E, E], [N, P, E, E]];
        for (i, &o) in order.iter().enumerate() {
            for (j, &n) in order.iter().enumerate() {
                assert_eq!(edge(o, n), t[i][j], "{o}->{n}");
                assert_eq!(edge_l64(L64::filled(o, 1), L64::filled(n, 1)), t[i][j]);
            }
        }
        assert_eq!(edge2(false, true), P);
        assert_eq!(edge2(true, false), N);
        assert_eq!(edge2(true, true), E);
        assert!(P.is_edge() && N.is_edge() && !E.is_edge());
    }
}
