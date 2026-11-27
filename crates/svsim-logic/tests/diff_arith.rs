//! 算術 (`+ - * / % **`、単項 `-`、`++ --`、混在 `pow_bits`) の参照モデルとの差分テスト
//! (IEEE 1800-2023 §11.4.2, §11.4.3、plan.md §5.2, §5.3、test-plan TC28, TC34 (pow_bits))。
//!
//! 各演算について、層 3 (`LogicVec`/`BitVec`)、層 2 (プレーンが隣接しないビュー、出力先は汚しておく)、
//! 層 1 (幅 ≤ 64)、`_assign` 版、2 値版 (CR-5)、混在カーネル (CR-6) を参照モデル `svsim-logic-ref` と
//! 比べる。出力の正規化不変条件 (CR-2) と、入力が変わらないこと (CR-7) も検査する。
//!
//! - 網羅 (TC28 E): 幅 1〜6 の 2 値の全組み合わせ × 符号付き/符号なし。べき乗は指数の幅 1〜4 の全値。
//! - 差分 (TC28 D): plan.md §5.3 の戦略 (X/Z なしの比率が高い算術向けの値、除数 0/1/-1/2 のべき、
//!   指数は負・0・1・大きい値)。多ワードの除算の境界 (除数のワード数、被除数 < 除数) は局所の戦略で
//!   増やす。

mod common;

use common::*;
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;
use svsim_logic::ops::arith;
use svsim_logic::{BitVec, LogicVec, Scratch, words_for};
use svsim_logic_ref as r;
use svsim_logic_ref::RefVec;

// ===========================================================================
// ローカルの補助 (共通のものは tests/common にある)
// ===========================================================================

#[derive(Clone, Copy, Debug)]
enum Bin {
    Add,
    Sub,
    Mul,
    Div(bool),
    Rem(bool),
}

const BIN_OPS: [Bin; 7] = [
    Bin::Add,
    Bin::Sub,
    Bin::Mul,
    Bin::Div(false),
    Bin::Div(true),
    Bin::Rem(false),
    Bin::Rem(true),
];

#[derive(Clone, Copy, Debug)]
enum Un {
    Neg,
    Inc,
    Dec,
}

const UN_OPS: [Un; 3] = [Un::Neg, Un::Inc, Un::Dec];

fn ref_bin(op: Bin, x: &RefVec, y: &RefVec) -> RefVec {
    match op {
        Bin::Add => r::add(x, y),
        Bin::Sub => r::sub(x, y),
        Bin::Mul => r::mul(x, y),
        Bin::Div(s) => r::div(x, y, s),
        Bin::Rem(s) => r::rem(x, y, s),
    }
}

/// `++ --` の参照: `x ± 1` (§11.4.2)。単項 `-` は参照モデルの `neg`。
fn ref_un(op: Un, x: &RefVec) -> RefVec {
    let one = RefVec::from_u64(x.width(), 1);
    match op {
        Un::Neg => r::neg(x),
        Un::Inc => r::add(x, &one),
        Un::Dec => r::sub(x, &one),
    }
}

// ===========================================================================
// 2 項演算の全層・全版
// ===========================================================================

fn check_bin(op: Bin, x: &LogicVec, y: &LogicVec) -> Result<(), TestCaseError> {
    let w = x.width();
    let ctx = format!("{op:?} w={w} x={x:?} y={y:?}");
    let ctx = ctx.as_str();
    let (x0, y0) = (x.clone(), y.clone());
    let want = ref_bin(op, &to_ref(x), &to_ref(y));

    // 層 3
    let got = match op {
        Bin::Add => x.add(y),
        Bin::Sub => x.sub(y),
        Bin::Mul => x.mul(y),
        Bin::Div(s) => x.div(y, s),
        Bin::Rem(s) => x.rem(y, s),
    };
    check_eq_ref(got.as_ref(), &want, &format!("{ctx} L3"))?;

    // 層 3 の _assign (CR-4)
    let mut v = x.clone();
    match op {
        Bin::Add => v.add_assign(y),
        Bin::Sub => v.sub_assign(y),
        Bin::Mul => v.mul_assign(y),
        Bin::Div(s) => v.div_assign(y, s),
        Bin::Rem(s) => v.rem_assign(y, s),
    }
    check_eq_ref(v.as_ref(), &want, &format!("{ctx} L3 assign"))?;

    // 層 2 (プレーン非隣接、出力先は全 X で汚しておく)。作業領域は事前確保しないものを使い、
    // カーネルが必要に応じて伸ばせることも確かめる。
    let xs = SplitLogic::from_vec(x);
    let ys = SplitLogic::from_vec(y);
    let mut s = Scratch::new();
    let mut d = SplitLogic::dirty(w);
    match op {
        Bin::Add => arith::add(d.as_mut(), xs.as_ref(), ys.as_ref()),
        Bin::Sub => arith::sub(d.as_mut(), xs.as_ref(), ys.as_ref()),
        Bin::Mul => arith::mul(d.as_mut(), xs.as_ref(), ys.as_ref(), &mut s),
        Bin::Div(sg) => arith::div(d.as_mut(), xs.as_ref(), ys.as_ref(), sg, &mut s),
        Bin::Rem(sg) => arith::rem(d.as_mut(), xs.as_ref(), ys.as_ref(), sg, &mut s),
    }
    check_eq_ref(d.as_ref(), &want, &format!("{ctx} L2"))?;
    let mut d = SplitLogic::from_vec(x);
    match op {
        Bin::Add => arith::add_assign(d.as_mut(), ys.as_ref()),
        Bin::Sub => arith::sub_assign(d.as_mut(), ys.as_ref()),
        Bin::Mul => arith::mul_assign(d.as_mut(), ys.as_ref(), &mut s),
        Bin::Div(sg) => arith::div_assign(d.as_mut(), ys.as_ref(), sg, &mut s),
        Bin::Rem(sg) => arith::rem_assign(d.as_mut(), ys.as_ref(), sg, &mut s),
    }
    check_eq_ref(d.as_ref(), &want, &format!("{ctx} L2 assign"))?;

    // 層 1
    if w <= 64 {
        let (lx, ly) = (to_l64(x), to_l64(y));
        let got = match op {
            Bin::Add => arith::add_l64(lx, ly, w),
            Bin::Sub => arith::sub_l64(lx, ly, w),
            Bin::Mul => arith::mul_l64(lx, ly, w),
            Bin::Div(s) => arith::div_l64(lx, ly, w, s),
            Bin::Rem(s) => arith::rem_l64(lx, ly, w, s),
        };
        check_eq_ref_l64(got, w, &want, &format!("{ctx} L1"))?;
    }
    check_unchanged(&x0, x, ctx)?;
    check_unchanged(&y0, y, ctx)?;

    // 2 値版 (X/Z を 0 にした入力、CR-5)
    let (x2, y2) = (x.to_bit_vec(), y.to_bit_vec());
    check_bin2(op, &x2, &y2)
}

fn check_bin2(op: Bin, x2: &BitVec, y2: &BitVec) -> Result<(), TestCaseError> {
    let w = x2.width();
    let ctx = format!("{op:?} 2-state w={w} x={x2:?} y={y2:?}");
    let ctx = ctx.as_str();
    let (x0, y0) = (x2.clone(), y2.clone());
    let want2 = ref_two_state(&ref_bin(op, &bits_to_ref(x2), &bits_to_ref(y2)));
    let got = match op {
        Bin::Add => x2.add(y2),
        Bin::Sub => x2.sub(y2),
        Bin::Mul => x2.mul(y2),
        Bin::Div(s) => x2.div(y2, s),
        Bin::Rem(s) => x2.rem(y2, s),
    };
    check_eq_ref2(got.as_ref(), &want2, &format!("{ctx} L3"))?;
    let mut v = x2.clone();
    match op {
        Bin::Add => v.add_assign(y2),
        Bin::Sub => v.sub_assign(y2),
        Bin::Mul => v.mul_assign(y2),
        Bin::Div(s) => v.div_assign(y2, s),
        Bin::Rem(s) => v.rem_assign(y2, s),
    }
    check_eq_ref2(v.as_ref(), &want2, &format!("{ctx} L3 assign"))?;
    let xs = SplitBits::from_vec(x2);
    let ys = SplitBits::from_vec(y2);
    let mut s = Scratch::new();
    let mut d = SplitBits::dirty(w);
    match op {
        Bin::Add => arith::add2(d.as_mut(), xs.as_ref(), ys.as_ref()),
        Bin::Sub => arith::sub2(d.as_mut(), xs.as_ref(), ys.as_ref()),
        Bin::Mul => arith::mul2(d.as_mut(), xs.as_ref(), ys.as_ref(), &mut s),
        Bin::Div(sg) => arith::div2(d.as_mut(), xs.as_ref(), ys.as_ref(), sg, &mut s),
        Bin::Rem(sg) => arith::rem2(d.as_mut(), xs.as_ref(), ys.as_ref(), sg, &mut s),
    }
    check_eq_ref2(d.as_ref(), &want2, &format!("{ctx} L2"))?;
    let mut d = SplitBits::from_vec(x2);
    match op {
        Bin::Add => arith::add_assign2(d.as_mut(), ys.as_ref()),
        Bin::Sub => arith::sub_assign2(d.as_mut(), ys.as_ref()),
        Bin::Mul => arith::mul_assign2(d.as_mut(), ys.as_ref(), &mut s),
        Bin::Div(sg) => arith::div_assign2(d.as_mut(), ys.as_ref(), sg, &mut s),
        Bin::Rem(sg) => arith::rem_assign2(d.as_mut(), ys.as_ref(), sg, &mut s),
    }
    check_eq_ref2(d.as_ref(), &want2, &format!("{ctx} L2 assign"))?;
    if w <= 64 {
        let (a, b) = (to_u64_scalar(x2), to_u64_scalar(y2));
        let got = match op {
            Bin::Add => arith::add2_l64(a, b, w),
            Bin::Sub => arith::sub2_l64(a, b, w),
            Bin::Mul => arith::mul2_l64(a, b, w),
            Bin::Div(s) => arith::div2_l64(a, b, w, s),
            Bin::Rem(s) => arith::rem2_l64(a, b, w, s),
        };
        let mut v = BitVec::zero(w);
        svsim_logic::scalar::store2(got, v.as_mut());
        prop_assert_eq!(
            got & !svsim_logic::scalar::mask(w),
            0,
            "{}: L1 not normalized",
            ctx
        );
        check_eq_ref2(v.as_ref(), &want2, &format!("{ctx} L1"))?;
    }
    check_unchanged(&x0, x2, ctx)?;
    check_unchanged(&y0, y2, ctx)?;
    Ok(())
}

// ===========================================================================
// 単項演算
// ===========================================================================

fn check_un(op: Un, x: &LogicVec) -> Result<(), TestCaseError> {
    let w = x.width();
    let ctx = format!("{op:?} w={w} x={x:?}");
    let ctx = ctx.as_str();
    let x0 = x.clone();
    let want = ref_un(op, &to_ref(x));
    let got = match op {
        Un::Neg => x.neg(),
        Un::Inc => x.inc(),
        Un::Dec => x.dec(),
    };
    check_eq_ref(got.as_ref(), &want, &format!("{ctx} L3"))?;
    let mut v = x.clone();
    match op {
        Un::Neg => v.neg_assign(),
        Un::Inc => v.inc_assign(),
        Un::Dec => v.dec_assign(),
    }
    check_eq_ref(v.as_ref(), &want, &format!("{ctx} L3 assign"))?;
    let xs = SplitLogic::from_vec(x);
    let mut d = SplitLogic::dirty(w);
    match op {
        Un::Neg => arith::neg(d.as_mut(), xs.as_ref()),
        Un::Inc => arith::inc(d.as_mut(), xs.as_ref()),
        Un::Dec => arith::dec(d.as_mut(), xs.as_ref()),
    }
    check_eq_ref(d.as_ref(), &want, &format!("{ctx} L2"))?;
    let mut d = SplitLogic::from_vec(x);
    match op {
        Un::Neg => arith::neg_assign(d.as_mut()),
        Un::Inc => arith::inc_assign(d.as_mut()),
        Un::Dec => arith::dec_assign(d.as_mut()),
    }
    check_eq_ref(d.as_ref(), &want, &format!("{ctx} L2 assign"))?;
    if w <= 64 {
        let l = to_l64(x);
        let got = match op {
            Un::Neg => arith::neg_l64(l, w),
            Un::Inc => arith::inc_l64(l, w),
            Un::Dec => arith::dec_l64(l, w),
        };
        check_eq_ref_l64(got, w, &want, &format!("{ctx} L1"))?;
    }
    check_unchanged(&x0, x, ctx)?;

    // 2 値版
    let x2 = x.to_bit_vec();
    let want2 = ref_two_state(&ref_un(op, &bits_to_ref(&x2)));
    let got = match op {
        Un::Neg => x2.neg(),
        Un::Inc => x2.inc(),
        Un::Dec => x2.dec(),
    };
    check_eq_ref2(got.as_ref(), &want2, &format!("{ctx} 2-state L3"))?;
    let mut v = x2.clone();
    match op {
        Un::Neg => v.neg_assign(),
        Un::Inc => v.inc_assign(),
        Un::Dec => v.dec_assign(),
    }
    check_eq_ref2(v.as_ref(), &want2, &format!("{ctx} 2-state L3 assign"))?;
    let xs2 = SplitBits::from_vec(&x2);
    let mut d = SplitBits::dirty(w);
    match op {
        Un::Neg => arith::neg2(d.as_mut(), xs2.as_ref()),
        Un::Inc => arith::inc2(d.as_mut(), xs2.as_ref()),
        Un::Dec => arith::dec2(d.as_mut(), xs2.as_ref()),
    }
    check_eq_ref2(d.as_ref(), &want2, &format!("{ctx} 2-state L2"))?;
    let mut d = SplitBits::from_vec(&x2);
    match op {
        Un::Neg => arith::neg_assign2(d.as_mut()),
        Un::Inc => arith::inc_assign2(d.as_mut()),
        Un::Dec => arith::dec_assign2(d.as_mut()),
    }
    check_eq_ref2(d.as_ref(), &want2, &format!("{ctx} 2-state L2 assign"))?;
    if w <= 64 {
        let a = to_u64_scalar(&x2);
        let got = match op {
            Un::Neg => arith::neg2_l64(a, w),
            Un::Inc => arith::inc2_l64(a, w),
            Un::Dec => arith::dec2_l64(a, w),
        };
        let mut v = BitVec::zero(w);
        svsim_logic::scalar::store2(got, v.as_mut());
        prop_assert_eq!(got & !svsim_logic::scalar::mask(w), 0, "{}: L1", ctx);
        check_eq_ref2(v.as_ref(), &want2, &format!("{ctx} 2-state L1"))?;
    }
    Ok(())
}

// ===========================================================================
// べき乗 (4 値、2 値、混在)
// ===========================================================================

fn check_pow(base: &LogicVec, bs: bool, exp: &LogicVec, es: bool) -> Result<(), TestCaseError> {
    let w = base.width();
    let ew = exp.width();
    let ctx = format!("pow w={w} base={base:?} (S={bs}) exp={exp:?} (S={es})");
    let ctx = ctx.as_str();
    let (b0, e0) = (base.clone(), exp.clone());
    let want = r::pow(&to_ref(base), bs, &to_ref(exp), es);

    check_eq_ref(base.pow(bs, exp, es).as_ref(), &want, &format!("{ctx} L3"))?;
    let bsp = SplitLogic::from_vec(base);
    let esp = SplitLogic::from_vec(exp);
    let mut s = Scratch::new();
    let mut d = SplitLogic::dirty(w);
    arith::pow(d.as_mut(), bsp.as_ref(), bs, esp.as_ref(), es, &mut s);
    check_eq_ref(d.as_ref(), &want, &format!("{ctx} L2"))?;
    if w <= 64 && ew <= 64 {
        let got = arith::pow_l64(to_l64(base), w, bs, to_l64(exp), ew, es);
        check_eq_ref_l64(got, w, &want, &format!("{ctx} L1"))?;
    }
    check_unchanged(&b0, base, ctx)?;
    check_unchanged(&e0, exp, ctx)?;

    // 混在 (2 値 base × 4 値の指数、CR-6): 参照モデルと、「from_two_state してから 4 値カーネル」の両方。
    let b2 = base.to_bit_vec();
    let want_m = r::pow(&bits_to_ref(&b2), bs, &to_ref(exp), es);
    let via4 = LogicVec::from_two_state(&b2).pow(bs, exp, es);
    let got = b2.pow_logic(bs, exp, es);
    check_eq_ref(got.as_ref(), &want_m, &format!("{ctx} pow_logic"))?;
    check_eq(
        got.as_ref(),
        via4.as_ref(),
        &format!("{ctx} pow_logic vs 4-state"),
    )?;
    let b2s = SplitBits::from_vec(&b2);
    let mut d = SplitLogic::dirty(w);
    arith::pow_bits(d.as_mut(), b2s.as_ref(), bs, esp.as_ref(), es, &mut s);
    check_eq_ref(d.as_ref(), &want_m, &format!("{ctx} pow_bits L2"))?;
    if w <= 64 && ew <= 64 {
        let got = arith::pow_bits_l64(to_u64_scalar(&b2), w, bs, to_l64(exp), ew, es);
        check_eq_ref_l64(got, w, &want_m, &format!("{ctx} pow_bits L1"))?;
    }

    // 2 値版 (CR-5)
    let e2 = exp.to_bit_vec();
    let want2 = ref_two_state(&r::pow(&bits_to_ref(&b2), bs, &bits_to_ref(&e2), es));
    check_eq_ref2(
        b2.pow(bs, &e2, es).as_ref(),
        &want2,
        &format!("{ctx} 2-state L3"),
    )?;
    let e2s = SplitBits::from_vec(&e2);
    let mut d = SplitBits::dirty(w);
    arith::pow2(d.as_mut(), b2s.as_ref(), bs, e2s.as_ref(), es, &mut s);
    check_eq_ref2(d.as_ref(), &want2, &format!("{ctx} 2-state L2"))?;
    if w <= 64 && ew <= 64 {
        let got = arith::pow2_l64(to_u64_scalar(&b2), w, bs, to_u64_scalar(&e2), ew, es);
        let mut v = BitVec::zero(w);
        svsim_logic::scalar::store2(got, v.as_mut());
        prop_assert_eq!(got & !svsim_logic::scalar::mask(w), 0, "{}: L1", ctx);
        check_eq_ref2(v.as_ref(), &want2, &format!("{ctx} 2-state L1"))?;
    }
    Ok(())
}

// ===========================================================================
// 網羅 (TC28 E): 幅 1〜6 の 2 値の全組み合わせ
// ===========================================================================

#[test]
fn exhaustive_binary_small_widths() {
    for w in 1u32..=6 {
        for a in 0..(1u64 << w) {
            for b in 0..(1u64 << w) {
                let (x, y) = (LogicVec::from_u64(w, a), LogicVec::from_u64(w, b));
                for op in BIN_OPS {
                    run_case(check_bin(op, &x, &y));
                }
            }
        }
    }
}

#[test]
fn exhaustive_unary_small_widths() {
    for w in 1u32..=6 {
        for a in 0..(1u64 << w) {
            let x = LogicVec::from_u64(w, a);
            for op in UN_OPS {
                run_case(check_un(op, &x));
            }
        }
    }
}

/// べき乗: base の幅 1〜6 の全値 × 指数の幅 1〜4 の全値 × 符号付き/符号なしの 4 通り。
#[test]
fn exhaustive_pow_small_widths() {
    for w in 1u32..=6 {
        for a in 0..(1u64 << w) {
            let base = LogicVec::from_u64(w, a);
            for ew in 1u32..=4 {
                for e in 0..(1u64 << ew) {
                    let exp = LogicVec::from_u64(ew, e);
                    for (bs, es) in [(false, false), (false, true), (true, false), (true, true)] {
                        run_case(check_pow(&base, bs, &exp, es));
                    }
                }
            }
        }
    }
}

// ===========================================================================
// 差分 (TC28 D、proptest)
// ===========================================================================

/// 有効なワード数を絞った 2 値の値 (多倍長の除算の境界: 除数 1 ワード / 多ワード、被除数 < 除数)。
fn short_known(w: u32) -> BoxedStrategy<LogicVec> {
    let nw = words_for(w);
    (logic_vec_known(w), 0..=nw, any::<bool>())
        .prop_map(move |(v, k, negate)| {
            // 上位の nw - k ワードを 0 にする。
            let mut bits: Vec<char> = v.to_bits_string().chars().collect();
            let keep = (64 * k).min(w as usize);
            let n = bits.len();
            for c in bits.iter_mut().take(n - keep) {
                *c = '0';
            }
            let s: String = bits.into_iter().collect();
            let v = LogicVec::from_bits_str(&s).unwrap();
            // 符号付きの除算で、絶対値のワード数が少ない負の値も作る。
            if negate { v.neg() } else { v }
        })
        .boxed()
}

/// 除算のオペランドの組: 被除数は算術向けの値か短い値、除数は common の除数か短い値。
fn div_pair() -> impl Strategy<Value = (LogicVec, LogicVec)> {
    width().prop_flat_map(|w| {
        (
            prop_oneof![logic_vec_arith(w), short_known(w)],
            prop_oneof![divisor(w), short_known(w), logic_vec_arith(w)],
        )
    })
}

proptest! {
    #[test]
    fn diff_add_sub_mul((x, y) in logic_pair_arith()) {
        for op in [Bin::Add, Bin::Sub, Bin::Mul] {
            check_bin(op, &x, &y)?;
        }
    }

    #[test]
    fn diff_div_rem((x, y) in div_pair()) {
        for op in [Bin::Div(false), Bin::Div(true), Bin::Rem(false), Bin::Rem(true)] {
            check_bin(op, &x, &y)?;
        }
    }

    /// 2 値の除算 (除数に 0 を多く含む、0 除算は 0 [V-2])。
    #[test]
    fn diff_div_rem_two_state(
        (x, y) in width().prop_flat_map(|w| (bit_vec(w), divisor_bits(w)))
    ) {
        for op in [Bin::Div(false), Bin::Div(true), Bin::Rem(false), Bin::Rem(true)] {
            check_bin2(op, &x, &y)?;
        }
    }

    #[test]
    fn diff_unary(x in width().prop_flat_map(logic_vec_arith)) {
        for op in UN_OPS {
            check_un(op, &x)?;
        }
    }

    #[test]
    fn diff_pow(
        (base, exp, bs, es) in width().prop_flat_map(|w| {
            (logic_vec_arith(w), exponent(), any::<bool>(), any::<bool>())
        })
    ) {
        check_pow(&base, bs, &exp, es)?;
    }

    /// base を ±1、0、2、奇数の小さい値に寄せたべき乗 (特殊ケースと、奇数の base での指数の縮約)。
    #[test]
    fn diff_pow_special_bases(
        (base, exp, bs, es) in width().prop_flat_map(|w| {
            let base = prop_oneof![
                Just(LogicVec::zero(w)),
                Just(LogicVec::from_u64(w, 1)),
                Just(LogicVec::from_i64(w, -1)),
                Just(LogicVec::from_u64(w, 2)),
                Just(LogicVec::from_i64(w, -2)),
                (0u64..50).prop_map(move |v| LogicVec::from_u64(w, 2 * v + 1)),
            ];
            (base, exponent(), any::<bool>(), any::<bool>())
        })
    ) {
        check_pow(&base, bs, &exp, es)?;
    }

    /// 性質 (TC26 P): X/Z なし、除数 ≠ 0 なら x == (x/y)*y + x%y (幅 w の剰余環で)。
    /// 符号付きでは |x % y| < |y|、x % y の符号は 0 か x と同じ。
    #[test]
    fn prop_div_rem_identity((x, y) in div_pair(), signed in any::<bool>()) {
        let (x, y) = (x.to_bit_vec(), y.to_bit_vec());
        // 除数 0 は 1 に置き換える (prop_assume で捨てると棄却が多すぎるため)。
        let y = if y.to_u64() == Some(0) { BitVec::from_u64(y.width(), 1) } else { y };
        let q = x.div(&y, signed);
        let rm = x.rem(&y, signed);
        prop_assert_eq!(q.mul(&y).add(&rm), x.clone(), "q*y + r == x");
        let w = x.width();
        let abs = |v: &BitVec| if signed && v.bit(w - 1) { v.neg() } else { v.clone() };
        let (ar, ay) = (abs(&rm), abs(&y));
        // 符号なしとして |r| < |y| (最小値の絶対値も符号なしの w ビットで表せる)。
        let lt = svsim_logic::ops::compare::lt2(ar.as_ref(), ay.as_ref(), false);
        prop_assert!(lt, "|r| < |y|");
        if signed && rm.to_u64() != Some(0) {
            prop_assert_eq!(rm.bit(w - 1), x.bit(w - 1), "sign(r) == sign(x)");
        }
    }
}
