//! M0-T4: ビット演算・リダクション・論理・比較・`?:`・エッジがヒープ確保しないことの検査
//! (plan.md §5.5、受け入れ基準 L-6、test-plan M0-TC54)。
//!
//! 1. 幅 ≤ 64 (1, 32, 63, 64) の `LogicVec` / `BitVec` の T4 の全メソッド。
//! 2. 任意幅 (65, 129, 1024) のスライスカーネル (層 2)。T4 の演算は作業領域 (`Scratch`) を使わない。
//!
//! 確保の回数は、スレッドローカルなカウンタを持つ `GlobalAlloc` で数える (R-24: テストは並列の
//! スレッドで走るため。`const` 初期化の `thread_local!` は確保しない)。

#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;

use svsim_logic::ops::reduce::LogicSet;
use svsim_logic::ops::{bitwise, compare, cond, edge, logical, reduce};
use svsim_logic::{BitRef, BitVec, Logic, LogicMut, LogicRef, LogicVec};

thread_local! {
    static ALLOCS: Cell<u64> = const { Cell::new(0) };
}

fn bump() {
    // スレッドの終了処理中 (TLS の破棄後) に呼ばれても panic しないように try_with を使う。
    let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
}

struct CountingAlloc;

// SAFETY: 確保と解放はすべて `System` にそのまま委ねる。追加の処理はスレッドローカルの `Cell` の
// 加算だけで、これはヒープ確保をせず、`GlobalAlloc` の契約 (返すポインタ、レイアウト) に影響しない。
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        bump();
        // SAFETY: 呼び出し側が `GlobalAlloc::alloc` の契約 (layout の大きさが 0 でない) を守っている。
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        bump();
        // SAFETY: 同上。
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        bump();
        // SAFETY: `ptr` はこのアロケータ (= System) が `layout` で確保したもの (呼び出し側の契約)。
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` はこのアロケータ (= System) が `layout` で確保したもの (呼び出し側の契約)。
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

/// `f` の実行中に、このスレッドで起きた確保の回数。
fn allocs_during<R>(f: impl FnOnce() -> R) -> u64 {
    let before = ALLOCS.with(Cell::get);
    let r = f();
    let after = ALLOCS.with(Cell::get);
    drop(black_box(r));
    after - before
}

/// 決まった模様の 4 値の値 (x と z を含む)。
fn sample(w: u32, seed: u32) -> LogicVec {
    let mut v = LogicVec::zero(w);
    for i in 0..w {
        let k = (i.wrapping_mul(2654435761) ^ seed) % 7;
        let val = match k {
            0..=2 => Logic::L0,
            3 | 4 => Logic::L1,
            5 => Logic::X,
            _ => Logic::Z,
        };
        v.set_bit(i, val);
    }
    v
}

fn sample2(w: u32, seed: u32) -> BitVec {
    sample(w, seed).to_bit_vec()
}

/// 検査器の自己テスト: 計測区間で確保すれば 1 以上を数える。
#[test]
fn tc54_counter_detects_allocation() {
    let n = allocs_during(|| black_box(Vec::<u64>::with_capacity(1)));
    assert!(n >= 1, "counter must detect an allocation (got {n})");
    let n = allocs_during(|| black_box(1 + 1));
    assert_eq!(n, 0);
}

/// 幅 ≤ 64 の `LogicVec` / `BitVec` の T4 の全メソッドが確保しない (L-6 の 1)。
#[test]
fn tc54_logic_vec_methods_do_not_allocate() {
    for w in [1u32, 32, 63, 64] {
        let x = sample(w, 1);
        let y = sample(w, 2);
        let bx = sample2(w, 3);
        let by = sample2(w, 4);
        let mut acc = x.clone();
        let mut bacc = bx.clone();
        let n = allocs_during(|| {
            let mut sink = 0u64;
            let mut s = |l: Logic| sink = sink.wrapping_add(l as u64);
            // ビット演算 (§11.4.8)
            black_box(x.not());
            black_box(x.and(&y));
            black_box(x.or(&y));
            black_box(x.xor(&y));
            black_box(x.xnor(&y));
            acc.not_assign();
            acc.and_assign(&y);
            acc.or_assign(&y);
            acc.xor_assign(&y);
            acc.xnor_assign(&y);
            black_box(bx.not());
            black_box(bx.and(&by));
            black_box(bx.or(&by));
            black_box(bx.xor(&by));
            black_box(bx.xnor(&by));
            bacc.not_assign();
            bacc.and_assign(&by);
            bacc.or_assign(&by);
            bacc.xor_assign(&by);
            bacc.xnor_assign(&by);
            // リダクション (§11.4.9)、$countbits 系 (§20.9)
            s(x.reduce_and());
            s(x.reduce_nand());
            s(x.reduce_or());
            s(x.reduce_nor());
            s(x.reduce_xor());
            s(x.reduce_xnor());
            black_box((x.count_bits(LogicSet::UNKNOWN), x.count_ones()));
            black_box((x.onehot(), x.onehot0(), x.is_unknown()));
            black_box((bx.reduce_and(), bx.reduce_nand(), bx.reduce_or()));
            black_box((bx.reduce_nor(), bx.reduce_xor(), bx.reduce_xnor()));
            black_box((bx.count_bits(LogicSet::ALL), bx.count_ones()));
            black_box((bx.onehot(), bx.onehot0()));
            // 真理値と論理演算 (§11.4.7, §12.4)
            s(x.truth());
            s(x.logical_not());
            s(x.truth()
                .and(y.truth())
                .or(Logic::X)
                .implies(Logic::L1)
                .equiv(Logic::L0));
            black_box((bx.truth(), bx.logical_not()));
            // 等価・比較 (§11.4.4〜§11.4.6, §12.5.1)
            s(x.eq(&y));
            s(x.ne(&y));
            s(x.wildcard_eq(&y));
            s(x.wildcard_ne(&y));
            black_box((
                x.case_eq(&y),
                x.case_ne(&y),
                x.casez_match(&y),
                x.casex_match(&y),
            ));
            for sg in [false, true] {
                s(x.lt(&y, sg));
                s(x.le(&y, sg));
                s(x.gt(&y, sg));
                s(x.ge(&y, sg));
                black_box((
                    bx.lt(&by, sg),
                    bx.le(&by, sg),
                    bx.gt(&by, sg),
                    bx.ge(&by, sg),
                ));
            }
            black_box((bx.eq(&by), bx.ne(&by), bx.case_eq(&by), bx.case_ne(&by)));
            black_box((bx.wildcard_eq(&by), bx.wildcard_ne(&by)));
            black_box((bx.casez_match(&by), bx.casex_match(&by)));
            // ?: のマージ (§11.4.11、§3.5.1)
            black_box(x.cond_merge(&y));
            black_box(bx.cond_merge_logic(&by));
            // エッジ (§9.4.2)
            black_box(edge::edge_lsb(x.as_ref(), y.as_ref()));
            black_box(edge::edge_lsb2(bx.as_ref(), by.as_ref()));
            black_box(x.clone());
            sink
        });
        assert_eq!(
            n, 0,
            "width {w}: T4 LogicVec/BitVec methods allocated {n} times"
        );
    }
}

/// 4 値の出力先 (aval/bval を別バッファに持つ)。
struct Buf {
    w: u32,
    a: Vec<u64>,
    b: Vec<u64>,
}

impl Buf {
    fn new(w: u32) -> Self {
        let n = svsim_logic::words_for(w);
        Buf {
            w,
            a: vec![0; n],
            b: vec![0; n],
        }
    }
    fn m(&mut self) -> LogicMut<'_> {
        LogicMut::new(self.w, &mut self.a, &mut self.b)
    }
}

/// 任意幅のスライスカーネル (層 2) が確保しない (L-6 の 2)。
#[test]
fn tc54_slice_kernels_do_not_allocate() {
    for w in [65u32, 129, 1024] {
        let x = sample(w, 5);
        let y = sample(w, 6);
        let bx = sample2(w, 7);
        let by = sample2(w, 8);
        let mut d = Buf::new(w);
        let mut d2 = BitVec::zero(w);
        let n = allocs_during(|| {
            let (xr, yr): (LogicRef<'_>, LogicRef<'_>) = (x.as_ref(), y.as_ref());
            let (bxr, byr): (BitRef<'_>, BitRef<'_>) = (bx.as_ref(), by.as_ref());
            bitwise::not(d.m(), xr);
            bitwise::and(d.m(), xr, yr);
            bitwise::or(d.m(), xr, yr);
            bitwise::xor(d.m(), xr, yr);
            bitwise::xnor(d.m(), xr, yr);
            bitwise::not_assign(d.m());
            bitwise::and_assign(d.m(), yr);
            bitwise::or_assign(d.m(), yr);
            bitwise::xor_assign(d.m(), yr);
            bitwise::xnor_assign(d.m(), yr);
            bitwise::not2(d2.as_mut(), bxr);
            bitwise::and2(d2.as_mut(), bxr, byr);
            bitwise::or2(d2.as_mut(), bxr, byr);
            bitwise::xor2(d2.as_mut(), bxr, byr);
            bitwise::xnor2(d2.as_mut(), bxr, byr);
            bitwise::not_assign2(d2.as_mut());
            bitwise::and_assign2(d2.as_mut(), byr);
            bitwise::or_assign2(d2.as_mut(), byr);
            bitwise::xor_assign2(d2.as_mut(), byr);
            bitwise::xnor_assign2(d2.as_mut(), byr);
            black_box((reduce::reduce_and(xr), reduce::reduce_nand(xr)));
            black_box((reduce::reduce_or(xr), reduce::reduce_nor(xr)));
            black_box((reduce::reduce_xor(xr), reduce::reduce_xnor(xr)));
            black_box((
                reduce::count_bits(xr, LogicSet::ALL),
                reduce::count_ones(xr),
            ));
            black_box((
                reduce::onehot(xr),
                reduce::onehot0(xr),
                reduce::is_unknown(xr),
            ));
            black_box((reduce::reduce_and2(bxr), reduce::reduce_xor2(bxr)));
            black_box((
                reduce::count_bits2(bxr, LogicSet::ZERO),
                reduce::onehot2(bxr),
            ));
            black_box((
                logical::truth(xr),
                logical::logical_not(xr),
                logical::truth2(bxr),
            ));
            black_box((
                compare::eq(xr, yr),
                compare::ne(xr, yr),
                compare::case_eq(xr, yr),
            ));
            black_box((compare::wildcard_eq(xr, yr), compare::casez_match(xr, yr)));
            black_box((compare::casex_match(xr, yr), compare::lt(xr, yr, true)));
            black_box((compare::ge(xr, yr, false), compare::eq2(bxr, byr)));
            black_box((compare::lt2(bxr, byr, true), compare::gt2(bxr, byr, false)));
            cond::cond_merge(d.m(), xr, yr);
            cond::cond_merge_bits(d.m(), bxr, byr);
            black_box(edge::edge_lsb(xr, yr));
        });
        assert_eq!(n, 0, "width {w}: T4 slice kernels allocated {n} times");
    }
}
