//! ヒープ確保 0 回の検査: 枠と、T3a/T3b の範囲 (構築・変換・ビュー・層 1 の load/store・
//! `Scratch`・DPI 形式との変換) (plan.md §5.5、test-plan TC54。タスク M0-T3b)。
//!
//! 確保の回数を数える `GlobalAlloc` を設定する。テストハーネスは `#[test]` を並列のスレッドで
//! 走らせるので、カウンタはスレッドローカルにする (R-24。`const` 初期化の `thread_local!` は確保しない)。
//!
//! 他の分野 (T4〜T6) の `no_alloc_<area>.rs` は、このファイルの [`CountingAlloc`] と [`count_allocs`] を
//! 写して使う (統合テストのファイル同士はコードを共有しないため)。

// GlobalAlloc の実装に unsafe が要る。このファイルに限って許す (plan.md §5.5、R-1)。
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;

use svsim_logic::abi::{
    read_sv_bit_vec_val, read_sv_logic_vec_val, sv_bit_vec_val_len, sv_logic_vec_val_len,
    write_sv_bit_vec_val, write_sv_logic_vec_val,
};
use svsim_logic::scalar::{self, L64};
use svsim_logic::{BitMut, BitRef, BitVec, Logic, LogicMut, LogicRef, LogicVec, Scratch};

thread_local! {
    /// このスレッドでの確保の回数 (alloc / alloc_zeroed / realloc)。
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

/// 確保の回数をスレッドごとに数え、実際の確保は [`System`] に任せるアロケータ。
struct CountingAlloc;

impl CountingAlloc {
    #[inline]
    fn bump() {
        // スレッドの終了処理中 (TLS の破棄後) は数えない。
        let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
    }
}

// SAFETY: 実際の確保・解放はすべて `System` にそのまま委ねる。追加の処理はスレッドローカルの
// `Cell<usize>` の加算だけで、これは確保をしない (`const` 初期化の thread_local) ので再入しない。
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        Self::bump();
        // SAFETY: 呼び出し側が `GlobalAlloc::alloc` の契約を満たしており、それをそのまま渡す。
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        Self::bump();
        // SAFETY: 同上。
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` は同じ `layout` でこのアロケータ (= `System`) から得たもの。
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        Self::bump();
        // SAFETY: `ptr` は同じ `layout` で `System` から得たもので、`new_size` の契約は呼び出し側が満たす。
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

/// `f` の実行中にこのスレッドで起きた確保の回数を数える。`f` の戻り値の解放は数えない。
fn count_allocs<R>(f: impl FnOnce() -> R) -> (usize, R) {
    let before = ALLOCS.with(Cell::get);
    let r = f();
    let after = ALLOCS.with(Cell::get);
    (after - before, r)
}

/// `f` が確保をしないことを検査する。
#[track_caller]
fn assert_no_alloc<R>(what: &str, f: impl FnOnce() -> R) -> R {
    let (n, r) = count_allocs(f);
    assert_eq!(n, 0, "{what}: {n} heap allocation(s)");
    r
}

const ALL_LOGIC: [Logic; 4] = [Logic::L0, Logic::L1, Logic::X, Logic::Z];

/// 層 3 のインライン版の幅 (TC54.1)。
const SMALL_WIDTHS: [u32; 4] = [1, 32, 63, 64];

/// 層 2 の任意幅 (TC54.2)。
const WIDE_WIDTHS: [u32; 3] = [65, 129, 1024];

// ===========================================================================
// 検査器の自己テスト
// ===========================================================================

/// 計測区間で意図的に確保すると 1 回以上を数える (TC54 の検査器の自己テスト)。
#[test]
fn counter_detects_allocation() {
    let (n, v) = count_allocs(|| black_box(Vec::<u8>::with_capacity(1)));
    assert!(n >= 1, "counted {n}");
    drop(v);
    let (n, _) = count_allocs(|| black_box(Box::new(1u64)));
    assert!(n >= 1);
    let (n, _) = count_allocs(|| black_box(1 + 1));
    assert_eq!(n, 0);
    // 幅 65 の LogicVec はヒープ版なので確保する (計測が効いていることの確認)。
    let (n, _) = count_allocs(|| black_box(LogicVec::zero(65)));
    assert!(n >= 1);
}

// ===========================================================================
// TC54.1: 幅 ≤ 64 の LogicVec / BitVec の中核の操作
// ===========================================================================

#[test]
fn small_logic_vec_core_ops_do_not_allocate() {
    for w in SMALL_WIDTHS {
        assert_no_alloc("LogicVec core ops", || {
            let z = black_box(LogicVec::zero(w));
            let one = black_box(LogicVec::from_u64(w, 0x1234_5678_9abc_def1));
            let neg = black_box(LogicVec::from_i64(w, -3));
            let mut acc = 0u64;
            for v in ALL_LOGIC {
                let f = black_box(LogicVec::filled(w, v));
                acc ^= f.has_unknown() as u64;
            }
            let mut x = one.clone();
            for v in ALL_LOGIC {
                x.set_bit(w - 1, v);
                acc ^= x.bit(w - 1) as u64;
                acc ^= x.bit(0) as u64;
            }
            {
                let mut m = x.as_mut();
                m.fill(Logic::X);
                m.set_bit(0, Logic::L1);
                m.copy_from(neg.as_ref());
                m.normalize();
                acc ^= m.as_ref().is_normalized() as u64;
            }
            acc ^= one.to_u64().unwrap_or(0);
            acc ^= neg.to_i64(true).unwrap_or(0) as u64;
            acc ^= neg.to_i64(false).unwrap_or(0) as u64;
            let b = black_box(one.to_bit_vec());
            let back = black_box(LogicVec::from(&b));
            let back2 = black_box(LogicVec::from(b.clone()));
            acc ^= (back == back2) as u64;
            acc ^= (z == one) as u64;
            acc ^= LogicVec::from(Logic::Z).has_unknown() as u64;
            acc ^= LogicVec::from_ref(one.as_ref()).to_u64().unwrap_or(0);
            black_box(acc);
        });
    }
}

#[test]
fn small_bit_vec_core_ops_do_not_allocate() {
    for w in SMALL_WIDTHS {
        assert_no_alloc("BitVec core ops", || {
            let z = black_box(BitVec::zero(w));
            let f = black_box(BitVec::filled(w, true));
            let one = black_box(BitVec::from_u64(w, 0xfedc_ba98_7654_3210));
            let neg = black_box(BitVec::from_i64(w, -5));
            let mut x = one.clone();
            x.set_bit(w - 1, true);
            x.set_bit(0, false);
            let mut acc = x.bit(w - 1) as u64 ^ x.bit(0) as u64;
            {
                let mut m = x.as_mut();
                m.fill(true);
                m.copy_from(neg.as_ref());
                m.normalize();
                acc ^= m.as_ref().is_normalized() as u64;
            }
            acc ^= one.to_u64().unwrap_or(0);
            acc ^= neg.to_i64(true).unwrap_or(0) as u64;
            acc ^= (z == f) as u64;
            acc ^= BitVec::from(true).to_u64().unwrap_or(0);
            acc ^= black_box(one.to_logic_vec()).to_u64().unwrap_or(0);
            acc ^= BitVec::from_ref(one.as_ref()).to_u64().unwrap_or(0);
            black_box(acc);
        });
    }
}

/// 層 1 (scalar) の値と load/store、共通の補助関数。
#[test]
fn scalar_core_does_not_allocate() {
    let mut buf = [0u64; 2];
    let mut buf2 = [0u64; 1];
    for w in SMALL_WIDTHS {
        assert_no_alloc("scalar core", || {
            let v = L64::new(0xdead_beef, 0x0f0f).masked(w);
            v.store(LogicMut::from_planar(w, &mut buf));
            let back = L64::load(LogicRef::from_planar(w, &buf));
            let mut acc = (back == v) as u64;
            acc ^= L64::filled(Logic::X, w).a ^ L64::all_x(w).b ^ L64::from_u64(7, w).a;
            acc ^= back.bit(w - 1) as u64 ^ back.to_two_state();
            acc ^= scalar::sign_extend(back.a, w) ^ scalar::extend(back.a, w, false);
            scalar::store2(0x55, BitMut::new(w, &mut buf2));
            acc ^= scalar::load2(BitRef::new(w, &buf2));
            black_box(acc);
        });
    }
}

// ===========================================================================
// TC54.2: 任意幅のビュー (層 2)、DPI 形式との変換、Scratch
// ===========================================================================

#[test]
fn wide_views_do_not_allocate() {
    for w in WIDE_WIDTHS {
        // 確保は計測区間の外で済ませる。
        let src = LogicVec::from_i64(w, -12345);
        let mut dst = LogicVec::filled(w, Logic::X);
        let mut planar = vec![0u64; 2 * svsim_logic::words_for(w)];
        let bsrc = BitVec::from_i64(w, -6789);
        let mut bdst = BitVec::zero(w);
        assert_no_alloc("wide views", || {
            let r = src.as_ref();
            let mut acc = r.has_unknown() as u64 ^ r.is_normalized() as u64;
            acc ^= r.bit(w - 1) as u64 ^ r.bit(64) as u64;
            {
                let mut m = dst.as_mut();
                m.fill(Logic::Z);
                for v in ALL_LOGIC {
                    m.set_bit(w - 1, v);
                    m.set_bit(64, v);
                }
                m.copy_from(r);
                m.reborrow().normalize();
                acc ^= m.bit(0) as u64;
            }
            {
                let mut p = LogicMut::from_planar(w, &mut planar);
                p.copy_from(r);
                acc ^= p.into_ref().bit(3) as u64;
            }
            acc ^= LogicRef::from_planar(w, &planar).bit(w - 1) as u64;
            {
                let mut bm = bdst.as_mut();
                bm.fill(true);
                bm.set_bit(w - 1, false);
                bm.copy_from(bsrc.as_ref());
                acc ^= bm.as_ref().bit(w - 1) as u64;
            }
            black_box(acc);
        });
    }
}

#[test]
fn dpi_conversion_does_not_allocate() {
    for w in SMALL_WIDTHS.iter().chain(&WIDE_WIDTHS).copied() {
        let src = LogicVec::from_i64(w, -987_654_321);
        let mut dst = LogicVec::zero(w);
        let mut buf = vec![0u32; sv_logic_vec_val_len(w)];
        let bsrc = BitVec::from_i64(w, -42);
        let mut bdst = BitVec::zero(w);
        let mut bbuf = vec![0u32; sv_bit_vec_val_len(w)];
        assert_no_alloc("DPI conversion", || {
            write_sv_logic_vec_val(src.as_ref(), &mut buf);
            read_sv_logic_vec_val(&buf, dst.as_mut());
            write_sv_bit_vec_val(bsrc.as_ref(), &mut bbuf);
            read_sv_bit_vec_val(&bbuf, bdst.as_mut());
        });
        assert_eq!(dst, src);
        assert_eq!(bdst, bsrc);
    }
}

/// `Scratch::reserve_for(width)` で事前確保した作業領域は、その幅の分までなら確保しない。
#[test]
fn reserved_scratch_does_not_allocate() {
    for w in SMALL_WIDTHS.iter().chain(&WIDE_WIDTHS).copied() {
        let need = Scratch::words_needed(w);
        let mut s = Scratch::new();
        s.reserve_for(w);
        assert_no_alloc("Scratch::words/parts", || {
            s.reserve_for(w); // 容量が足りていれば何もしない
            black_box(s.words(need));
            let [a, b, c] = s.parts([need / 2, need / 4, need / 4]);
            a.fill(1);
            b.fill(2);
            c.fill(3);
            black_box(s.words(1));
        });

        // 入れ子用の内側の段も reserve_nested で確保しておけば、split_words は確保しない。
        let mut s = Scratch::new();
        s.reserve_nested(w, 2);
        assert_no_alloc("Scratch::split_words", || {
            let (mine, inner) = s.split_words(need);
            mine.fill(1);
            let (m2, inner2) = inner.split_words(need);
            m2.fill(2);
            black_box(inner2.words(need));
        });

        // 内側の段を予約しなくても、2 回目以降は使い回す (確保は初回だけ)。
        let mut s = Scratch::for_width(w);
        let (n, _) = count_allocs(|| {
            let (_, inner) = s.split_words(1);
            black_box(inner.words(need));
        });
        assert!(n >= 1, "first split_words allocates the inner level");
        assert_no_alloc("Scratch::split_words (warm)", || {
            let (_, inner) = s.split_words(1);
            black_box(inner.words(need));
        });
    }
}
