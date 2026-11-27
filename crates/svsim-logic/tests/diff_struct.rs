//! 構造演算 (シフト、拡張・切り詰め、2 値/4 値の変換、連結、複製、選択、インデックス、`$clog2`) の
//! 参照モデルとの差分テスト (plan.md §5.2, §5.3、test-plan TC30, TC34 (シフトの混在), TC35, TC36,
//! TC52)。
//!
//! 各演算について、層 3 (`LogicVec`/`BitVec`)、層 2 (プレーンが隣接しないビュー)、層 1 (幅 ≤ 64)、
//! `_assign` 版、`_const` 版、2 値版、混在カーネル (§3.5.1) を、参照モデル `svsim-logic-ref` と比べる。
//! 出力の正規化不変条件 (CR-2) と入力が変わらないこと (CR-7) も検査する。

mod common;

use common::*;
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;
use svsim_logic::ops::select::Index;
use svsim_logic::ops::{concat, math, resize, select, shift};
use svsim_logic::{BitVec, Logic, LogicVec};
use svsim_logic_ref as r;
use svsim_logic_ref::{RefIndex, RefVec};

// ===========================================================================
// ローカルの補助 (共通のものは tests/common にある)
// ===========================================================================

fn ref_index(i: Index) -> RefIndex {
    match i {
        Index::Unknown => RefIndex::Unknown,
        Index::Value(v) => RefIndex::Value(v),
    }
}

/// 4 値のシフト量を `u64` に読む (既知のときだけ)。64 ビットを超えれば `u64::MAX`。
fn amount_u64(amt: &LogicVec) -> Option<u64> {
    if amt.has_unknown() {
        return None;
    }
    Some(amt.to_u64().unwrap_or(u64::MAX))
}

fn amount2_u64(amt: &BitVec) -> u64 {
    amt.to_u64().unwrap_or(u64::MAX)
}

#[derive(Clone, Copy, Debug)]
enum ShiftOp {
    Shl,
    Shr,
    Ashr(bool),
}

const SHIFT_OPS: [ShiftOp; 4] = [
    ShiftOp::Shl,
    ShiftOp::Shr,
    ShiftOp::Ashr(true),
    ShiftOp::Ashr(false),
];

fn ref_shift(op: ShiftOp, x: &RefVec, amt: &RefVec) -> RefVec {
    match op {
        ShiftOp::Shl => r::shl(x, amt),
        ShiftOp::Shr => r::shr(x, amt),
        ShiftOp::Ashr(s) => r::ashr(x, amt, s),
    }
}

/// シフトの全層・全版を参照モデルと比べる (TC30, TC34)。
fn check_shift(op: ShiftOp, x: &LogicVec, amt: &LogicVec) -> Result<(), TestCaseError> {
    let ctx = format!("{op:?} w={} amt={amt:?}", x.width());
    let ctx = ctx.as_str();
    let w = x.width();
    let (x0, amt0) = (x.clone(), amt.clone());
    let want = ref_shift(op, &to_ref(x), &to_ref(amt));

    // 層 3
    let got = match op {
        ShiftOp::Shl => x.shl(amt),
        ShiftOp::Shr => x.shr(amt),
        ShiftOp::Ashr(s) => x.ashr(amt, s),
    };
    check_eq_ref(got.as_ref(), &want, &format!("{ctx} L3"))?;

    // 層 2 (プレーン非隣接、出力先は全 X で汚しておく)
    let xs = SplitLogic::from_vec(x);
    let amts = SplitLogic::from_vec(amt);
    let mut d = SplitLogic::dirty(w);
    match op {
        ShiftOp::Shl => shift::shl(d.as_mut(), xs.as_ref(), amts.as_ref()),
        ShiftOp::Shr => shift::shr(d.as_mut(), xs.as_ref(), amts.as_ref()),
        ShiftOp::Ashr(s) => shift::ashr(d.as_mut(), xs.as_ref(), amts.as_ref(), s),
    }
    check_eq_ref(d.as_ref(), &want, &format!("{ctx} L2"))?;

    // _assign (CR-4)
    let mut v = x.clone();
    match op {
        ShiftOp::Shl => v.shl_assign(amt),
        ShiftOp::Shr => v.shr_assign(amt),
        ShiftOp::Ashr(s) => v.ashr_assign(amt, s),
    }
    check_eq_ref(v.as_ref(), &want, &format!("{ctx} L3 assign"))?;
    let mut s2 = SplitLogic::from_vec(x);
    match op {
        ShiftOp::Shl => shift::shl_assign(s2.as_mut(), amts.as_ref()),
        ShiftOp::Shr => shift::shr_assign(s2.as_mut(), amts.as_ref()),
        ShiftOp::Ashr(s) => shift::ashr_assign(s2.as_mut(), amts.as_ref(), s),
    }
    check_eq_ref(s2.as_ref(), &want, &format!("{ctx} L2 assign"))?;

    // _const (シフト量が既知のとき)
    if let Some(n) = amount_u64(amt) {
        let got = match op {
            ShiftOp::Shl => x.shl_const(n),
            ShiftOp::Shr => x.shr_const(n),
            ShiftOp::Ashr(s) => x.ashr_const(n, s),
        };
        check_eq_ref(got.as_ref(), &want, &format!("{ctx} L3 const"))?;
        let mut d = SplitLogic::dirty(w);
        match op {
            ShiftOp::Shl => shift::shl_const(d.as_mut(), xs.as_ref(), n),
            ShiftOp::Shr => shift::shr_const(d.as_mut(), xs.as_ref(), n),
            ShiftOp::Ashr(s) => shift::ashr_const(d.as_mut(), xs.as_ref(), n, s),
        }
        check_eq_ref(d.as_ref(), &want, &format!("{ctx} L2 const"))?;
        let mut s3 = SplitLogic::from_vec(x);
        match op {
            ShiftOp::Shl => shift::shl_const_assign(s3.as_mut(), n),
            ShiftOp::Shr => shift::shr_const_assign(s3.as_mut(), n),
            ShiftOp::Ashr(s) => shift::ashr_const_assign(s3.as_mut(), n, s),
        }
        check_eq_ref(s3.as_ref(), &want, &format!("{ctx} L2 const assign"))?;
        if w <= 64 {
            let got = match op {
                ShiftOp::Shl => shift::shl_const_l64(to_l64(x), w, n),
                ShiftOp::Shr => shift::shr_const_l64(to_l64(x), w, n),
                ShiftOp::Ashr(s) => shift::ashr_const_l64(to_l64(x), w, n, s),
            };
            check_eq_ref_l64(got, w, &want, &format!("{ctx} L1 const"))?;
        }
    }

    // 層 1 (データもシフト量も幅 ≤ 64)
    if w <= 64 && amt.width() <= 64 {
        let a = to_l64(amt);
        let got = match op {
            ShiftOp::Shl => shift::shl_l64(to_l64(x), w, a),
            ShiftOp::Shr => shift::shr_l64(to_l64(x), w, a),
            ShiftOp::Ashr(s) => shift::ashr_l64(to_l64(x), w, a, s),
        };
        check_eq_ref_l64(got, w, &want, &format!("{ctx} L1"))?;
    }

    // 2 値版 (X/Z を含まない入力、CR-5)
    let x2 = x.to_bit_vec();
    let amt2 = amt.to_bit_vec();
    let want2 = ref_two_state(&ref_shift(op, &bits_to_ref(&x2), &bits_to_ref(&amt2)));
    let got = match op {
        ShiftOp::Shl => x2.shl(&amt2),
        ShiftOp::Shr => x2.shr(&amt2),
        ShiftOp::Ashr(s) => x2.ashr(&amt2, s),
    };
    check_eq_ref2(got.as_ref(), &want2, &format!("{ctx} 2-state L3"))?;
    let x2s = SplitBits::from_vec(&x2);
    let amt2s = SplitBits::from_vec(&amt2);
    let mut d2 = SplitBits::dirty(w);
    match op {
        ShiftOp::Shl => shift::shl2(d2.as_mut(), x2s.as_ref(), amt2s.as_ref()),
        ShiftOp::Shr => shift::shr2(d2.as_mut(), x2s.as_ref(), amt2s.as_ref()),
        ShiftOp::Ashr(s) => shift::ashr2(d2.as_mut(), x2s.as_ref(), amt2s.as_ref(), s),
    }
    check_eq_ref2(d2.as_ref(), &want2, &format!("{ctx} 2-state L2"))?;
    let mut d2a = SplitBits::from_vec(&x2);
    match op {
        ShiftOp::Shl => shift::shl_assign2(d2a.as_mut(), amt2s.as_ref()),
        ShiftOp::Shr => shift::shr_assign2(d2a.as_mut(), amt2s.as_ref()),
        ShiftOp::Ashr(s) => shift::ashr_assign2(d2a.as_mut(), amt2s.as_ref(), s),
    }
    check_eq_ref2(d2a.as_ref(), &want2, &format!("{ctx} 2-state L2 assign"))?;
    let n2 = amount2_u64(&amt2);
    let got = match op {
        ShiftOp::Shl => x2.shl_const(n2),
        ShiftOp::Shr => x2.shr_const(n2),
        ShiftOp::Ashr(s) => x2.ashr_const(n2, s),
    };
    check_eq_ref2(got.as_ref(), &want2, &format!("{ctx} 2-state L3 const"))?;
    let mut d2c = SplitBits::dirty(w);
    match op {
        ShiftOp::Shl => shift::shl_const2(d2c.as_mut(), x2s.as_ref(), n2),
        ShiftOp::Shr => shift::shr_const2(d2c.as_mut(), x2s.as_ref(), n2),
        ShiftOp::Ashr(s) => shift::ashr_const2(d2c.as_mut(), x2s.as_ref(), n2, s),
    }
    check_eq_ref2(d2c.as_ref(), &want2, &format!("{ctx} 2-state L2 const"))?;
    let mut d2ca = SplitBits::from_vec(&x2);
    match op {
        ShiftOp::Shl => shift::shl_const_assign2(d2ca.as_mut(), n2),
        ShiftOp::Shr => shift::shr_const_assign2(d2ca.as_mut(), n2),
        ShiftOp::Ashr(s) => shift::ashr_const_assign2(d2ca.as_mut(), n2, s),
    }
    check_eq_ref2(
        d2ca.as_ref(),
        &want2,
        &format!("{ctx} 2-state L2 const assign"),
    )?;
    if w <= 64 {
        let xv = to_u64_scalar(&x2);
        let got = match op {
            ShiftOp::Shl => shift::shl2_l64(xv, w, n2),
            ShiftOp::Shr => shift::shr2_l64(xv, w, n2),
            ShiftOp::Ashr(s) => shift::ashr2_l64(xv, w, n2, s),
        };
        let gv = BitVec::from_u64(w, got);
        prop_assert_eq!(
            got,
            gv.to_u64().unwrap(),
            "{} 2-state L1 not normalized",
            ctx
        );
        check_eq_ref2(gv.as_ref(), &want2, &format!("{ctx} 2-state L1"))?;
    }

    // 混在カーネル (2 値データ × 4 値シフト量 → 4 値結果、§3.5.1、CR-6)
    let xl = LogicVec::from_two_state(&x2);
    let want_m = ref_shift(op, &to_ref(&xl), &to_ref(amt));
    let mut dm = SplitLogic::dirty(w);
    match op {
        ShiftOp::Shl => shift::shl_bits(dm.as_mut(), x2s.as_ref(), amts.as_ref()),
        ShiftOp::Shr => shift::shr_bits(dm.as_mut(), x2s.as_ref(), amts.as_ref()),
        ShiftOp::Ashr(s) => shift::ashr_bits(dm.as_mut(), x2s.as_ref(), amts.as_ref(), s),
    }
    check_eq_ref(dm.as_ref(), &want_m, &format!("{ctx} mixed L2"))?;
    let via4 = match op {
        ShiftOp::Shl => xl.shl(amt),
        ShiftOp::Shr => xl.shr(amt),
        ShiftOp::Ashr(s) => xl.ashr(amt, s),
    };
    check_eq(
        dm.as_ref(),
        via4.as_ref(),
        &format!("{ctx} mixed vs from_two_state"),
    )?;
    let got = match op {
        ShiftOp::Shl => x2.shl_logic(amt),
        ShiftOp::Shr => x2.shr_logic(amt),
        ShiftOp::Ashr(s) => x2.ashr_logic(amt, s),
    };
    check_eq_ref(got.as_ref(), &want_m, &format!("{ctx} mixed L3"))?;
    if w <= 64 && amt.width() <= 64 {
        let xv = to_u64_scalar(&x2);
        let a = to_l64(amt);
        let got = match op {
            ShiftOp::Shl => shift::shl_bits_l64(xv, w, a),
            ShiftOp::Shr => shift::shr_bits_l64(xv, w, a),
            ShiftOp::Ashr(s) => shift::ashr_bits_l64(xv, w, a, s),
        };
        check_eq_ref_l64(got, w, &want_m, &format!("{ctx} mixed L1"))?;
    }

    check_unchanged(&x0, x, ctx)?;
    check_unchanged(&amt0, amt, ctx)?;
    Ok(())
}

/// 拡張・切り詰めの全層を参照モデルと比べる (TC35)。
fn check_resize(x: &LogicVec, to: u32, signed: bool) -> Result<(), TestCaseError> {
    let from = x.width();
    let ctx = format!("resize {from}->{to} signed={signed}");
    let ctx = ctx.as_str();
    let x0 = x.clone();
    let want = r::resize(to as usize, &to_ref(x), signed);
    check_eq_ref(x.resize(to, signed).as_ref(), &want, &format!("{ctx} L3"))?;
    let xs = SplitLogic::from_vec(x);
    let mut d = SplitLogic::dirty(to);
    resize::resize(d.as_mut(), xs.as_ref(), signed);
    check_eq_ref(d.as_ref(), &want, &format!("{ctx} L2"))?;
    if from <= 64 && to <= 64 {
        let got = resize::resize_l64(to_l64(x), from, to, signed);
        check_eq_ref_l64(got, to, &want, &format!("{ctx} L1"))?;
    }
    // 2 値
    let x2 = x.to_bit_vec();
    let want2 = ref_two_state(&r::resize(to as usize, &bits_to_ref(&x2), signed));
    check_eq_ref2(
        x2.resize(to, signed).as_ref(),
        &want2,
        &format!("{ctx} 2-state L3"),
    )?;
    let x2s = SplitBits::from_vec(&x2);
    let mut d2 = SplitBits::dirty(to);
    resize::resize2(d2.as_mut(), x2s.as_ref(), signed);
    check_eq_ref2(d2.as_ref(), &want2, &format!("{ctx} 2-state L2"))?;
    if from <= 64 && to <= 64 {
        let got = resize::resize2_l64(to_u64_scalar(&x2), from, to, signed);
        let gv = BitVec::from_u64(to, got);
        prop_assert_eq!(gv.to_u64(), Some(got), "{} 2-state L1 not normalized", ctx);
        check_eq_ref2(gv.as_ref(), &want2, &format!("{ctx} 2-state L1"))?;
    }
    check_unchanged(&x0, x, ctx)
}

/// 4 値 ⇔ 2 値の変換 (TC31, TC35)。
fn check_two_state(x: &LogicVec) -> Result<(), TestCaseError> {
    let w = x.width();
    let want = r::to_two_state(&to_ref(x));
    check_eq_ref2(x.to_two_state().as_ref(), &want, "to_two_state L3")?;
    let xs = SplitLogic::from_vec(x);
    let mut d = SplitBits::dirty(w);
    resize::to_two_state(d.as_mut(), xs.as_ref());
    check_eq_ref2(d.as_ref(), &want, "to_two_state L2")?;
    let b = x.to_bit_vec();
    let mut l = SplitLogic::dirty(w);
    resize::from_two_state(l.as_mut(), SplitBits::from_vec(&b).as_ref());
    check_eq_ref(l.as_ref(), &want, "from_two_state L2")?;
    check_eq_ref(
        LogicVec::from_two_state(&b).as_ref(),
        &want,
        "from_two_state L3",
    )?;
    prop_assert!(
        l.b.iter().all(|&v| v == 0),
        "from_two_state: bval must be 0"
    );
    if w <= 64 {
        let t = resize::to_two_state_l64(to_l64(x));
        prop_assert_eq!(t, to_u64_scalar(&b), "to_two_state L1");
        let f = resize::from_two_state_l64(t);
        check_eq_ref_l64(f, w, &want, "from_two_state L1")?;
    }
    Ok(())
}

/// 連結 (TC35)。
fn check_concat(parts: &[LogicVec]) -> Result<(), TestCaseError> {
    let total: u32 = parts.iter().map(|p| p.width()).sum();
    let ctx = format!(
        "concat {:?}",
        parts.iter().map(|p| p.width()).collect::<Vec<_>>()
    );
    let ctx = ctx.as_str();
    let refs: Vec<RefVec> = parts.iter().map(to_ref).collect();
    let want = r::concat(&refs);
    let pr: Vec<&LogicVec> = parts.iter().collect();
    check_eq_ref(LogicVec::concat(&pr).as_ref(), &want, &format!("{ctx} L3"))?;
    let splits: Vec<SplitLogic> = parts.iter().map(SplitLogic::from_vec).collect();
    let views: Vec<_> = splits.iter().map(|s| s.as_ref()).collect();
    let mut d = SplitLogic::dirty(total);
    concat::concat(d.as_mut(), &views);
    check_eq_ref(d.as_ref(), &want, &format!("{ctx} L2"))?;
    // 2 値
    let bits: Vec<BitVec> = parts.iter().map(|p| p.to_bit_vec()).collect();
    let want2 = ref_two_state(&r::concat(
        &bits.iter().map(bits_to_ref).collect::<Vec<_>>(),
    ));
    let br: Vec<&BitVec> = bits.iter().collect();
    check_eq_ref2(
        BitVec::concat(&br).as_ref(),
        &want2,
        &format!("{ctx} 2-state L3"),
    )?;
    let bsplits: Vec<SplitBits> = bits.iter().map(SplitBits::from_vec).collect();
    let bviews: Vec<_> = bsplits.iter().map(|s| s.as_ref()).collect();
    let mut d2 = SplitBits::dirty(total);
    concat::concat2(d2.as_mut(), &bviews);
    check_eq_ref2(d2.as_ref(), &want2, &format!("{ctx} 2-state L2"))?;
    Ok(())
}

/// 複製 (TC35)。
fn check_replicate(x: &LogicVec, n: u32) -> Result<(), TestCaseError> {
    let ctx = format!("replicate w={} n={n}", x.width());
    let ctx = ctx.as_str();
    let want = r::replicate(&to_ref(x), n);
    check_eq_ref(x.replicate(n).as_ref(), &want, &format!("{ctx} L3"))?;
    let mut d = SplitLogic::dirty(x.width() * n);
    concat::replicate(d.as_mut(), SplitLogic::from_vec(x).as_ref(), n);
    check_eq_ref(d.as_ref(), &want, &format!("{ctx} L2"))?;
    let b = x.to_bit_vec();
    let want2 = ref_two_state(&r::replicate(&bits_to_ref(&b), n));
    check_eq_ref2(
        b.replicate(n).as_ref(),
        &want2,
        &format!("{ctx} 2-state L3"),
    )?;
    let mut d2 = SplitBits::dirty(x.width() * n);
    concat::replicate2(d2.as_mut(), SplitBits::from_vec(&b).as_ref(), n);
    check_eq_ref2(d2.as_ref(), &want2, &format!("{ctx} 2-state L2"))?;
    Ok(())
}

/// 選択の読み書き (TC33, TC35)。`dst` は書き込み先 (幅は `src` と同じでなくてよい)。
fn check_select(src: &LogicVec, sel: &LogicVec, idx: Index) -> Result<(), TestCaseError> {
    let (sw, lw) = (src.width(), sel.width());
    let ctx = format!("select src_w={sw} sel_w={lw} idx={idx:?}");
    let ctx = ctx.as_str();
    let ri = ref_index(idx);
    let src0 = src.clone();

    // get_slice
    let want = r::get_slice(lw as usize, &to_ref(src), ri);
    check_eq_ref(
        src.get_slice(lw, idx).as_ref(),
        &want,
        &format!("{ctx} get L3"),
    )?;
    let ss = SplitLogic::from_vec(src);
    let mut d = SplitLogic::from_vec(&LogicVec::filled(lw, Logic::Z));
    select::get_slice(d.as_mut(), ss.as_ref(), idx);
    check_eq_ref(d.as_ref(), &want, &format!("{ctx} get L2"))?;
    if sw <= 64 && lw <= 64 {
        let got = select::get_slice_l64(to_l64(src), sw, lw, idx);
        check_eq_ref_l64(got, lw, &want, &format!("{ctx} get L1"))?;
    }

    // get_bit
    let want_bit = r::get_slice(1, &to_ref(src), ri);
    check_logic_eq_ref(src.get_bit(idx), &want_bit, &format!("{ctx} get_bit L3"))?;
    check_logic_eq_ref(
        select::get_bit(ss.as_ref(), idx),
        &want_bit,
        &format!("{ctx} get_bit L2"),
    )?;
    if sw <= 64 {
        check_logic_eq_ref(
            select::get_bit_l64(to_l64(src), sw, idx),
            &want_bit,
            &format!("{ctx} get_bit L1"),
        )?;
    }

    // set_slice: src に sel を書く
    let want_set = r::set_slice(&to_ref(src), ri, &to_ref(sel));
    let mut v = src.clone();
    v.set_slice(idx, sel);
    check_eq_ref(v.as_ref(), &want_set, &format!("{ctx} set L3"))?;
    let mut ds = SplitLogic::from_vec(src);
    select::set_slice(ds.as_mut(), idx, SplitLogic::from_vec(sel).as_ref());
    check_eq_ref(ds.as_ref(), &want_set, &format!("{ctx} set L2"))?;
    if sw <= 64 && lw <= 64 {
        let got = select::set_slice_l64(to_l64(src), sw, idx, to_l64(sel), lw);
        check_eq_ref_l64(got, sw, &want_set, &format!("{ctx} set L1"))?;
    }

    // set_bit: sel の LSB を書く
    let bitv = sel.bit(0);
    let want_sb = r::set_slice(&to_ref(src), ri, &to_ref(&LogicVec::from(bitv)));
    let mut v = src.clone();
    v.set_bit_at(idx, bitv);
    check_eq_ref(v.as_ref(), &want_sb, &format!("{ctx} set_bit L3"))?;
    let mut ds = SplitLogic::from_vec(src);
    select::set_bit(ds.as_mut(), idx, bitv);
    check_eq_ref(ds.as_ref(), &want_sb, &format!("{ctx} set_bit L2"))?;

    // 2 値 (範囲外・Unknown は 0、§3.5.1 規則 5)
    let s2 = src.to_bit_vec();
    let l2 = sel.to_bit_vec();
    let want2 = ref_two_state(&r::get_slice(lw as usize, &bits_to_ref(&s2), ri));
    check_eq_ref2(
        s2.get_slice(lw, idx).as_ref(),
        &want2,
        &format!("{ctx} get2 L3"),
    )?;
    let mut d2 = SplitBits::dirty(lw);
    select::get_slice2(d2.as_mut(), SplitBits::from_vec(&s2).as_ref(), idx);
    check_eq_ref2(d2.as_ref(), &want2, &format!("{ctx} get2 L2"))?;
    if sw <= 64 && lw <= 64 {
        let got = select::get_slice2_l64(to_u64_scalar(&s2), sw, lw, idx);
        let gv = BitVec::from_u64(lw, got);
        prop_assert_eq!(gv.to_u64(), Some(got), "{} get2 L1 not normalized", ctx);
        check_eq_ref2(gv.as_ref(), &want2, &format!("{ctx} get2 L1"))?;
    }
    let want_b2 = ref_two_state(&r::get_slice(1, &bits_to_ref(&s2), ri));
    prop_assert_eq!(
        s2.get_bit(idx),
        want_b2.to_bits_string() == "1",
        "{} get_bit2",
        ctx
    );
    let want_set2 = ref_two_state(&r::set_slice(&bits_to_ref(&s2), ri, &bits_to_ref(&l2)));
    let mut v2 = s2.clone();
    v2.set_slice(idx, &l2);
    check_eq_ref2(v2.as_ref(), &want_set2, &format!("{ctx} set2 L3"))?;
    let mut ds2 = SplitBits::from_vec(&s2);
    select::set_slice2(ds2.as_mut(), idx, SplitBits::from_vec(&l2).as_ref());
    check_eq_ref2(ds2.as_ref(), &want_set2, &format!("{ctx} set2 L2"))?;
    if sw <= 64 && lw <= 64 {
        let got = select::set_slice2_l64(to_u64_scalar(&s2), sw, idx, to_u64_scalar(&l2), lw);
        let gv = BitVec::from_u64(sw, got);
        prop_assert_eq!(gv.to_u64(), Some(got), "{} set2 L1 not normalized", ctx);
        check_eq_ref2(gv.as_ref(), &want_set2, &format!("{ctx} set2 L1"))?;
    }
    let b0 = l2.bit(0);
    let want_sb2 = ref_two_state(&r::set_slice(
        &bits_to_ref(&s2),
        ri,
        &bits_to_ref(&BitVec::from(b0)),
    ));
    let mut v2 = s2.clone();
    v2.set_bit_at(idx, b0);
    check_eq_ref2(v2.as_ref(), &want_sb2, &format!("{ctx} set_bit2 L3"))?;

    check_unchanged(&src0, src, ctx)
}

/// インデックスの解釈 (TC33, TC35)。
fn check_index_value(idx: &LogicVec, signed: bool) -> Result<(), TestCaseError> {
    let want = ref_index_to_index(r::index_value(&to_ref(idx), signed));
    let ctx = format!("index_value {idx:?} signed={signed}");
    prop_assert_eq!(idx.index_value(signed), want, "{} L3", ctx);
    prop_assert_eq!(
        select::index_value(SplitLogic::from_vec(idx).as_ref(), signed),
        want,
        "{} L2",
        ctx
    );
    if idx.width() <= 64 {
        prop_assert_eq!(
            select::index_value_l64(to_l64(idx), idx.width(), signed),
            want,
            "{} L1",
            ctx
        );
    }
    let b = idx.to_bit_vec();
    let want2 = ref_index_to_index(r::index_value(&bits_to_ref(&b), signed));
    prop_assert_eq!(b.index_value(signed), want2, "{} 2-state L3", ctx);
    prop_assert_eq!(
        select::index_value2(b.as_ref(), signed),
        want2,
        "{} 2-state L2",
        ctx
    );
    Ok(())
}

fn ref_index_to_index(i: RefIndex) -> Index {
    match i {
        RefIndex::Unknown => Index::Unknown,
        RefIndex::Value(v) => Index::Value(v),
    }
}

/// `$clog2` (TC36)。
fn check_clog2(x: &LogicVec) -> Result<(), TestCaseError> {
    let want = r::clog2(&to_ref(x));
    let ctx = format!("clog2 {x:?}");
    prop_assert_eq!(x.clog2(), want, "{} L3", ctx);
    prop_assert_eq!(
        math::clog2(SplitLogic::from_vec(x).as_ref()),
        want,
        "{} L2",
        ctx
    );
    if x.width() <= 64 {
        prop_assert_eq!(math::clog2_l64(to_l64(x)), want, "{} L1", ctx);
    }
    let b = x.to_bit_vec();
    let want2 = r::clog2(&bits_to_ref(&b)).expect("2-state has no X/Z");
    prop_assert_eq!(b.clog2(), want2, "{} 2-state L3", ctx);
    prop_assert_eq!(math::clog2_2(b.as_ref()), want2, "{} 2-state L2", ctx);
    if x.width() <= 64 {
        prop_assert_eq!(
            math::clog2_2_l64(to_u64_scalar(&b)),
            want2,
            "{} 2-state L1",
            ctx
        );
    }
    Ok(())
}

// ===========================================================================
// 戦略 (このファイル用)
// ===========================================================================

/// 連結の要素: 幅 0 (§11.4.12.1) を含む小さめの幅の値。
fn concat_part() -> BoxedStrategy<LogicVec> {
    prop_oneof![
        1 => Just(LogicVec::zero(0)),
        6 => prop::sample::select(vec![1u32, 2, 3, 7, 8, 31, 32, 33, 63, 64, 65, 127, 128, 129])
            .prop_flat_map(logic_vec),
        3 => (1u32..=200).prop_flat_map(logic_vec),
    ]
    .boxed()
}

/// 拡張・切り詰めの目標幅: 元の幅の前後、境界値、任意。
fn resize_target(w: u32) -> BoxedStrategy<u32> {
    prop_oneof![
        2 => Just(w),
        2 => 1u32..=w.saturating_sub(1).max(1),
        2 => w..=w + 130,
        2 => width(),
    ]
    .boxed()
}

/// `$clog2` の入力: 2 のべきの前後を多く含める。
fn clog2_input(w: u32) -> BoxedStrategy<LogicVec> {
    prop_oneof![
        4 => logic_vec_arith(w),
        2 => (0..w, -1i64..=1).prop_map(move |(k, d)| {
            // 2^k + d (幅 w で切り詰め)
            let mut p = LogicVec::zero(w);
            p.set_bit(k, Logic::L1);
            match d {
                -1 => {
                    // 2^k - 1: 下位 k ビットが 1
                    let mut q = LogicVec::zero(w);
                    for i in 0..k {
                        q.set_bit(i, Logic::L1);
                    }
                    q
                }
                1 => {
                    if k != 0 {
                        p.set_bit(0, Logic::L1);
                    } else if w > 1 {
                        p = LogicVec::from_u64(w, 2);
                    }
                    p
                }
                _ => p,
            }
        }),
        1 => Just(LogicVec::zero(w)),
        1 => Just(LogicVec::from_u64(w, 1)),
    ]
    .boxed()
}

// ===========================================================================
// proptest
// ===========================================================================

proptest! {
    /// TC30 / TC34: シフトの差分テスト (4 値・2 値・混在、全層、`_assign`・`_const`)。
    #[test]
    fn diff_shift((x, amt) in width().prop_flat_map(|w| (logic_vec(w), shift_amount(w)))) {
        for op in SHIFT_OPS {
            check_shift(op, &x, &amt)?;
        }
    }

    /// TC30 性質: X/Z なしで `shl(shl(x, a), b) == shl(x, a + b)`、`shr(shl(x, n), n)` は上位 n ビットを
    /// 0 にした x。
    #[test]
    fn prop_shift_compose(
        (x, a, b) in width().prop_flat_map(|w| (logic_vec_known(w), 0..=w as u64 + 2, 0..=w as u64 + 2))
    ) {
        prop_assert_eq!(x.shl_const(a).shl_const(b), x.shl_const(a + b));
        prop_assert_eq!(x.shr_const(a).shr_const(b), x.shr_const(a + b));
        let w = x.width();
        let n = a;
        let mut want = x.clone();
        for i in w.saturating_sub(n.min(w as u64) as u32)..w {
            want.set_bit(i, Logic::L0);
        }
        prop_assert_eq!(x.shl_const(n).shr_const(n), want);
    }

    /// TC35: 拡張・切り詰め。
    #[test]
    fn diff_resize(
        (x, to, signed) in width().prop_flat_map(|w| (logic_vec(w), resize_target(w), any::<bool>()))
    ) {
        check_resize(&x, to, signed)?;
    }

    /// TC31 性質: 拡張してから元の幅に切り詰めると元に戻る。
    #[test]
    fn prop_resize_round_trip(
        (x, extra, signed) in width().prop_flat_map(|w| (logic_vec(w), 0u32..200, any::<bool>()))
    ) {
        let w = x.width();
        prop_assert_eq!(x.resize(w + extra, signed).resize(w, signed), x);
    }

    /// TC31 / TC35: 2 値 ⇔ 4 値。
    #[test]
    fn diff_two_state(x in width().prop_flat_map(logic_vec)) {
        check_two_state(&x)?;
    }

    /// TC35: 連結 (要素数 0〜8、幅 0 を含む)。
    #[test]
    fn diff_concat(parts in prop::collection::vec(concat_part(), 0..=8)) {
        check_concat(&parts)?;
    }

    /// TC35: 複製 (n = 0〜5)。
    #[test]
    fn diff_replicate((x, n) in (prop_oneof![
        3 => width(),
        1 => Just(0u32),
    ]).prop_flat_map(|w| {
        let v = if w == 0 { Just(LogicVec::zero(0)).boxed() } else { logic_vec(w) };
        (v, 0u32..=5)
    })) {
        check_replicate(&x, n)?;
    }

    /// TC33 / TC35: 選択 (オフセットは負、0、境界、完全に範囲外、`i64` の端、`Unknown`)。
    #[test]
    fn diff_select(
        (src, sel, off) in (width(), prop_oneof![3 => 1u32..=8, 2 => 1u32..=130, 1 => width()])
            .prop_flat_map(|(sw, lw)| (logic_vec(sw), logic_vec(lw), select_offset(sw, lw)))
    ) {
        let idx = match off {
            None => Index::Unknown,
            Some(v) => Index::Value(v),
        };
        check_select(&src, &sel, idx)?;
    }

    /// TC33 / TC35: インデックスの解釈 (幅 1〜200、X/Z を含む、符号付き/符号なし)。
    #[test]
    fn diff_index_value(
        (idx, signed) in (prop_oneof![
            2 => prop::sample::select(vec![1u32, 2, 4, 8, 32, 63, 64, 65, 100, 128, 129]),
            1 => 1u32..=200,
        ]).prop_flat_map(|w| (logic_vec(w), any::<bool>()))
    ) {
        check_index_value(&idx, signed)?;
    }

    /// TC36: `$clog2`。
    #[test]
    fn diff_clog2(x in width().prop_flat_map(clog2_input)) {
        check_clog2(&x)?;
    }
}

// ===========================================================================
// 網羅テスト (plan.md §5.4: 幅 1〜4 の全値 × シフト量 0..=w+1 と X)
// ===========================================================================

fn lv(s: &str) -> LogicVec {
    LogicVec::from_bits_str(s).unwrap()
}

/// TC30 網羅: 幅 1〜4 の全 4 値 × シフト量 `0..=w+1` と X/Z を含むシフト量。
#[test]
fn exhaustive_shift_w1_4() {
    for w in 1..=4u32 {
        let mut amts: Vec<LogicVec> = (0..=w as u64 + 1)
            .map(|n| LogicVec::from_u64(3, n))
            .collect();
        amts.extend(["x0", "z11", "x", "1z", "xxx"].map(lv));
        for x in all_logic(w) {
            for amt in &amts {
                for op in SHIFT_OPS {
                    run_case(check_shift(op, &x, amt));
                }
            }
        }
    }
}

/// 網羅: 幅 1〜3 の全値で拡張・切り詰め (1〜5 ビットへ)、2 値/4 値の変換、`$clog2`、
/// インデックス、選択 (オフセット -4..=6 と Unknown)。
#[test]
fn exhaustive_struct_w1_3() {
    for w in 1..=3u32 {
        let vals = all_logic(w);
        for x in &vals {
            for to in 1..=5 {
                for s in [false, true] {
                    run_case(check_resize(x, to, s));
                }
            }
            run_case(check_two_state(x));
            run_case(check_clog2(x));
            for s in [false, true] {
                run_case(check_index_value(x, s));
            }
            for sel in [lv("1"), lv("x0"), lv("z1x")] {
                for off in -4..=6 {
                    run_case(check_select(x, &sel, Index::Value(off)));
                }
                run_case(check_select(x, &sel, Index::Unknown));
            }
        }
        // 連結: 幅 w の値と 2 ビットの値の全組 (w <= 2)
        if w <= 2 {
            for x in &vals {
                for y in all_logic(2) {
                    run_case(check_concat(&[x.clone(), LogicVec::zero(0), y]));
                }
                for n in 0..=3 {
                    run_case(check_replicate(x, n));
                }
            }
        }
    }
}

// ===========================================================================
// TC52: 幅境界の横断テスト (L-5)
// ===========================================================================

// 幅と値の模様は tests/common の `TC52_WIDTHS` / `tc52_patterns`。

fn tc52_amounts(w: u32) -> Vec<LogicVec> {
    let mut v: Vec<LogicVec> = [0u64, 1, 63, 64, 65, w as u64 - 1, w as u64, w as u64 + 1]
        .iter()
        .map(|&n| LogicVec::from_u64(64, n))
        .collect();
    let mut big = LogicVec::zero(128);
    big.set_bit(64, Logic::L1); // 2^64
    v.push(big);
    v.push(lv("x01"));
    v
}

/// TC52: 決まった幅 × 値のパターンで T6 の全演算を参照モデルと比べる。
#[test]
fn tc52_width_boundaries() {
    for w in TC52_WIDTHS {
        let pats = tc52_patterns(w);
        let amts = tc52_amounts(w);
        for x in &pats {
            for amt in &amts {
                for op in SHIFT_OPS {
                    run_case(check_shift(op, x, amt));
                }
            }
            for to in [1, w - 1, w, w + 1, 64, 65, 129] {
                if to == 0 {
                    continue;
                }
                for s in [false, true] {
                    run_case(check_resize(x, to, s));
                }
            }
            run_case(check_two_state(x));
            run_case(check_clog2(x));
            run_case(check_concat(&[x.clone(), lv("1x"), x.clone()]));
            run_case(check_replicate(x, 2));
            let wi = w as i64;
            for (lw, off) in [
                (1u32, 0i64),
                (1, wi - 1),
                (1, wi),
                (4, wi - 2),
                (64, -1),
                (64, 1),
                (65, wi - 65),
                (w, 0),
                (w, -wi),
                (3, i64::MAX),
                (3, i64::MIN),
            ] {
                let sel = LogicVec::filled(lw, Logic::L1);
                run_case(check_select(x, &sel, Index::Value(off)));
            }
            run_case(check_select(x, &lv("10"), Index::Unknown));
        }
        for p in pats.iter().take(3) {
            for s in [false, true] {
                run_case(check_index_value(p, s));
            }
        }
    }
}
