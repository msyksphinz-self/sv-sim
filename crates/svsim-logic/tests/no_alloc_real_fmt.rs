//! 実数との変換と表示用フォーマットがヒープ確保をしないことの検査 (plan.md §5.5、test-plan TC54、
//! 受け入れ基準 L-6、Q-9)。
//!
//! 1. 幅 ≤ 64 の `LogicVec` / `BitVec` の実数変換のメソッド。
//! 2. 任意幅 (65, 129, 1024) の層 2 の実数変換 (作業領域は要らない)。
//! 3. 表示 (`format_int`, `format_int2`): 呼び出し側が `out.reserve(format_len_bound(w, radix))`
//!    (`Min { width: n }` なら n との大きい方) と `Scratch::reserve_for(w)` を済ませていれば 0 回 (Q-9)。
//!
//! 確保の回数は、スレッドローカルなカウンタを持つ `GlobalAlloc` で数える (R-24)。

// このファイルだけ `GlobalAlloc` の実装のために unsafe を使う (plan.md §5.5)。
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;

use svsim_logic::fmt::{self, FieldWidth, FmtRadix};
use svsim_logic::real;
use svsim_logic::{BitVec, Logic, LogicVec, Scratch};

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

/// 1. 幅 ≤ 64 の `LogicVec` / `BitVec` の実数変換。
#[test]
fn layer3_narrow_real_no_alloc() {
    for w in [1u32, 32, 63, 64] {
        let mut x = LogicVec::filled(w, Logic::L1);
        x.set_bit(0, Logic::X);
        let b = BitVec::filled(w, true);
        assert_no_alloc(&format!("real w={w}"), || {
            black_box(black_box(&x).to_f64(true));
            black_box(black_box(&x).to_f32(false));
            black_box(black_box(&b).to_f64(false));
            black_box(black_box(&b).to_f32(true));
            for v in [35.5, -1.5, 1e20, f64::NAN, 5e-324] {
                black_box(LogicVec::from_f64_round(w, black_box(v)));
                black_box(LogicVec::from_f64_trunc(w, black_box(v)));
                black_box(BitVec::from_f64_round(w, black_box(v)));
                black_box(BitVec::from_f64_trunc(w, black_box(v)));
            }
            black_box(real::real_to_bits(black_box(1.0)));
            black_box(real::bits_to_shortreal(black_box(0x3f80_0000)));
        });
    }
}

/// 2. 任意幅の層 2 の実数変換。
#[test]
fn layer2_wide_real_no_alloc() {
    for w in [65u32, 129, 1024] {
        let x = LogicVec::filled(w, Logic::L1);
        let b = BitVec::filled(w, true);
        let mut d = LogicVec::filled(w, Logic::X);
        let mut d2 = BitVec::zero(w);
        assert_no_alloc(&format!("real layer 2 w={w}"), || {
            black_box(real::to_f64(x.as_ref(), true));
            black_box(real::to_f64(x.as_ref(), false));
            black_box(real::to_f32(x.as_ref(), false));
            black_box(real::to_f64_2(b.as_ref(), false));
            black_box(real::to_f32_2(b.as_ref(), true));
            for v in [35.5, -1e300, 1e20, f64::INFINITY] {
                black_box(real::from_f64_round(d.as_mut(), black_box(v)));
                black_box(real::from_f64_trunc(d.as_mut(), black_box(v)));
                black_box(real::from_f64_round2(d2.as_mut(), black_box(v)));
                black_box(real::from_f64_trunc2(d2.as_mut(), black_box(v)));
            }
        });
    }
}

/// 3. 表示: 出力先と作業領域を予約すれば確保しない (Q-9)。
#[test]
fn format_reserved_no_alloc() {
    let radices = [FmtRadix::Bin, FmtRadix::Oct, FmtRadix::Dec, FmtRadix::Hex];
    let widths = [
        FieldWidth::Auto,
        FieldWidth::Min {
            width: 0,
            zero_pad: false,
        },
        FieldWidth::Min {
            width: 40,
            zero_pad: true,
        },
        FieldWidth::Min {
            width: 3,
            zero_pad: false,
        },
    ];
    for w in [1u32, 32, 63, 64, 65, 129, 1024] {
        let mut vals = vec![
            LogicVec::zero(w),
            LogicVec::filled(w, Logic::L1),
            LogicVec::filled(w, Logic::X),
        ];
        let mut min = LogicVec::zero(w);
        min.set_bit(w - 1, Logic::L1);
        vals.push(min);
        let mut mixed = LogicVec::filled(w, Logic::L1);
        mixed.set_bit(0, Logic::Z);
        vals.push(mixed);
        for x in &vals {
            let b = x.to_bit_vec();
            for radix in radices {
                for fw in widths {
                    for signed in [false, true] {
                        let need = match fw {
                            FieldWidth::Auto => fmt::format_len_bound(w, radix),
                            FieldWidth::Min { width, .. } => {
                                fmt::format_len_bound(w, radix).max(width as usize)
                            }
                        };
                        let mut out = String::with_capacity(need);
                        let mut out2 = String::with_capacity(need);
                        let mut s = Scratch::for_width(w);
                        let ctx = format!("format w={w} {radix:?} {fw:?} signed={signed} x={x:?}");
                        assert_no_alloc(&ctx, || {
                            fmt::format_int(&mut out, x.as_ref(), radix, signed, fw, &mut s);
                            fmt::format_int2(&mut out2, b.as_ref(), radix, signed, fw, &mut s);
                        });
                        black_box((out, out2));
                    }
                }
            }
        }
    }
}
