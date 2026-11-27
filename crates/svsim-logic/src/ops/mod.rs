//! 演算 (層 2 のスライスカーネル、層 1 のスカラ版、層 3 の `impl LogicVec` メソッド)。
//!
//! 分野ごとにファイルを分け、並列タスクが同じファイルを触らずに済むようにしている (plan.md §3.7)。
//! このファイルは宣言だけを持ち、再エクスポートはしない。カーネルは `ops::<分野>::<名前>` で参照する
//! (例: `ops::bitwise::and`)。分野をまたいで同じ名前の関数があっても衝突しない。
//!
//! 各ファイルの約束 (plan.md §3.3「スライスカーネルの共通規約」):
//! - 4 値のカーネルは [`LogicRef`](crate::LogicRef) / [`LogicMut`](crate::LogicMut) を、
//!   2 値のカーネルは [`BitRef`](crate::BitRef) / [`BitMut`](crate::BitMut) を受け、名前の末尾に `2` を付ける。
//! - 文脈決定のオペランドの幅が揃っていることを `assert!` で検査する。それ以外では panic しない。
//! - ヒープ確保をしない。作業領域は [`Scratch`](crate::Scratch) で受ける。
//! - 出力は正規化不変条件 (plan.md §3.2) を満たす。

pub mod arith;
pub mod bitwise;
pub mod compare;
pub mod concat;
pub mod cond;
pub mod edge;
pub mod logical;
pub mod math;
pub mod reduce;
pub mod resize;
pub mod select;
pub mod shift;

// ===========================================================================
// 幅の検査 (ops 内で共有する。M0-T14 で bitwise.rs から移した)
// ===========================================================================

/// 文脈決定のオペランドの幅が揃っていることを検査する (plan.md §3.3 規約 1、決定 Q-1)。
/// release でも残す。幅 0 は `debug_assert!` で弾く (plan.md §3.2)。
#[inline]
#[track_caller]
pub(crate) fn check_same_width(op: &str, a: u32, b: u32) {
    assert!(a == b, "{op}: width mismatch ({a} vs {b})");
    debug_assert!(a != 0, "{op}: width 0 is only allowed in concatenation");
}

/// 1 オペランドの演算の幅 0 の検査 (`debug_assert!`、plan.md §3.2)。
#[inline]
#[track_caller]
pub(crate) fn check_nonzero_width(op: &str, w: u32) {
    debug_assert!(w != 0, "{op}: width 0 is only allowed in concatenation");
}
