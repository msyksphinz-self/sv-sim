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
