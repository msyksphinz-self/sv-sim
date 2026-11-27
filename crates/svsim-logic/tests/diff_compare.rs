//! M0-T4: 等価・比較・ワイルドカード・casez/casex・条件演算子のマージの差分テスト。
//!
//! 参照モデル `svsim-logic-ref` と、層 1 (幅 ≤ 64)、層 2 (プレーンが隣接しないビュー)、
//! 層 3 (`LogicVec` / `BitVec`)、2 値版、混在カーネル `cond_merge_bits` を比べる
//! (plan.md §5.2、test-plan CR-1〜CR-7)。
//!
//! - M0-TC22: 幅 1〜3 の 4 値の全値 × 全組の網羅テスト (plan.md §5.4)。
//! - M0-TC23: proptest の差分テスト (plan.md §5.3 の戦略)。
//! - M0-TC34 の一部: `cond_merge_bits` が「`from_two_state` してから `cond_merge`」と一致する (CR-6)。
//!
//! 条番号: 等価 IEEE 1800-2023 §11.4.5、ワイルドカード §11.4.6、casez/casex §12.5.1、
//! 関係 §11.4.4 / §11.8.1、条件演算子 §11.4.11。

mod common;

use common::*;
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;
use svsim_logic::ops::{compare, cond};
use svsim_logic::{BitRef, BitVec, L64, Logic, LogicRef, LogicVec};
use svsim_logic_ref as r;
use svsim_logic_ref::{RefLogic, RefVec};

fn rl(v: Logic) -> RefLogic {
    match v {
        Logic::L0 => RefLogic::Zero,
        Logic::L1 => RefLogic::One,
        Logic::X => RefLogic::X,
        Logic::Z => RefLogic::Z,
    }
}

fn rb(b: bool) -> RefLogic {
    RefLogic::from_bool(b)
}

fn all_logic(w: u32) -> Vec<LogicVec> {
    (0..4usize.pow(w))
        .map(|mut n| {
            let mut v = LogicVec::zero(w);
            for i in 0..w {
                v.set_bit(i, Logic::ALL[n % 4]);
                n /= 4;
            }
            v
        })
        .collect()
}

fn all_bits(w: u32) -> Vec<BitVec> {
    (0..1u64 << w).map(|n| BitVec::from_u64(w, n)).collect()
}

// ===========================================================================
// 等価系 (§11.4.5, §11.4.6, §12.5.1)
// ===========================================================================

#[derive(Clone, Copy, Debug)]
enum Eqv {
    Eq,
    Ne,
    CaseEq,
    CaseNe,
    WildEq,
    WildNe,
    Casez,
    Casex,
}

const EQVS: [Eqv; 8] = [
    Eqv::Eq,
    Eqv::Ne,
    Eqv::CaseEq,
    Eqv::CaseNe,
    Eqv::WildEq,
    Eqv::WildNe,
    Eqv::Casez,
    Eqv::Casex,
];

impl Eqv {
    fn reference(self, x: &RefVec, y: &RefVec) -> RefLogic {
        match self {
            Eqv::Eq => r::eq(x, y),
            Eqv::Ne => r::ne(x, y),
            Eqv::CaseEq => rb(r::case_eq(x, y)),
            Eqv::CaseNe => rb(r::case_ne(x, y)),
            Eqv::WildEq => r::wildcard_eq(x, y),
            Eqv::WildNe => r::wildcard_ne(x, y),
            Eqv::Casez => rb(r::casez_match(x, y)),
            Eqv::Casex => rb(r::casex_match(x, y)),
        }
    }
    fn l3(self, x: &LogicVec, y: &LogicVec) -> RefLogic {
        match self {
            Eqv::Eq => rl(x.eq(y)),
            Eqv::Ne => rl(x.ne(y)),
            Eqv::CaseEq => rb(x.case_eq(y)),
            Eqv::CaseNe => rb(x.case_ne(y)),
            Eqv::WildEq => rl(x.wildcard_eq(y)),
            Eqv::WildNe => rl(x.wildcard_ne(y)),
            Eqv::Casez => rb(x.casez_match(y)),
            Eqv::Casex => rb(x.casex_match(y)),
        }
    }
    fn l2(self, x: LogicRef<'_>, y: LogicRef<'_>) -> RefLogic {
        match self {
            Eqv::Eq => rl(compare::eq(x, y)),
            Eqv::Ne => rl(compare::ne(x, y)),
            Eqv::CaseEq => rb(compare::case_eq(x, y)),
            Eqv::CaseNe => rb(compare::case_ne(x, y)),
            Eqv::WildEq => rl(compare::wildcard_eq(x, y)),
            Eqv::WildNe => rl(compare::wildcard_ne(x, y)),
            Eqv::Casez => rb(compare::casez_match(x, y)),
            Eqv::Casex => rb(compare::casex_match(x, y)),
        }
    }
    fn l1(self, x: L64, y: L64, w: u32) -> RefLogic {
        match self {
            Eqv::Eq => rl(compare::eq_l64(x, y, w)),
            Eqv::Ne => rl(compare::ne_l64(x, y, w)),
            Eqv::CaseEq => rb(compare::case_eq_l64(x, y, w)),
            Eqv::CaseNe => rb(compare::case_ne_l64(x, y, w)),
            Eqv::WildEq => rl(compare::wildcard_eq_l64(x, y, w)),
            Eqv::WildNe => rl(compare::wildcard_ne_l64(x, y, w)),
            Eqv::Casez => rb(compare::casez_match_l64(x, y, w)),
            Eqv::Casex => rb(compare::casex_match_l64(x, y, w)),
        }
    }
    fn b3(self, x: &BitVec, y: &BitVec) -> bool {
        match self {
            Eqv::Eq => x.eq(y),
            Eqv::Ne => x.ne(y),
            Eqv::CaseEq => x.case_eq(y),
            Eqv::CaseNe => x.case_ne(y),
            Eqv::WildEq => x.wildcard_eq(y),
            Eqv::WildNe => x.wildcard_ne(y),
            Eqv::Casez => x.casez_match(y),
            Eqv::Casex => x.casex_match(y),
        }
    }
    fn b2(self, x: BitRef<'_>, y: BitRef<'_>) -> bool {
        match self {
            Eqv::Eq => compare::eq2(x, y),
            Eqv::Ne => compare::ne2(x, y),
            Eqv::CaseEq => compare::case_eq2(x, y),
            Eqv::CaseNe => compare::case_ne2(x, y),
            Eqv::WildEq => compare::wildcard_eq2(x, y),
            Eqv::WildNe => compare::wildcard_ne2(x, y),
            Eqv::Casez => compare::casez_match2(x, y),
            Eqv::Casex => compare::casex_match2(x, y),
        }
    }
    /// 2 値の層 1 は `eq2_l64` / `ne2_l64` だけ (他の等価は 2 値では同じ)。
    fn b1(self, x: u64, y: u64, w: u32) -> bool {
        match self {
            Eqv::Ne | Eqv::CaseNe | Eqv::WildNe => compare::ne2_l64(x, y, w),
            _ => compare::eq2_l64(x, y, w),
        }
    }
}

fn check_eqv(x: &LogicVec, y: &LogicVec) -> Result<(), TestCaseError> {
    let w = x.width();
    let (rx, ry) = (to_ref(x), to_ref(y));
    let (sx, sy) = (SplitLogic::from_vec(x), SplitLogic::from_vec(y));
    let (x0, y0) = (x.clone(), y.clone());
    for op in EQVS {
        let ctx = format!("{op:?} w={w} x={x:?} y={y:?}");
        let want = op.reference(&rx, &ry);
        prop_assert_eq!(op.l3(x, y), want, "{} L3", ctx);
        prop_assert_eq!(op.l2(sx.as_ref(), sy.as_ref()), want, "{} L2", ctx);
        if w <= 64 {
            prop_assert_eq!(op.l1(to_l64(x), to_l64(y), w), want, "{} L1", ctx);
        }
    }
    prop_assert_eq!(x, &x0);
    prop_assert_eq!(y, &y0);
    Ok(())
}

fn check_eqv2(x: &BitVec, y: &BitVec) -> Result<(), TestCaseError> {
    let w = x.width();
    let (rx, ry) = (bits_to_ref(x), bits_to_ref(y));
    let (sx, sy) = (SplitBits::from_vec(x), SplitBits::from_vec(y));
    for op in EQVS {
        let ctx = format!("{op:?}2 w={w} x={x:?} y={y:?}");
        let want = op.reference(&rx, &ry);
        prop_assert_eq!(rb(op.b3(x, y)), want, "{} L3", ctx);
        prop_assert_eq!(rb(op.b2(sx.as_ref(), sy.as_ref())), want, "{} L2", ctx);
        if w <= 64 {
            let got = op.b1(to_u64_scalar(x), to_u64_scalar(y), w);
            prop_assert_eq!(rb(got), want, "{} L1", ctx);
        }
    }
    Ok(())
}

// ===========================================================================
// 関係演算 (§11.4.4, §11.8.1)
// ===========================================================================

#[derive(Clone, Copy, Debug)]
enum Rel {
    Lt,
    Le,
    Gt,
    Ge,
}

const RELS: [Rel; 4] = [Rel::Lt, Rel::Le, Rel::Gt, Rel::Ge];

impl Rel {
    fn reference(self, x: &RefVec, y: &RefVec, s: bool) -> RefLogic {
        match self {
            Rel::Lt => r::lt(x, y, s),
            Rel::Le => r::le(x, y, s),
            Rel::Gt => r::gt(x, y, s),
            Rel::Ge => r::ge(x, y, s),
        }
    }
    fn l3(self, x: &LogicVec, y: &LogicVec, s: bool) -> Logic {
        match self {
            Rel::Lt => x.lt(y, s),
            Rel::Le => x.le(y, s),
            Rel::Gt => x.gt(y, s),
            Rel::Ge => x.ge(y, s),
        }
    }
    fn l2(self, x: LogicRef<'_>, y: LogicRef<'_>, s: bool) -> Logic {
        match self {
            Rel::Lt => compare::lt(x, y, s),
            Rel::Le => compare::le(x, y, s),
            Rel::Gt => compare::gt(x, y, s),
            Rel::Ge => compare::ge(x, y, s),
        }
    }
    fn l1(self, x: L64, y: L64, w: u32, s: bool) -> Logic {
        match self {
            Rel::Lt => compare::lt_l64(x, y, w, s),
            Rel::Le => compare::le_l64(x, y, w, s),
            Rel::Gt => compare::gt_l64(x, y, w, s),
            Rel::Ge => compare::ge_l64(x, y, w, s),
        }
    }
    fn b3(self, x: &BitVec, y: &BitVec, s: bool) -> bool {
        match self {
            Rel::Lt => x.lt(y, s),
            Rel::Le => x.le(y, s),
            Rel::Gt => x.gt(y, s),
            Rel::Ge => x.ge(y, s),
        }
    }
    fn b2(self, x: BitRef<'_>, y: BitRef<'_>, s: bool) -> bool {
        match self {
            Rel::Lt => compare::lt2(x, y, s),
            Rel::Le => compare::le2(x, y, s),
            Rel::Gt => compare::gt2(x, y, s),
            Rel::Ge => compare::ge2(x, y, s),
        }
    }
    fn b1(self, x: u64, y: u64, w: u32, s: bool) -> bool {
        match self {
            Rel::Lt => compare::lt2_l64(x, y, w, s),
            Rel::Le => compare::le2_l64(x, y, w, s),
            Rel::Gt => compare::gt2_l64(x, y, w, s),
            Rel::Ge => compare::ge2_l64(x, y, w, s),
        }
    }
}

fn check_rel(x: &LogicVec, y: &LogicVec) -> Result<(), TestCaseError> {
    let w = x.width();
    let (rx, ry) = (to_ref(x), to_ref(y));
    let (sx, sy) = (SplitLogic::from_vec(x), SplitLogic::from_vec(y));
    for s in [false, true] {
        for op in RELS {
            let ctx = format!("{op:?} signed={s} w={w} x={x:?} y={y:?}");
            let want = op.reference(&rx, &ry, s);
            prop_assert_eq!(rl(op.l3(x, y, s)), want, "{} L3", ctx);
            prop_assert_eq!(rl(op.l2(sx.as_ref(), sy.as_ref(), s)), want, "{} L2", ctx);
            if w <= 64 {
                prop_assert_eq!(rl(op.l1(to_l64(x), to_l64(y), w, s)), want, "{} L1", ctx);
            }
        }
    }
    Ok(())
}

fn check_rel2(x: &BitVec, y: &BitVec) -> Result<(), TestCaseError> {
    let w = x.width();
    let (rx, ry) = (bits_to_ref(x), bits_to_ref(y));
    for s in [false, true] {
        for op in RELS {
            let ctx = format!("{op:?}2 signed={s} w={w} x={x:?} y={y:?}");
            let want = op.reference(&rx, &ry, s);
            prop_assert_eq!(rb(op.b3(x, y, s)), want, "{} L3", ctx);
            prop_assert_eq!(rb(op.b2(x.as_ref(), y.as_ref(), s)), want, "{} L2", ctx);
            if w <= 64 {
                let got = op.b1(to_u64_scalar(x), to_u64_scalar(y), w, s);
                prop_assert_eq!(rb(got), want, "{} L1", ctx);
            }
        }
    }
    Ok(())
}

// ===========================================================================
// 条件演算子のマージ (§11.4.11) と混在カーネル (plan.md §3.5.1)
// ===========================================================================

fn check_cond_merge(t: &LogicVec, f: &LogicVec) -> Result<(), TestCaseError> {
    let w = t.width();
    let ctx = format!("cond_merge w={w} t={t:?} f={f:?}");
    let want = r::cond_merge(&to_ref(t), &to_ref(f));
    let (t0, f0) = (t.clone(), f.clone());
    check_eq_ref(t.cond_merge(f).as_ref(), &want, &format!("{ctx} L3"))?;
    let (st, sf) = (SplitLogic::from_vec(t), SplitLogic::from_vec(f));
    let mut d = SplitLogic::dirty(w);
    cond::cond_merge(d.as_mut(), st.as_ref(), sf.as_ref());
    check_eq_ref(d.as_ref(), &want, &format!("{ctx} L2"))?;
    if w <= 64 {
        let got = cond::cond_merge_l64(to_l64(t), to_l64(f), w);
        check_eq_ref_l64(got, w, &want, &format!("{ctx} L1"))?;
    }
    prop_assert_eq!(t, &t0);
    prop_assert_eq!(f, &f0);
    Ok(())
}

/// 混在カーネル `cond_merge_bits` (CR-6): 参照モデルと、「`from_two_state` してから `cond_merge`」の両方。
fn check_cond_merge_bits(t: &BitVec, f: &BitVec) -> Result<(), TestCaseError> {
    let w = t.width();
    let ctx = format!("cond_merge_bits w={w} t={t:?} f={f:?}");
    let want = r::cond_merge(&bits_to_ref(t), &bits_to_ref(f));
    let via4 = LogicVec::from(t).cond_merge(&LogicVec::from(f));
    check_eq_ref(via4.as_ref(), &want, &format!("{ctx} via from_two_state"))?;

    let l3 = t.cond_merge_logic(f);
    check_eq_ref(l3.as_ref(), &want, &format!("{ctx} L3"))?;
    check_eq(l3.as_ref(), via4.as_ref(), &format!("{ctx} L3 == via4"))?;

    let (st, sf) = (SplitBits::from_vec(t), SplitBits::from_vec(f));
    let mut d = SplitLogic::dirty(w);
    cond::cond_merge_bits(d.as_mut(), st.as_ref(), sf.as_ref());
    check_eq_ref(d.as_ref(), &want, &format!("{ctx} L2"))?;
    if w <= 64 {
        let got = cond::cond_merge_bits_l64(to_u64_scalar(t), to_u64_scalar(f), w);
        check_eq_ref_l64(got, w, &want, &format!("{ctx} L1"))?;
    }
    Ok(())
}

// ===========================================================================
// M0-TC22 網羅テスト
// ===========================================================================

#[test]
fn tc22_exhaustive_compare_widths_1_to_3() {
    for w in 1..=3 {
        let vals = all_logic(w);
        for x in &vals {
            for y in &vals {
                check_eqv(x, y).unwrap();
                check_rel(x, y).unwrap();
                check_cond_merge(x, y).unwrap();
            }
        }
    }
}

#[test]
fn tc22_exhaustive_compare_two_state_widths_1_to_4() {
    for w in 1..=4 {
        let vals = all_bits(w);
        for x in &vals {
            for y in &vals {
                check_eqv2(x, y).unwrap();
                check_rel2(x, y).unwrap();
                check_cond_merge_bits(x, y).unwrap();
            }
        }
    }
}

// ===========================================================================
// M0-TC23 差分テスト
// ===========================================================================

/// 等価系で意味のある入力を増やす: `y` を `x` から作る (一致、1 ビットだけ変える、x/z を混ぜる)。
fn near_pair() -> impl Strategy<Value = (LogicVec, LogicVec)> {
    width().prop_flat_map(|w| {
        (logic_vec(w), any::<u32>(), 0usize..4, 0usize..4).prop_map(move |(x, i, kind, val)| {
            let mut y = x.clone();
            let i = i % w;
            match kind {
                0 => {}
                _ => y.set_bit(i, Logic::ALL[val]),
            }
            (x, y)
        })
    })
}

/// 2 値の大小比較で意味のある入力 (上位ワードだけ違う、符号ビットだけ違う、同値)。
fn near_bit_pair() -> impl Strategy<Value = (BitVec, BitVec)> {
    width().prop_flat_map(|w| {
        (bit_vec(w), any::<u32>(), any::<bool>()).prop_map(move |(x, i, flip)| {
            let mut y = x.clone();
            if flip {
                let i = i % w;
                y.set_bit(i, !y.bit(i));
            }
            (x, y)
        })
    })
}

proptest! {
    #[test]
    fn tc23_equality((x, y) in logic_pair()) {
        check_eqv(&x, &y)?;
    }

    #[test]
    fn tc23_equality_near((x, y) in near_pair()) {
        check_eqv(&x, &y)?;
        check_eqv(&y, &x)?;
    }

    #[test]
    fn tc23_equality_scalar((x, y) in scalar_logic_pair()) {
        check_eqv(&x, &y)?;
    }

    #[test]
    fn tc23_equality_two_state((x, y) in bit_pair()) {
        check_eqv2(&x, &y)?;
    }

    #[test]
    fn tc23_equality_two_state_near((x, y) in near_bit_pair()) {
        check_eqv2(&x, &y)?;
    }

    #[test]
    fn tc23_relational((x, y) in logic_pair_arith()) {
        check_rel(&x, &y)?;
    }

    #[test]
    fn tc23_relational_scalar((x, y) in scalar_width().prop_flat_map(|w| (logic_vec_arith(w), logic_vec_arith(w)))) {
        check_rel(&x, &y)?;
    }

    #[test]
    fn tc23_relational_two_state((x, y) in bit_pair()) {
        check_rel2(&x, &y)?;
    }

    #[test]
    fn tc23_relational_two_state_near((x, y) in near_bit_pair()) {
        check_rel2(&x, &y)?;
        check_rel2(&y, &x)?;
    }

    #[test]
    fn tc23_cond_merge((t, f) in logic_pair()) {
        check_cond_merge(&t, &f)?;
    }

    #[test]
    fn tc23_cond_merge_near((t, f) in near_pair()) {
        check_cond_merge(&t, &f)?;
    }

    #[test]
    fn tc23_cond_merge_scalar((t, f) in scalar_logic_pair()) {
        check_cond_merge(&t, &f)?;
    }

    #[test]
    fn tc23_cond_merge_bits((t, f) in bit_pair()) {
        check_cond_merge_bits(&t, &f)?;
    }

    #[test]
    fn tc23_cond_merge_bits_near((t, f) in near_bit_pair()) {
        check_cond_merge_bits(&t, &f)?;
    }
}
