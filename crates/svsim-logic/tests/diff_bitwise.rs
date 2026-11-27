//! M0-T4: ビット演算・リダクション・`$countbits` 系・真理値と論理演算・エッジ検出の差分テスト。
//!
//! 参照モデル `svsim-logic-ref` と、層 1 (幅 ≤ 64)、層 2 (プレーンが隣接しないビュー)、
//! 層 3 (`LogicVec` / `BitVec`)、`_assign` 版、2 値版を比べる (plan.md §5.2、test-plan CR-1〜CR-7)。
//!
//! - M0-TC22: 幅 1〜3 の 4 値の全値 (2 値は全値) の網羅テスト (plan.md §5.4)。
//! - M0-TC23: proptest の差分テスト (plan.md §5.3 の戦略)。
//!
//! 条番号: ビット演算 IEEE 1800-2023 §11.4.8、リダクション §11.4.9、`$countbits` 系 §20.9、
//! 真理値と論理演算 §11.4.7 / §12.4、エッジ §9.4.2。

mod common;

use common::*;
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;
use svsim_logic::ops::reduce::LogicSet;
use svsim_logic::ops::{bitwise, edge, logical, reduce};
use svsim_logic::{BitMut, BitRef, BitVec, L64, Logic, LogicMut, LogicRef, LogicVec};
use svsim_logic_ref as r;
use svsim_logic_ref::{RefLogic, RefVec};

// ===========================================================================
// ビット演算 (§11.4.8)
// ===========================================================================

#[derive(Clone, Copy, Debug)]
enum Bin {
    And,
    Or,
    Xor,
    Xnor,
}

const BINS: [Bin; 4] = [Bin::And, Bin::Or, Bin::Xor, Bin::Xnor];

impl Bin {
    fn reference(self, x: &RefVec, y: &RefVec) -> RefVec {
        match self {
            Bin::And => r::and(x, y),
            Bin::Or => r::or(x, y),
            Bin::Xor => r::xor(x, y),
            Bin::Xnor => r::xnor(x, y),
        }
    }
    fn l3(self, x: &LogicVec, y: &LogicVec) -> LogicVec {
        match self {
            Bin::And => x.and(y),
            Bin::Or => x.or(y),
            Bin::Xor => x.xor(y),
            Bin::Xnor => x.xnor(y),
        }
    }
    fn l3_assign(self, x: &mut LogicVec, y: &LogicVec) {
        match self {
            Bin::And => x.and_assign(y),
            Bin::Or => x.or_assign(y),
            Bin::Xor => x.xor_assign(y),
            Bin::Xnor => x.xnor_assign(y),
        }
    }
    fn l2(self, d: LogicMut<'_>, x: LogicRef<'_>, y: LogicRef<'_>) {
        match self {
            Bin::And => bitwise::and(d, x, y),
            Bin::Or => bitwise::or(d, x, y),
            Bin::Xor => bitwise::xor(d, x, y),
            Bin::Xnor => bitwise::xnor(d, x, y),
        }
    }
    fn l2_assign(self, d: LogicMut<'_>, y: LogicRef<'_>) {
        match self {
            Bin::And => bitwise::and_assign(d, y),
            Bin::Or => bitwise::or_assign(d, y),
            Bin::Xor => bitwise::xor_assign(d, y),
            Bin::Xnor => bitwise::xnor_assign(d, y),
        }
    }
    fn l1(self, x: L64, y: L64, w: u32) -> L64 {
        match self {
            Bin::And => bitwise::and_l64(x, y, w),
            Bin::Or => bitwise::or_l64(x, y, w),
            Bin::Xor => bitwise::xor_l64(x, y, w),
            Bin::Xnor => bitwise::xnor_l64(x, y, w),
        }
    }
    fn b3(self, x: &BitVec, y: &BitVec) -> BitVec {
        match self {
            Bin::And => x.and(y),
            Bin::Or => x.or(y),
            Bin::Xor => x.xor(y),
            Bin::Xnor => x.xnor(y),
        }
    }
    fn b3_assign(self, x: &mut BitVec, y: &BitVec) {
        match self {
            Bin::And => x.and_assign(y),
            Bin::Or => x.or_assign(y),
            Bin::Xor => x.xor_assign(y),
            Bin::Xnor => x.xnor_assign(y),
        }
    }
    fn b2(self, d: BitMut<'_>, x: BitRef<'_>, y: BitRef<'_>) {
        match self {
            Bin::And => bitwise::and2(d, x, y),
            Bin::Or => bitwise::or2(d, x, y),
            Bin::Xor => bitwise::xor2(d, x, y),
            Bin::Xnor => bitwise::xnor2(d, x, y),
        }
    }
    fn b2_assign(self, d: BitMut<'_>, y: BitRef<'_>) {
        match self {
            Bin::And => bitwise::and_assign2(d, y),
            Bin::Or => bitwise::or_assign2(d, y),
            Bin::Xor => bitwise::xor_assign2(d, y),
            Bin::Xnor => bitwise::xnor_assign2(d, y),
        }
    }
    fn b1(self, x: u64, y: u64, w: u32) -> u64 {
        match self {
            Bin::And => bitwise::and2_l64(x, y, w),
            Bin::Or => bitwise::or2_l64(x, y, w),
            Bin::Xor => bitwise::xor2_l64(x, y, w),
            Bin::Xnor => bitwise::xnor2_l64(x, y, w),
        }
    }
}

/// 4 値の 2 項ビット演算を 3 層 + `_assign` で参照モデルと比べる (CR-1〜CR-4, CR-7)。
fn check_bin(op: Bin, x: &LogicVec, y: &LogicVec) -> Result<(), TestCaseError> {
    let w = x.width();
    let ctx = format!("{op:?} w={w} x={x:?} y={y:?}");
    let want = op.reference(&to_ref(x), &to_ref(y));
    let (x0, y0) = (x.clone(), y.clone());

    check_eq_ref(op.l3(x, y).as_ref(), &want, &format!("{ctx} L3"))?;
    let mut z = x.clone();
    op.l3_assign(&mut z, y);
    check_eq_ref(z.as_ref(), &want, &format!("{ctx} L3 assign"))?;

    let (sx, sy) = (SplitLogic::from_vec(x), SplitLogic::from_vec(y));
    let mut d = SplitLogic::dirty(w);
    op.l2(d.as_mut(), sx.as_ref(), sy.as_ref());
    check_eq_ref(d.as_ref(), &want, &format!("{ctx} L2"))?;
    let mut d = sx.clone();
    op.l2_assign(d.as_mut(), sy.as_ref());
    check_eq_ref(d.as_ref(), &want, &format!("{ctx} L2 assign"))?;

    if w <= 64 {
        let got = op.l1(to_l64(x), to_l64(y), w);
        check_eq_ref_l64(got, w, &want, &format!("{ctx} L1"))?;
    }
    // CR-7: 入力を変更しない
    prop_assert_eq!(x, &x0);
    prop_assert_eq!(y, &y0);
    prop_assert_eq!(sx.to_vec(), x0);
    Ok(())
}

/// 2 値の 2 項ビット演算 (CR-5)。
fn check_bin2(op: Bin, x: &BitVec, y: &BitVec) -> Result<(), TestCaseError> {
    let w = x.width();
    let ctx = format!("{op:?}2 w={w} x={x:?} y={y:?}");
    let want = r::to_two_state(&op.reference(&bits_to_ref(x), &bits_to_ref(y)));

    check_eq_ref2(op.b3(x, y).as_ref(), &want, &format!("{ctx} L3"))?;
    let mut z = x.clone();
    op.b3_assign(&mut z, y);
    check_eq_ref2(z.as_ref(), &want, &format!("{ctx} L3 assign"))?;

    let (sx, sy) = (SplitBits::from_vec(x), SplitBits::from_vec(y));
    let mut d = SplitBits::dirty(w);
    op.b2(d.as_mut(), sx.as_ref(), sy.as_ref());
    check_eq_ref2(d.as_ref(), &want, &format!("{ctx} L2"))?;
    let mut d = sx.clone();
    op.b2_assign(d.as_mut(), sy.as_ref());
    check_eq_ref2(d.as_ref(), &want, &format!("{ctx} L2 assign"))?;

    if w <= 64 {
        let got = op.b1(to_u64_scalar(x), to_u64_scalar(y), w);
        check_eq_ref_u64(got, w, &want, &format!("{ctx} L1"))?;
    }
    Ok(())
}

/// 4 値の `~` (CR-1〜CR-4)。
fn check_not(x: &LogicVec) -> Result<(), TestCaseError> {
    let w = x.width();
    let ctx = format!("not w={w} x={x:?}");
    let want = r::not(&to_ref(x));
    let x0 = x.clone();
    check_eq_ref((!x).as_ref(), &want, &format!("{ctx} L3"))?;
    check_eq_ref((!x.clone()).as_ref(), &want, &format!("{ctx} L3 owned"))?;
    let mut z = x.clone();
    z.not_assign();
    check_eq_ref(z.as_ref(), &want, &format!("{ctx} L3 assign"))?;
    let sx = SplitLogic::from_vec(x);
    let mut d = SplitLogic::dirty(w);
    bitwise::not(d.as_mut(), sx.as_ref());
    check_eq_ref(d.as_ref(), &want, &format!("{ctx} L2"))?;
    let mut d = sx.clone();
    bitwise::not_assign(d.as_mut());
    check_eq_ref(d.as_ref(), &want, &format!("{ctx} L2 assign"))?;
    if w <= 64 {
        check_eq_ref_l64(
            bitwise::not_l64(to_l64(x), w),
            w,
            &want,
            &format!("{ctx} L1"),
        )?;
    }
    prop_assert_eq!(x, &x0);
    Ok(())
}

/// 2 値の `~` (CR-5)。
fn check_not2(x: &BitVec) -> Result<(), TestCaseError> {
    let w = x.width();
    let ctx = format!("not2 w={w} x={x:?}");
    let want = r::to_two_state(&r::not(&bits_to_ref(x)));
    check_eq_ref2((!x).as_ref(), &want, &format!("{ctx} L3"))?;
    check_eq_ref2((!x.clone()).as_ref(), &want, &format!("{ctx} L3 owned"))?;
    let mut z = x.clone();
    z.not_assign();
    check_eq_ref2(z.as_ref(), &want, &format!("{ctx} L3 assign"))?;
    let sx = SplitBits::from_vec(x);
    let mut d = SplitBits::dirty(w);
    bitwise::not2(d.as_mut(), sx.as_ref());
    check_eq_ref2(d.as_ref(), &want, &format!("{ctx} L2"))?;
    let mut d = sx.clone();
    bitwise::not_assign2(d.as_mut());
    check_eq_ref2(d.as_ref(), &want, &format!("{ctx} L2 assign"))?;
    if w <= 64 {
        let got = bitwise::not2_l64(to_u64_scalar(x), w);
        check_eq_ref_u64(got, w, &want, &format!("{ctx} L1"))?;
    }
    Ok(())
}

// ===========================================================================
// リダクション (§11.4.9)
// ===========================================================================

#[derive(Clone, Copy, Debug)]
enum Red {
    And,
    Nand,
    Or,
    Nor,
    Xor,
    Xnor,
}

const REDS: [Red; 6] = [Red::And, Red::Nand, Red::Or, Red::Nor, Red::Xor, Red::Xnor];

impl Red {
    fn reference(self, x: &RefVec) -> RefLogic {
        match self {
            Red::And => r::reduce_and(x),
            Red::Nand => r::reduce_nand(x),
            Red::Or => r::reduce_or(x),
            Red::Nor => r::reduce_nor(x),
            Red::Xor => r::reduce_xor(x),
            Red::Xnor => r::reduce_xnor(x),
        }
    }
    fn l3(self, x: &LogicVec) -> Logic {
        match self {
            Red::And => x.reduce_and(),
            Red::Nand => x.reduce_nand(),
            Red::Or => x.reduce_or(),
            Red::Nor => x.reduce_nor(),
            Red::Xor => x.reduce_xor(),
            Red::Xnor => x.reduce_xnor(),
        }
    }
    fn l2(self, x: LogicRef<'_>) -> Logic {
        match self {
            Red::And => reduce::reduce_and(x),
            Red::Nand => reduce::reduce_nand(x),
            Red::Or => reduce::reduce_or(x),
            Red::Nor => reduce::reduce_nor(x),
            Red::Xor => reduce::reduce_xor(x),
            Red::Xnor => reduce::reduce_xnor(x),
        }
    }
    fn l1(self, x: L64, w: u32) -> Logic {
        match self {
            Red::And => reduce::reduce_and_l64(x, w),
            Red::Nand => reduce::reduce_nand_l64(x, w),
            Red::Or => reduce::reduce_or_l64(x, w),
            Red::Nor => reduce::reduce_nor_l64(x, w),
            Red::Xor => reduce::reduce_xor_l64(x, w),
            Red::Xnor => reduce::reduce_xnor_l64(x, w),
        }
    }
    fn b3(self, x: &BitVec) -> bool {
        match self {
            Red::And => x.reduce_and(),
            Red::Nand => x.reduce_nand(),
            Red::Or => x.reduce_or(),
            Red::Nor => x.reduce_nor(),
            Red::Xor => x.reduce_xor(),
            Red::Xnor => x.reduce_xnor(),
        }
    }
    fn b2(self, x: BitRef<'_>) -> bool {
        match self {
            Red::And => reduce::reduce_and2(x),
            Red::Nand => reduce::reduce_nand2(x),
            Red::Or => reduce::reduce_or2(x),
            Red::Nor => reduce::reduce_nor2(x),
            Red::Xor => reduce::reduce_xor2(x),
            Red::Xnor => reduce::reduce_xnor2(x),
        }
    }
    fn b1(self, x: u64, w: u32) -> bool {
        match self {
            Red::And => reduce::reduce_and2_l64(x, w),
            Red::Nand => reduce::reduce_nand2_l64(x, w),
            Red::Or => reduce::reduce_or2_l64(x, w),
            Red::Nor => reduce::reduce_nor2_l64(x, w),
            Red::Xor => reduce::reduce_xor2_l64(x, w),
            Red::Xnor => reduce::reduce_xnor2_l64(x, w),
        }
    }
}

fn check_reductions(x: &LogicVec) -> Result<(), TestCaseError> {
    let w = x.width();
    let rx = to_ref(x);
    let sx = SplitLogic::from_vec(x);
    for op in REDS {
        let ctx = format!("{op:?} w={w} x={x:?}");
        let want = op.reference(&rx);
        prop_assert_eq!(logic_to_ref(op.l3(x)), want, "{} L3", ctx);
        prop_assert_eq!(logic_to_ref(op.l2(sx.as_ref())), want, "{} L2", ctx);
        if w <= 64 {
            prop_assert_eq!(logic_to_ref(op.l1(to_l64(x), w)), want, "{} L1", ctx);
        }
    }
    Ok(())
}

fn check_reductions2(x: &BitVec) -> Result<(), TestCaseError> {
    let w = x.width();
    let rx = bits_to_ref(x);
    let sx = SplitBits::from_vec(x);
    for op in REDS {
        let ctx = format!("{op:?}2 w={w} x={x:?}");
        let want = op.reference(&rx);
        prop_assert_eq!(bool_to_ref(op.b3(x)), want, "{} L3", ctx);
        prop_assert_eq!(bool_to_ref(op.b2(sx.as_ref())), want, "{} L2", ctx);
        if w <= 64 {
            prop_assert_eq!(bool_to_ref(op.b1(to_u64_scalar(x), w)), want, "{} L1", ctx);
        }
    }
    Ok(())
}

// ===========================================================================
// $countbits 系 (§20.9)
// ===========================================================================

/// 4 ビットのマスク (Logic の repr の順) から集合を作る。参照モデル用の並びも返す。
fn set_from_mask(m: u8) -> (LogicSet, Vec<RefLogic>) {
    let vals: Vec<Logic> = Logic::ALL
        .into_iter()
        .filter(|v| m & (1 << v.to_u8()) != 0)
        .collect();
    let set = LogicSet::of(&vals);
    (set, vals.into_iter().map(logic_to_ref).collect())
}

fn check_counts(x: &LogicVec, set_mask: u8) -> Result<(), TestCaseError> {
    let w = x.width();
    let ctx = format!("count w={w} x={x:?} set={set_mask:04b}");
    let rx = to_ref(x);
    let (set, rset) = set_from_mask(set_mask);
    let want = r::count_bits(&rx, &rset) as u32;
    let ones = r::count_bits(&rx, &[RefLogic::One]) as u32;
    let sx = SplitLogic::from_vec(x);

    prop_assert_eq!(x.count_bits(set), want, "{} L3", ctx);
    prop_assert_eq!(reduce::count_bits(sx.as_ref(), set), want, "{} L2", ctx);
    prop_assert_eq!(x.count_ones(), ones, "{} ones L3", ctx);
    prop_assert_eq!(reduce::count_ones(sx.as_ref()), ones, "{} ones L2", ctx);
    prop_assert_eq!(x.onehot(), r::onehot(&rx), "{} onehot", ctx);
    prop_assert_eq!(reduce::onehot(sx.as_ref()), r::onehot(&rx), "{}", ctx);
    prop_assert_eq!(x.onehot0(), r::onehot0(&rx), "{} onehot0", ctx);
    prop_assert_eq!(reduce::onehot0(sx.as_ref()), r::onehot0(&rx), "{}", ctx);
    prop_assert_eq!(x.is_unknown(), r::is_unknown(&rx), "{} isunknown", ctx);
    prop_assert_eq!(
        reduce::is_unknown(sx.as_ref()),
        r::is_unknown(&rx),
        "{}",
        ctx
    );
    if w <= 64 {
        let l = to_l64(x);
        prop_assert_eq!(reduce::count_bits_l64(l, w, set), want, "{} L1", ctx);
        prop_assert_eq!(reduce::count_ones_l64(l, w), ones, "{} L1", ctx);
        prop_assert_eq!(reduce::onehot_l64(l, w), r::onehot(&rx), "{} L1", ctx);
        prop_assert_eq!(reduce::onehot0_l64(l, w), r::onehot0(&rx), "{} L1", ctx);
        prop_assert_eq!(reduce::is_unknown_l64(l, w), r::is_unknown(&rx), "{}", ctx);
    }
    Ok(())
}

fn check_counts2(x: &BitVec, set_mask: u8) -> Result<(), TestCaseError> {
    let w = x.width();
    let ctx = format!("count2 w={w} x={x:?} set={set_mask:04b}");
    let rx = bits_to_ref(x);
    let (set, rset) = set_from_mask(set_mask);
    let want = r::count_bits(&rx, &rset) as u32;
    let ones = r::count_bits(&rx, &[RefLogic::One]) as u32;
    prop_assert_eq!(x.count_bits(set), want, "{} L3", ctx);
    prop_assert_eq!(reduce::count_bits2(x.as_ref(), set), want, "{} L2", ctx);
    prop_assert_eq!(x.count_ones(), ones, "{} L3", ctx);
    prop_assert_eq!(reduce::count_ones2(x.as_ref()), ones, "{} L2", ctx);
    prop_assert_eq!(x.onehot(), r::onehot(&rx), "{}", ctx);
    prop_assert_eq!(reduce::onehot2(x.as_ref()), r::onehot(&rx), "{}", ctx);
    prop_assert_eq!(x.onehot0(), r::onehot0(&rx), "{}", ctx);
    prop_assert_eq!(reduce::onehot02(x.as_ref()), r::onehot0(&rx), "{}", ctx);
    if w <= 64 {
        let v = to_u64_scalar(x);
        prop_assert_eq!(reduce::count_bits2_l64(v, w, set), want, "{} L1", ctx);
        prop_assert_eq!(reduce::count_ones2_l64(v, w), ones, "{} L1", ctx);
        prop_assert_eq!(reduce::onehot2_l64(v, w), r::onehot(&rx), "{} L1", ctx);
        prop_assert_eq!(reduce::onehot02_l64(v, w), r::onehot0(&rx), "{} L1", ctx);
    }
    Ok(())
}

// ===========================================================================
// 真理値と論理演算 (§11.4.7, §12.4)
// ===========================================================================

fn check_truth(x: &LogicVec) -> Result<(), TestCaseError> {
    let w = x.width();
    let ctx = format!("truth w={w} x={x:?}");
    let rx = to_ref(x);
    let want = r::truth(&rx);
    let want_not = r::logical_not(&rx);
    let sx = SplitLogic::from_vec(x);
    prop_assert_eq!(logic_to_ref(x.truth()), want, "{} L3", ctx);
    prop_assert_eq!(
        logic_to_ref(logical::truth(sx.as_ref())),
        want,
        "{} L2",
        ctx
    );
    prop_assert_eq!(logic_to_ref(x.logical_not()), want_not, "{} ! L3", ctx);
    prop_assert_eq!(
        logic_to_ref(logical::logical_not(sx.as_ref())),
        want_not,
        "{} ! L2",
        ctx
    );
    if w <= 64 {
        let l = to_l64(x);
        prop_assert_eq!(logic_to_ref(logical::truth_l64(l, w)), want, "{} L1", ctx);
        prop_assert_eq!(
            logic_to_ref(logical::logical_not_l64(l, w)),
            want_not,
            "{} L1",
            ctx
        );
    }
    Ok(())
}

fn check_truth2(x: &BitVec) -> Result<(), TestCaseError> {
    let w = x.width();
    let ctx = format!("truth2 w={w} x={x:?}");
    let rx = bits_to_ref(x);
    let want = r::truth(&rx);
    let want_not = r::logical_not(&rx);
    prop_assert_eq!(bool_to_ref(x.truth()), want, "{} L3", ctx);
    prop_assert_eq!(bool_to_ref(logical::truth2(x.as_ref())), want, "{} L2", ctx);
    prop_assert_eq!(bool_to_ref(x.logical_not()), want_not, "{} L3", ctx);
    prop_assert_eq!(
        bool_to_ref(logical::logical_not2(x.as_ref())),
        want_not,
        "{} L2",
        ctx
    );
    if w <= 64 {
        let v = to_u64_scalar(x);
        prop_assert_eq!(bool_to_ref(logical::truth2_l64(v, w)), want, "{} L1", ctx);
        prop_assert_eq!(
            bool_to_ref(logical::logical_not2_l64(v, w)),
            want_not,
            "{} L1",
            ctx
        );
    }
    Ok(())
}

/// 論理演算 `&& || -> <->`: 各オペランド (自己決定、幅は別々) の真理値に `Logic` の演算を適用する。
fn check_logical_binary(x: &LogicVec, y: &LogicVec) -> Result<(), TestCaseError> {
    let ctx = format!("logical x={x:?} y={y:?}");
    let (rx, ry) = (to_ref(x), to_ref(y));
    let (tx, ty) = (x.truth(), y.truth());
    prop_assert_eq!(
        logic_to_ref(tx.and(ty)),
        r::logical_and(&rx, &ry),
        "{} &&",
        ctx
    );
    prop_assert_eq!(
        logic_to_ref(tx.or(ty)),
        r::logical_or(&rx, &ry),
        "{} ||",
        ctx
    );
    prop_assert_eq!(
        logic_to_ref(tx.implies(ty)),
        r::logical_implies(&rx, &ry),
        "{} ->",
        ctx
    );
    prop_assert_eq!(
        logic_to_ref(tx.equiv(ty)),
        r::logical_equiv(&rx, &ry),
        "{} <->",
        ctx
    );
    Ok(())
}

// ===========================================================================
// エッジ (§9.4.2)
// ===========================================================================

fn check_edge_lsb(old: &LogicVec, new: &LogicVec) -> Result<(), TestCaseError> {
    let ctx = format!("edge_lsb old={old:?} new={new:?}");
    let want = r::edge_lsb(&to_ref(old), &to_ref(new));
    let (so, sn) = (SplitLogic::from_vec(old), SplitLogic::from_vec(new));
    prop_assert_eq!(
        edge_to_ref(edge::edge_lsb(so.as_ref(), sn.as_ref())),
        want,
        "{} L2",
        ctx
    );
    prop_assert_eq!(
        edge_to_ref(edge::edge(old.bit(0), new.bit(0))),
        want,
        "{} bit",
        ctx
    );
    if old.width() <= 64 && new.width() <= 64 {
        prop_assert_eq!(
            edge_to_ref(edge::edge_l64(to_l64(old), to_l64(new))),
            want,
            "{} L1",
            ctx
        );
    }
    Ok(())
}

fn check_edge_lsb2(old: &BitVec, new: &BitVec) -> Result<(), TestCaseError> {
    let ctx = format!("edge_lsb2 old={old:?} new={new:?}");
    let want = r::edge_lsb(&bits_to_ref(old), &bits_to_ref(new));
    prop_assert_eq!(
        edge_to_ref(edge::edge_lsb2(old.as_ref(), new.as_ref())),
        want,
        "{}",
        ctx
    );
    prop_assert_eq!(
        edge_to_ref(edge::edge2(old.bit(0), new.bit(0))),
        want,
        "{}",
        ctx
    );
    if old.width() <= 64 && new.width() <= 64 {
        let got = edge::edge2_l64(to_u64_scalar(old), to_u64_scalar(new));
        prop_assert_eq!(edge_to_ref(got), want, "{} L1", ctx);
    }
    Ok(())
}

// ===========================================================================
// M0-TC22 網羅テスト (幅 1〜3 の全値 × 全組、plan.md §5.4)
// ===========================================================================

#[test]
fn tc22_exhaustive_bitwise_widths_1_to_3() {
    for w in 1..=3 {
        let vals = all_logic(w);
        for x in &vals {
            check_not(x).unwrap();
            check_reductions(x).unwrap();
            check_truth(x).unwrap();
            for m in 0..16u8 {
                check_counts(x, m).unwrap();
            }
            for y in &vals {
                for op in BINS {
                    check_bin(op, x, y).unwrap();
                }
                check_edge_lsb(x, y).unwrap();
            }
        }
    }
}

#[test]
fn tc22_exhaustive_bitwise_two_state_widths_1_to_4() {
    for w in 1..=4 {
        let vals = all_bits(w);
        for x in &vals {
            check_not2(x).unwrap();
            check_reductions2(x).unwrap();
            check_truth2(x).unwrap();
            for m in 0..16u8 {
                check_counts2(x, m).unwrap();
            }
            for y in &vals {
                for op in BINS {
                    check_bin2(op, x, y).unwrap();
                }
                check_edge_lsb2(x, y).unwrap();
            }
        }
    }
}

/// 論理演算の網羅: 幅 1〜2 の全値 × 全値 (幅は別々でもよい、§11.4.7 の自己決定)。
#[test]
fn tc22_exhaustive_logical_binary() {
    let vals: Vec<LogicVec> = (1..=2).flat_map(all_logic).collect();
    for x in &vals {
        for y in &vals {
            check_logical_binary(x, y).unwrap();
        }
    }
}

/// `Logic::{not, and, or, implies, equiv}` の 4×4 を参照モデルの 1 ビット演算と比べる (§11.4.7, §11.4.8)。
#[test]
fn tc22_exhaustive_logic_scalar_methods() {
    for a in Logic::ALL {
        assert_eq!(logic_to_ref(a.not()), !logic_to_ref(a), "!{a}");
        for b in Logic::ALL {
            assert_eq!(
                logic_to_ref(a.and(b)),
                logic_to_ref(a).and(logic_to_ref(b)),
                "{a}&&{b}"
            );
            assert_eq!(
                logic_to_ref(a.or(b)),
                logic_to_ref(a).or(logic_to_ref(b)),
                "{a}||{b}"
            );
            assert_eq!(
                logic_to_ref(a.implies(b)),
                logic_to_ref(a).implies(logic_to_ref(b)),
                "{a}->{b}"
            );
            assert_eq!(
                logic_to_ref(a.equiv(b)),
                logic_to_ref(a).equiv(logic_to_ref(b)),
                "{a}<->{b}"
            );
        }
    }
}

// ===========================================================================
// M0-TC23 差分テスト (proptest、plan.md §5.3)
// ===========================================================================

proptest! {
    #[test]
    fn tc23_bitwise_binary((x, y) in logic_pair()) {
        for op in BINS {
            check_bin(op, &x, &y)?;
        }
    }

    #[test]
    fn tc23_bitwise_binary_scalar((x, y) in scalar_logic_pair()) {
        for op in BINS {
            check_bin(op, &x, &y)?;
        }
    }

    #[test]
    fn tc23_bitwise_binary_two_state((x, y) in bit_pair()) {
        for op in BINS {
            check_bin2(op, &x, &y)?;
        }
    }

    #[test]
    fn tc23_bitwise_not(x in any_logic_vec()) {
        check_not(&x)?;
    }

    #[test]
    fn tc23_bitwise_not_two_state(x in any_bit_vec()) {
        check_not2(&x)?;
    }

    #[test]
    fn tc23_reductions(x in any_logic_vec()) {
        check_reductions(&x)?;
    }

    #[test]
    fn tc23_reductions_scalar(x in scalar_width().prop_flat_map(logic_vec)) {
        check_reductions(&x)?;
    }

    #[test]
    fn tc23_reductions_two_state(x in any_bit_vec()) {
        check_reductions2(&x)?;
    }

    #[test]
    fn tc23_countbits(x in any_logic_vec(), m in 0u8..16) {
        check_counts(&x, m)?;
    }

    #[test]
    fn tc23_countbits_two_state(x in any_bit_vec(), m in 0u8..16) {
        check_counts2(&x, m)?;
    }

    #[test]
    fn tc23_truth(x in any_logic_vec()) {
        check_truth(&x)?;
    }

    #[test]
    fn tc23_truth_two_state(x in any_bit_vec()) {
        check_truth2(&x)?;
    }

    #[test]
    fn tc23_logical_binary(x in any_logic_vec(), y in any_logic_vec()) {
        check_logical_binary(&x, &y)?;
    }

    #[test]
    fn tc23_edge_lsb(old in any_logic_vec(), new in any_logic_vec()) {
        check_edge_lsb(&old, &new)?;
    }

    #[test]
    fn tc23_edge_lsb_two_state(old in any_bit_vec(), new in any_bit_vec()) {
        check_edge_lsb2(&old, &new)?;
    }
}
