//! `svsim-logic` の参照モデル (差分テストのオラクル、テスト専用)。
//!
//! 規格 (IEEE 1800-2023) の真理値表を 1 ビットずつそのまま書いた素朴な実装。
//! `svsim-logic` には依存しない。値の受け渡しは bits 文字列で行う (plan.md §5.1)。
//!
//! 現在はタスク M0-T1 のスタブで、本体はタスク M0-T9 で実装する。

#![forbid(unsafe_code)]

pub mod value;

pub use value::{RefLogic, RefVec};
