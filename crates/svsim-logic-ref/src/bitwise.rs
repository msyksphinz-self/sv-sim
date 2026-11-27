//! ビット演算・リダクション・真理値・論理演算・ビット数え (plan.md §3.5(a))。
//!
//! 真理値表は IEEE 1800-2023 §11.4.8 の表を 4×4 の配列としてそのまま書き、
//! ビットごとに適用する。表の添字は 0, 1, x, z の順。

use crate::value::{RefLogic, RefVec};

// 表を読みやすくするための短い別名。ビット演算の結果に z は出ない (§11.4.8)。
use RefLogic::{One as I, X, Z as Hz, Zero as O};

/// `~` の表 (§11.4.8)。z の入力は x と同じに扱う。
pub(crate) const NOT_TABLE: [RefLogic; 4] = [I, O, X, X];

/// `&` の表 (§11.4.8)。行が左オペランド、列が右オペランド (0, 1, x, z)。
pub(crate) const AND_TABLE: [[RefLogic; 4]; 4] = [
    //    0  1  x  z
    [O, O, O, O], // 0
    [O, I, X, X], // 1
    [O, X, X, X], // x
    [O, X, X, X], // z
];

/// `|` の表 (§11.4.8)。
pub(crate) const OR_TABLE: [[RefLogic; 4]; 4] = [
    //    0  1  x  z
    [O, I, X, X], // 0
    [I, I, I, I], // 1
    [X, I, X, X], // x
    [X, I, X, X], // z
];

/// `^` の表 (§11.4.8)。
pub(crate) const XOR_TABLE: [[RefLogic; 4]; 4] = [
    //    0  1  x  z
    [O, I, X, X], // 0
    [I, O, X, X], // 1
    [X, X, X, X], // x
    [X, X, X, X], // z
];

/// `^~` / `~^` の表 (§11.4.8)。
pub(crate) const XNOR_TABLE: [[RefLogic; 4]; 4] = [
    //    0  1  x  z
    [I, O, X, X], // 0
    [O, I, X, X], // 1
    [X, X, X, X], // x
    [X, X, X, X], // z
];

fn map2(x: &RefVec, y: &RefVec, table: &[[RefLogic; 4]; 4]) -> RefVec {
    assert_eq!(x.width(), y.width(), "operand widths must match (CD)");
    RefVec(
        x.0.iter()
            .zip(y.0.iter())
            .map(|(a, b)| table[a.idx()][b.idx()])
            .collect(),
    )
}

/// `~x` (§11.4.8)。
pub fn not(x: &RefVec) -> RefVec {
    RefVec(x.0.iter().map(|b| NOT_TABLE[b.idx()]).collect())
}

/// `x & y` (§11.4.8)。幅は同じであること。
pub fn and(x: &RefVec, y: &RefVec) -> RefVec {
    map2(x, y, &AND_TABLE)
}

/// `x | y` (§11.4.8)。
pub fn or(x: &RefVec, y: &RefVec) -> RefVec {
    map2(x, y, &OR_TABLE)
}

/// `x ^ y` (§11.4.8)。
pub fn xor(x: &RefVec, y: &RefVec) -> RefVec {
    map2(x, y, &XOR_TABLE)
}

/// `x ~^ y` / `x ^~ y` (§11.4.8)。
pub fn xnor(x: &RefVec, y: &RefVec) -> RefVec {
    map2(x, y, &XNOR_TABLE)
}

fn fold(x: &RefVec, init: RefLogic, table: &[[RefLogic; 4]; 4]) -> RefLogic {
    x.0.iter().fold(init, |acc, b| table[acc.idx()][b.idx()])
}

/// 単項 `&x` (§11.4.9)。ビットを AND の表で順に畳み込む。
pub fn reduce_and(x: &RefVec) -> RefLogic {
    fold(x, I, &AND_TABLE)
}

/// 単項 `~&x` (§11.4.9)。
pub fn reduce_nand(x: &RefVec) -> RefLogic {
    !reduce_and(x)
}

/// 単項 `|x` (§11.4.9)。
pub fn reduce_or(x: &RefVec) -> RefLogic {
    fold(x, O, &OR_TABLE)
}

/// 単項 `~|x` (§11.4.9)。
pub fn reduce_nor(x: &RefVec) -> RefLogic {
    !reduce_or(x)
}

/// 単項 `^x` (§11.4.9)。
pub fn reduce_xor(x: &RefVec) -> RefLogic {
    fold(x, O, &XOR_TABLE)
}

/// 単項 `~^x` / `^~x` (§11.4.9)。
pub fn reduce_xnor(x: &RefVec) -> RefLogic {
    !reduce_xor(x)
}

/// 式の真理値 (§11.4.7, §12.4)。既知の 1 があれば 1、全ビットが既知の 0 なら 0、それ以外は X。
pub fn truth(x: &RefVec) -> RefLogic {
    if x.0.contains(&I) {
        I
    } else if x.0.iter().all(|b| *b == O) {
        O
    } else {
        X
    }
}

/// `!x` (§11.4.7)。`truth(x)` の否定。
pub fn logical_not(x: &RefVec) -> RefLogic {
    !truth(x)
}

/// `x && y` (§11.4.7)。各オペランドの真理値に AND の表を適用する。短絡評価は評価器の責務。
pub fn logical_and(x: &RefVec, y: &RefVec) -> RefLogic {
    truth(x).and(truth(y))
}

/// `x || y` (§11.4.7)。
pub fn logical_or(x: &RefVec, y: &RefVec) -> RefLogic {
    truth(x).or(truth(y))
}

/// `x -> y` (§11.4.7)。`!x || y`。
pub fn logical_implies(x: &RefVec, y: &RefVec) -> RefLogic {
    truth(x).implies(truth(y))
}

/// `x <-> y` (§11.4.7)。`(x -> y) && (y -> x)`。
pub fn logical_equiv(x: &RefVec, y: &RefVec) -> RefLogic {
    truth(x).equiv(truth(y))
}

/// `$countbits(x, set...)` (§20.9)。`set` に含まれる値を持つビットの数。
/// `$countones(x)` は `count_bits(x, &[RefLogic::One])`。
pub fn count_bits(x: &RefVec, set: &[RefLogic]) -> usize {
    x.0.iter().filter(|b| set.contains(b)).count()
}

/// `$onehot(x)` (§20.9) = `$countbits(x, '1) == 1`。
pub fn onehot(x: &RefVec) -> bool {
    count_bits(x, &[I]) == 1
}

/// `$onehot0(x)` (§20.9) = `$countbits(x, '1) <= 1`。
pub fn onehot0(x: &RefVec) -> bool {
    count_bits(x, &[I]) <= 1
}

/// `$isunknown(x)` (§20.9) = `$countbits(x, 'x, 'z) != 0`。
pub fn is_unknown(x: &RefVec) -> bool {
    count_bits(x, &[X, Hz]) != 0
}
