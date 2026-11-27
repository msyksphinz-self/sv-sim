//! 算術 (`+ - * / % **`、単項 `-`、`++ --`、`pow_bits`) がヒープ確保をしないことの検査
//! (plan.md §5.5、test-plan TC54、受け入れ基準 L-6)。
//!
//! 1. 幅 ≤ 64 の `LogicVec` / `BitVec` の算術の全メソッド (層 1 の式で計算するので作業領域も要らない)。
//! 2. 任意幅 (65, 129, 1024) のスライスカーネル (層 2) に、`Scratch::reserve_for(width)` で事前に確保した
//!    作業領域と、事前に確保した出力先を渡したもの。除数 1 ワード / 多ワード / 0、指数が広い場合を含む。
//!
//! 確保の回数は、スレッドローカルなカウンタを持つ `GlobalAlloc` で数える (R-24: テストハーネスは
//! `#[test]` を並列のスレッドで走らせるため)。カウンタは `const` 初期化の `thread_local!` なので、
//! それ自身は確保しない。

// このファイルだけ `GlobalAlloc` の実装のために unsafe を使う (plan.md §5.5)。
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;

use svsim_logic::ops::arith;
use svsim_logic::{BitMut, BitVec, L64, Logic, LogicMut, LogicVec, Scratch, words_for};

struct Counting;

thread_local! {
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

fn bump() {
    // スレッドの終了処理中などで TLS が使えない場合は数えない。
    let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
}

// SAFETY: 実際の確保・解放は `System` にそのまま委ねる。追加の処理はスレッドローカルのカウンタを
// 増やすだけで、確保をせず、`layout` の契約も変えない。
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        bump();
        // SAFETY: 呼び出し側が `GlobalAlloc::alloc` の契約 (layout の大きさが 0 でない) を守る。
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        bump();
        // SAFETY: 同上。
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        bump();
        // SAFETY: `ptr` はこのアロケータ (= `System`) が `layout` で確保したもの (呼び出し側の契約)。
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` はこのアロケータ (= `System`) が `layout` で確保したもの (呼び出し側の契約)。
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// `f` の実行中の確保回数。
fn count_allocs(f: impl FnOnce()) -> usize {
    let before = ALLOCS.with(|c| c.get());
    f();
    ALLOCS.with(|c| c.get()) - before
}

fn assert_no_alloc(name: &str, f: impl FnOnce()) {
    let n = count_allocs(f);
    assert_eq!(n, 0, "{name}: {n} heap allocation(s)");
}

/// 検査器の自己テスト: 意図的な確保を数える (TC54)。
#[test]
fn counter_detects_allocation() {
    let n = count_allocs(|| {
        black_box(Vec::<u8>::with_capacity(1));
    });
    assert!(n >= 1);
    // 幅 65 の LogicVec はヒープに置くので確保する。
    assert!(count_allocs(|| drop(black_box(LogicVec::zero(65)))) >= 1);
}

/// 1. 幅 ≤ 64 の `LogicVec` / `BitVec` の T5 の全メソッド。
#[test]
fn layer3_narrow_no_alloc() {
    for w in [1u32, 32, 63, 64] {
        let x = LogicVec::from_i64(w, -5);
        let y = LogicVec::from_u64(w, 3);
        let zero = LogicVec::zero(w);
        let mut xu = LogicVec::from_u64(w, 6);
        xu.set_bit(0, Logic::X);
        let e = LogicVec::from_i64(8, -3);
        let e_wide = LogicVec::from_u64(100, 77);
        let ex = LogicVec::filled(4, Logic::Z);
        let bx = x.to_bit_vec();
        let by = y.to_bit_vec();
        let bzero = BitVec::zero(w);
        let be = e.to_bit_vec();
        let be_wide = e_wide.to_bit_vec();
        assert_no_alloc(&format!("LogicVec w={w}"), || {
            let mut v = black_box(x.clone());
            black_box(v.add(&y));
            black_box(v.sub(&y));
            black_box(v.mul(&y));
            black_box(v.div(&y, true));
            black_box(v.div(&zero, false));
            black_box(v.rem(&y, true));
            black_box(v.rem(&zero, true));
            black_box(v.neg());
            black_box(v.inc());
            black_box(v.dec());
            black_box(xu.add(&y));
            black_box(v.pow(true, &e, true));
            black_box(v.pow(false, &e_wide, false));
            black_box(v.pow(false, &ex, false));
            black_box(y.pow(false, &e_wide, true));
            v.add_assign(&y);
            v.sub_assign(&y);
            v.mul_assign(&y);
            v.div_assign(&y, true);
            v.rem_assign(&y, false);
            v.neg_assign();
            v.inc_assign();
            v.dec_assign();
            black_box(v);
        });
        assert_no_alloc(&format!("BitVec w={w}"), || {
            let mut v = black_box(bx.clone());
            black_box(v.add(&by));
            black_box(v.sub(&by));
            black_box(v.mul(&by));
            black_box(v.div(&by, true));
            black_box(v.div(&bzero, false));
            black_box(v.rem(&by, true));
            black_box(v.rem(&bzero, true));
            black_box(v.neg());
            black_box(v.inc());
            black_box(v.dec());
            black_box(v.pow(true, &be, true));
            black_box(v.pow(false, &be_wide, false));
            black_box(v.pow_logic(true, &e, true));
            black_box(v.pow_logic(false, &e_wide, false));
            black_box(v.pow_logic(false, &ex, false));
            v.add_assign(&by);
            v.sub_assign(&by);
            v.mul_assign(&by);
            v.div_assign(&by, true);
            v.rem_assign(&by, false);
            v.neg_assign();
            v.inc_assign();
            v.dec_assign();
            black_box(v);
        });
        // 層 1
        let (lx, ly) = (L64::load(x.as_ref()), L64::load(y.as_ref()));
        let le = L64::load(e.as_ref());
        assert_no_alloc(&format!("L64 w={w}"), || {
            black_box(arith::add_l64(lx, ly, w));
            black_box(arith::mul_l64(lx, ly, w));
            black_box(arith::div_l64(lx, ly, w, true));
            black_box(arith::rem_l64(lx, ly, w, true));
            black_box(arith::pow_l64(lx, w, true, le, 8, true));
            black_box(arith::pow2_l64(lx.a, w, false, 200, 8, false));
            black_box(arith::pow_bits_l64(lx.a, w, false, le, 8, true));
            black_box(arith::div2_l64(lx.a, 0, w, true));
        });
    }
}

/// 2. 任意幅のスライスカーネルに、`reserve_for(width)` で事前確保した `Scratch` を渡す。
#[test]
fn layer2_wide_no_alloc() {
    for w in [65u32, 129, 1024] {
        let nw = words_for(w);
        let x = LogicVec::from_i64(w, -12345).mul(&LogicVec::from_i64(w, -987_654_321));
        let mut big = LogicVec::filled(w, Logic::L1);
        big.set_bit(w - 2, Logic::L0);
        let small = LogicVec::from_u64(w, 7);
        let zero = LogicVec::zero(w);
        let mut xu = x.clone();
        xu.set_bit(3, Logic::Z);
        let e = LogicVec::from_u64(16, 1000);
        let e_neg = LogicVec::from_i64(8, -3);
        let mut e_wide = LogicVec::zero(2048);
        e_wide.set_bit(2000, Logic::L1);
        e_wide.set_bit(5, Logic::L1);
        let bx = x.to_bit_vec();
        let bbig = big.to_bit_vec();
        let bsmall = small.to_bit_vec();
        let bzero = zero.to_bit_vec();
        let be = e.to_bit_vec();
        let be_wide = e_wide.to_bit_vec();
        let mut s = Scratch::new();
        s.reserve_for(w);
        let mut da = vec![0u64; nw];
        let mut db = vec![0u64; nw];
        let mut d2 = vec![0u64; nw];
        let base3 = LogicVec::from_u64(w, 3);
        assert_no_alloc(&format!("slice kernels w={w}"), || {
            let xr = x.as_ref();
            for y in [&big, &small, &zero, &xu] {
                let yr = y.as_ref();
                arith::add(LogicMut::new(w, &mut da, &mut db), xr, yr);
                arith::sub(LogicMut::new(w, &mut da, &mut db), xr, yr);
                arith::mul(LogicMut::new(w, &mut da, &mut db), xr, yr, &mut s);
                for signed in [false, true] {
                    arith::div(LogicMut::new(w, &mut da, &mut db), xr, yr, signed, &mut s);
                    arith::rem(LogicMut::new(w, &mut da, &mut db), xr, yr, signed, &mut s);
                    // 被除数 < 除数
                    arith::div(LogicMut::new(w, &mut da, &mut db), yr, xr, signed, &mut s);
                }
                let mut d = LogicMut::new(w, &mut da, &mut db);
                d.copy_from(xr);
                arith::add_assign(d.reborrow(), yr);
                arith::sub_assign(d.reborrow(), yr);
                arith::mul_assign(d.reborrow(), yr, &mut s);
                arith::div_assign(d.reborrow(), yr, true, &mut s);
                d.copy_from(xr);
                arith::rem_assign(d.reborrow(), yr, false, &mut s);
            }
            arith::neg(LogicMut::new(w, &mut da, &mut db), xr);
            arith::inc(LogicMut::new(w, &mut da, &mut db), xr);
            arith::dec(LogicMut::new(w, &mut da, &mut db), xr);
            arith::neg_assign(LogicMut::new(w, &mut da, &mut db));
            arith::inc_assign(LogicMut::new(w, &mut da, &mut db));
            arith::dec_assign(LogicMut::new(w, &mut da, &mut db));
            for ex in [&e, &e_neg, &e_wide] {
                arith::pow(
                    LogicMut::new(w, &mut da, &mut db),
                    xr,
                    true,
                    ex.as_ref(),
                    true,
                    &mut s,
                );
                arith::pow(
                    LogicMut::new(w, &mut da, &mut db),
                    base3.as_ref(),
                    false,
                    ex.as_ref(),
                    false,
                    &mut s,
                );
                arith::pow_bits(
                    LogicMut::new(w, &mut da, &mut db),
                    bx.as_ref(),
                    false,
                    ex.as_ref(),
                    false,
                    &mut s,
                );
            }

            // 2 値版
            let bxr = bx.as_ref();
            for by in [&bbig, &bsmall, &bzero] {
                let byr = by.as_ref();
                arith::add2(BitMut::new(w, &mut d2), bxr, byr);
                arith::sub2(BitMut::new(w, &mut d2), bxr, byr);
                arith::mul2(BitMut::new(w, &mut d2), bxr, byr, &mut s);
                for signed in [false, true] {
                    arith::div2(BitMut::new(w, &mut d2), bxr, byr, signed, &mut s);
                    arith::rem2(BitMut::new(w, &mut d2), bxr, byr, signed, &mut s);
                }
                let mut d = BitMut::new(w, &mut d2);
                d.copy_from(bxr);
                arith::add_assign2(d.reborrow(), byr);
                arith::sub_assign2(d.reborrow(), byr);
                arith::mul_assign2(d.reborrow(), byr, &mut s);
                arith::div_assign2(d.reborrow(), byr, true, &mut s);
                arith::rem_assign2(d.reborrow(), byr, true, &mut s);
            }
            arith::neg2(BitMut::new(w, &mut d2), bxr);
            arith::inc_assign2(BitMut::new(w, &mut d2));
            arith::dec_assign2(BitMut::new(w, &mut d2));
            for ex in [&be, &be_wide] {
                arith::pow2(
                    BitMut::new(w, &mut d2),
                    bxr,
                    true,
                    ex.as_ref(),
                    false,
                    &mut s,
                );
            }
        });
    }
}

/// `Scratch::words_needed` が算術の最大の必要量 (`div` の `2·nw + 1`) を上回る (plan.md §3.5(c))。
#[test]
fn words_needed_covers_arith() {
    for w in [1u32, 64, 65, 129, 1024, 1 << 20] {
        // div / rem の 2·nw + 1 ワード
        assert!(Scratch::words_needed(w) > 2 * words_for(w), "w={w}");
    }
}
