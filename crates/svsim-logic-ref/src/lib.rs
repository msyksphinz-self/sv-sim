//! `svsim-logic` の参照モデル (差分テストのオラクル、テスト専用)。
//!
//! 規格 (IEEE 1800-2023) の真理値表を 1 ビットずつそのまま書いた素朴な実装。
//! `svsim-logic` には依存しない。値の受け渡しは bits 文字列で行う (plan.md §5.1)。
//! 速度は考えず、規格との対応 (条番号コメント) と読みやすさを優先する。
//!
//! # 関数の形の規則
//! - 全演算は自由関数で、オペランドは `&RefVec`。2 値版は無い (2 値の挙動は、X/Z を含まない入力の
//!   結果に [`to_two_state`] を適用して確かめる)。
//! - 本体のカーネルが `dst` を取る位置には、結果を新しく作るものは**結果の幅** (`usize`) を取り、
//!   結果を返す (例: `resize(width, x, signed)`, `get_slice(width, src, idx)`,
//!   `from_string_bytes(width, bytes)`, `from_f64_round(width, v)`)。
//!   `dst` を読み書きするもの (`set_slice`) は `&RefVec` を受けて新しい値を返す。
//! - `signed` やインデックスなどの引数は、本体の関数 (plan.md §3.5) と同じ順に取る。
//! - オペランドの幅の規則 (§11.6.1、plan.md §3.6) は呼び出し側の責務。文脈決定のオペランドの幅が
//!   揃っていなければ panic する。
//!
//! # 演算の方針 (plan.md §5.1)
//! - ビット演算、リダクション、等価、ワイルドカード、casez/casex、`?:` のマージ、エッジ:
//!   規格の真理値表を 4×4 の表として書き、ビットごとに適用する。
//! - シフト、連結、選択、拡張: `Vec` の要素の並べ替え。
//! - 算術、比較、べき乗、`$clog2`: X/Z があれば全 X (`$clog2` は `None`)。無ければ `BigInt` で計算し、
//!   `2^w` の剰余を取る。

#![forbid(unsafe_code)]

pub mod arith;
pub mod bitwise;
pub mod compare;
pub mod edge;
pub mod fmt;
pub mod literal;
pub mod real;
pub mod string;
pub mod structural;
pub mod value;

pub use arith::{add, div, mul, neg, pow, rem, sub};
pub use bitwise::{
    and, count_bits, is_unknown, logical_and, logical_equiv, logical_implies, logical_not,
    logical_or, not, onehot, onehot0, or, reduce_and, reduce_nand, reduce_nor, reduce_or,
    reduce_xnor, reduce_xor, truth, xnor, xor,
};
pub use compare::{
    case_eq, case_ne, casex_match, casez_match, cond_merge, eq, ge, gt, le, lt, ne, wildcard_eq,
    wildcard_ne,
};
pub use edge::{RefEdge, edge, edge_lsb};
pub use fmt::{RefFieldWidth, RefFmtRadix, format_int, itoa};
pub use literal::{RefLiteral, RefRadix, parse_based, parse_unsized_decimal};
pub use real::{RefRealToIntStatus, from_f64_round, from_f64_trunc, to_f32, to_f64};
pub use string::{atoi, from_string_bytes, to_string_bytes};
pub use structural::{
    RefIndex, ashr, clog2, concat, get_slice, index_value, replicate, resize, set_slice, shl, shr,
    to_two_state,
};
pub use value::{RefLogic, RefVec};
