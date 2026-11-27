//! sv-sim の共通基盤: ソースマップ、診断、文字列インターン、型付きインデックス配列、状態アリーナ。
//!
//! 設計は `docs/milestones/M0/plan.md` §4 を参照。

#![forbid(unsafe_code)]

pub mod arena;
pub mod diag;
pub mod index;
pub mod intern;
pub mod source;
