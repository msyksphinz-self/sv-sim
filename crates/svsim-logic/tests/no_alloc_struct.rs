//! 構造演算 (シフト、拡張・切り詰め、2 値/4 値の変換、連結、複製、選択、`$clog2`) がヒープ確保を
//! しないことの検査 (plan.md §5.5、test-plan TC54、受け入れ基準 L-6)。
//!
//! 1. 幅 ≤ 64 の `LogicVec` / `BitVec` の全メソッド (結果の幅も ≤ 64 のもの)。
//! 2. 任意幅 (65, 129, 1024) のスライスカーネル (層 2) に、事前に確保した出力先を渡したもの。
//!
//! 確保の回数は、スレッドローカルなカウンタを持つ `GlobalAlloc` で数える (R-24: テストハーネスは
//! `#[test]` を並列のスレッドで走らせるため)。カウンタは `const` 初期化の `thread_local!` なので、
//! それ自身は確保しない。

// このファイルだけ `GlobalAlloc` の実装のために unsafe を使う (plan.md §5.5)。
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;

use svsim_logic::ops::select::{self, Index};
use svsim_logic::ops::{concat, math, resize, shift};
use svsim_logic::{BitVec, L64, Logic, LogicVec, words_for};

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

/// 1. 幅 ≤ 64 の `LogicVec` / `BitVec` の T6 の全メソッド。
#[test]
fn layer3_narrow_no_alloc() {
    for w in [1u32, 32, 63, 64] {
        let mut x = LogicVec::filled(w, Logic::L1);
        x.set_bit(0, Logic::X);
        x.set_bit(w - 1, Logic::Z);
        let amt = LogicVec::from_u64(7, 3);
        let amt_x = LogicVec::filled(3, Logic::X);
        let b = BitVec::filled(w, true);
        let bamt = BitVec::from_u64(7, 2);
        let one = LogicVec::from_u64(1, 1);
        let e = LogicVec::zero(0);
        let bone = BitVec::from_u64(1, 1);
        let sel = LogicVec::from_u64(1, 0);
        let bsel = BitVec::from_u64(1, 0);
        assert_no_alloc(&format!("LogicVec w={w}"), || {
            let mut v = black_box(x.clone());
            black_box(v.shl(&amt));
            black_box(v.shr(&amt_x));
            black_box(v.ashr(&amt, true));
            black_box(v.shl_const(5));
            black_box(v.shr_const(5));
            black_box(v.ashr_const(5, true));
            v.shl_assign(&amt);
            v.shr_assign(&amt);
            v.ashr_assign(&amt, true);
            black_box(v.resize(64, true));
            black_box(v.resize(1, false));
            black_box(v.to_two_state());
            black_box(LogicVec::from_two_state(&b));
            black_box(v.index_value(true));
            black_box(v.get_slice(8, Index::Value(-3)));
            black_box(v.get_bit(Index::Value(0)));
            v.set_slice(Index::Value(1), &one);
            v.set_bit_at(Index::Value(0), Logic::L1);
            black_box(v.clog2());
            if w < 64 {
                black_box(LogicVec::concat(&[&one, &e, &v.resize(w, false)]));
                black_box(sel.replicate(w));
            }
            black_box(v);
        });
        assert_no_alloc(&format!("BitVec w={w}"), || {
            let mut v = black_box(b.clone());
            black_box(v.shl(&bamt));
            black_box(v.shr(&bamt));
            black_box(v.ashr(&bamt, true));
            black_box(v.shl_const(5));
            black_box(v.shr_const(5));
            black_box(v.ashr_const(5, false));
            black_box(v.shl_logic(&amt));
            black_box(v.shr_logic(&amt_x));
            black_box(v.ashr_logic(&amt, true));
            black_box(v.resize(64, true));
            black_box(v.index_value(false));
            black_box(v.get_slice(8, Index::Unknown));
            black_box(v.get_bit(Index::Value(1)));
            v.set_slice(Index::Value(1), &bone);
            v.set_bit_at(Index::Value(0), false);
            black_box(v.clog2());
            if w < 64 {
                black_box(BitVec::concat(&[&bone, &v]));
                black_box(bsel.replicate(w));
            }
            black_box(v);
        });
        // 層 1
        let l = L64::load(x.as_ref());
        assert_no_alloc(&format!("L64 w={w}"), || {
            black_box(shift::shl_l64(l, w, L64::new(3, 0)));
            black_box(shift::ashr_const_l64(l, w, 3, true));
            black_box(shift::shr2_l64(l.a, w, 3));
            black_box(shift::ashr_bits_l64(l.a, w, L64::new(1, 0), true));
            black_box(resize::resize_l64(l, w, 64, true));
            black_box(select::get_slice_l64(l, w, 4, Index::Value(-1)));
            black_box(select::set_slice_l64(
                l,
                w,
                Index::Value(2),
                L64::new(1, 0),
                1,
            ));
            black_box(select::index_value_l64(l, w, true));
            black_box(math::clog2_l64(l));
        });
    }
}

/// 2. 任意幅のスライスカーネル (出力先は事前に確保しておく)。
#[test]
fn layer2_wide_no_alloc() {
    for w in [65u32, 129, 1024] {
        let nw = words_for(w);
        let mut x = LogicVec::filled(w, Logic::L1);
        x.set_bit(w - 1, Logic::X);
        let amt = LogicVec::from_u64(70, 67);
        let b = BitVec::filled(w, true);
        let bamt = BitVec::from_u64(8, 3);
        let mut da = vec![0u64; nw];
        let mut db = vec![0u64; nw];
        let mut d2 = vec![0u64; nw];
        let wide = 2 * w + 3;
        let mut ca = vec![0u64; words_for(wide)];
        let mut cb = vec![0u64; words_for(wide)];
        let mut c2 = vec![0u64; words_for(wide)];
        let tail = LogicVec::from_u64(3, 5);
        let btail = BitVec::from_u64(3, 5);
        let mut sa = vec![0u64; 1];
        let mut sb = vec![0u64; 1];
        let mut s2 = vec![0u64; 1];
        let mut ra = vec![0u64; nw];
        let mut rb = vec![0u64; nw];
        let mut r2 = vec![0u64; nw];
        assert_no_alloc(&format!("slice kernels w={w}"), || {
            use svsim_logic::{BitMut, LogicMut};
            let xr = x.as_ref();
            shift::shl(LogicMut::new(w, &mut da, &mut db), xr, amt.as_ref());
            shift::shr(LogicMut::new(w, &mut da, &mut db), xr, amt.as_ref());
            shift::ashr(LogicMut::new(w, &mut da, &mut db), xr, amt.as_ref(), true);
            shift::shl_const(LogicMut::new(w, &mut da, &mut db), xr, 100);
            shift::ashr_const_assign(LogicMut::new(w, &mut da, &mut db), 3, true);
            shift::shl_assign(LogicMut::new(w, &mut da, &mut db), amt.as_ref());
            shift::shl2(BitMut::new(w, &mut d2), b.as_ref(), bamt.as_ref());
            shift::ashr_assign2(BitMut::new(w, &mut d2), bamt.as_ref(), true);
            shift::shl_bits(LogicMut::new(w, &mut da, &mut db), b.as_ref(), amt.as_ref());
            shift::ashr_bits(
                LogicMut::new(w, &mut da, &mut db),
                b.as_ref(),
                amt.as_ref(),
                true,
            );
            resize::resize(LogicMut::new(w, &mut ra, &mut rb), tail.as_ref(), true);
            resize::resize2(BitMut::new(w, &mut r2), btail.as_ref(), true);
            resize::to_two_state(BitMut::new(w, &mut d2), xr);
            resize::from_two_state(LogicMut::new(w, &mut da, &mut db), b.as_ref());
            concat::concat(
                LogicMut::new(wide, &mut ca, &mut cb),
                &[xr, tail.as_ref(), xr],
            );
            concat::concat2(
                BitMut::new(wide, &mut c2),
                &[b.as_ref(), btail.as_ref(), b.as_ref()],
            );
            concat::replicate(
                LogicMut::new(w, &mut ra, &mut rb),
                LogicVec::zero(1).as_ref(),
                w,
            );
            black_box(select::index_value(amt.as_ref(), true));
            select::get_slice(
                LogicMut::new(5, &mut sa, &mut sb),
                xr,
                Index::Value(w as i64 - 3),
            );
            select::get_slice2(BitMut::new(5, &mut s2), b.as_ref(), Index::Unknown);
            select::set_slice(
                LogicMut::new(w, &mut da, &mut db),
                Index::Value(62),
                tail.as_ref(),
            );
            select::set_slice2(BitMut::new(w, &mut d2), Index::Value(-1), btail.as_ref());
            black_box(select::get_bit(xr, Index::Value(64)));
            black_box(math::clog2(xr));
            black_box(math::clog2_2(b.as_ref()));
        });
    }
}
