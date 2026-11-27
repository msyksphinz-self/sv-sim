//! 比較・等価・ワイルドカード・casez/casex・`?:` のマージ (plan.md §3.5(b)(f))。
//!
//! 等価系はビットごとの 4×4 表で「そのビットの照合結果」を求め、それを AND で畳み込む。

use std::cmp::Ordering;

use crate::value::{RefLogic, RefVec};

use RefLogic::{One as I, X, Zero as O};

/// `==` の 1 ビット分の照合 (§11.4.5)。既知の一致 1、既知の不一致 0、どちらかが未知なら x。
const EQ_BIT: [[RefLogic; 4]; 4] = [
    //    0  1  x  z
    [I, O, X, X], // 0
    [O, I, X, X], // 1
    [X, X, X, X], // x
    [X, X, X, X], // z
];

/// `==?` の 1 ビット分の照合 (§11.4.6)。行が左、列が右。
/// 右の x/z はワイルドカードで常に一致 (1)。左の x/z はワイルドカードではなく x。
const WILDCARD_EQ_BIT: [[RefLogic; 4]; 4] = [
    //    0  1  x  z
    [I, O, I, I], // 0
    [O, I, I, I], // 1
    [X, X, I, I], // x
    [X, X, I, I], // z
];

/// `casez` の 1 ビット分の照合 (§12.5.1)。どちらかが z (`?` を含む) なら無視 (一致)。
/// 残りは `===` (x は x とだけ一致)。
const CASEZ_BIT: [[bool; 4]; 4] = [
    //    0      1      x      z
    [true, false, false, true], // 0
    [false, true, false, true], // 1
    [false, false, true, true], // x
    [true, true, true, true],   // z
];

/// `casex` の 1 ビット分の照合 (§12.5.1)。どちらかが x か z なら無視 (一致)。
const CASEX_BIT: [[bool; 4]; 4] = [
    //    0      1      x     z
    [true, false, true, true], // 0
    [false, true, true, true], // 1
    [true, true, true, true],  // x
    [true, true, true, true],  // z
];

/// `c ? t : f` で c が x/z のときのビットごとのマージ (§11.4.11 の表)。
/// 両方 0 なら 0、両方 1 なら 1、それ以外 (z と z も含む) は x。
const COND_MERGE_BIT: [[RefLogic; 4]; 4] = [
    //    0  1  x  z
    [O, X, X, X], // 0
    [X, I, X, X], // 1
    [X, X, X, X], // x
    [X, X, X, X], // z
];

fn check_widths(x: &RefVec, y: &RefVec) {
    assert_eq!(
        x.width(),
        y.width(),
        "operand widths must match (extended by the caller)"
    );
}

fn fold_and(x: &RefVec, y: &RefVec, table: &[[RefLogic; 4]; 4]) -> RefLogic {
    check_widths(x, y);
    x.0.iter()
        .zip(y.0.iter())
        .fold(I, |acc, (a, b)| acc.and(table[a.idx()][b.idx()]))
}

/// `x == y` (§11.4.5)。既知ビットに不一致があれば 0、無くて未知があれば x、それ以外は 1。
pub fn eq(x: &RefVec, y: &RefVec) -> RefLogic {
    fold_and(x, y, &EQ_BIT)
}

/// `x != y` (§11.4.5)。`eq` の否定。
pub fn ne(x: &RefVec, y: &RefVec) -> RefLogic {
    !eq(x, y)
}

/// `x === y` (§11.4.5)。x は x とだけ、z は z とだけ一致する。
pub fn case_eq(x: &RefVec, y: &RefVec) -> bool {
    check_widths(x, y);
    x.0 == y.0
}

/// `x !== y` (§11.4.5)。
pub fn case_ne(x: &RefVec, y: &RefVec) -> bool {
    !case_eq(x, y)
}

/// `x ==? y` (§11.4.6)。右オペランドの x/z はワイルドカード。
pub fn wildcard_eq(x: &RefVec, y: &RefVec) -> RefLogic {
    fold_and(x, y, &WILDCARD_EQ_BIT)
}

/// `x !=? y` (§11.4.6)。`wildcard_eq` の否定。
pub fn wildcard_ne(x: &RefVec, y: &RefVec) -> RefLogic {
    !wildcard_eq(x, y)
}

/// `casez` の照合 (§12.5.1)。
pub fn casez_match(x: &RefVec, y: &RefVec) -> bool {
    check_widths(x, y);
    x.0.iter()
        .zip(y.0.iter())
        .all(|(a, b)| CASEZ_BIT[a.idx()][b.idx()])
}

/// `casex` の照合 (§12.5.1)。
pub fn casex_match(x: &RefVec, y: &RefVec) -> bool {
    check_widths(x, y);
    x.0.iter()
        .zip(y.0.iter())
        .all(|(a, b)| CASEX_BIT[a.idx()][b.idx()])
}

/// 関係演算の共通部分 (§11.4.4)。どちらかに未知ビットがあれば x。
/// 符号付きで比べるのは `signed` (両オペランドが符号付き、§11.8.1) のときだけ。
fn relational(x: &RefVec, y: &RefVec, signed: bool, pred: fn(Ordering) -> bool) -> RefLogic {
    check_widths(x, y);
    match (x.to_bigint(signed), y.to_bigint(signed)) {
        (Some(a), Some(b)) => RefLogic::from_bool(pred(a.cmp(&b))),
        _ => X,
    }
}

/// `x < y` (§11.4.4)。
pub fn lt(x: &RefVec, y: &RefVec, signed: bool) -> RefLogic {
    relational(x, y, signed, Ordering::is_lt)
}

/// `x <= y` (§11.4.4)。
pub fn le(x: &RefVec, y: &RefVec, signed: bool) -> RefLogic {
    relational(x, y, signed, Ordering::is_le)
}

/// `x > y` (§11.4.4)。
pub fn gt(x: &RefVec, y: &RefVec, signed: bool) -> RefLogic {
    relational(x, y, signed, Ordering::is_gt)
}

/// `x >= y` (§11.4.4)。
pub fn ge(x: &RefVec, y: &RefVec, signed: bool) -> RefLogic {
    relational(x, y, signed, Ordering::is_ge)
}

/// `c ? t : f` で c が x/z のときの結果 (§11.4.11)。t と f の幅は同じであること。
/// z と z のビットも x になる (規格の表を優先。[要VCS確認 V-8])。
pub fn cond_merge(t: &RefVec, f: &RefVec) -> RefVec {
    check_widths(t, f);
    RefVec(
        t.0.iter()
            .zip(f.0.iter())
            .map(|(a, b)| COND_MERGE_BIT[a.idx()][b.idx()])
            .collect(),
    )
}
