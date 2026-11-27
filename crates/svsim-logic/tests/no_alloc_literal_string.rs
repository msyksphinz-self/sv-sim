//! リテラル解析・文字列変換・atoi 系がヒープ確保をしないことの検査 (plan.md §5.5、test-plan TC54、
//! 受け入れ基準 L-6)。
//!
//! 1. 結果の幅が 64 以下のリテラル (10 進は 38 桁以下) の解析と、そのエラー。
//! 2. `from_string_bytes` (任意幅、事前に確保した出力先)、幅 ≤ 64 の `LogicVec`/`BitVec` の
//!    `from_string_bytes`、容量を確保済みの `Vec` への `write_string_bytes`、`atoi` 系。
//!
//! 確保の回数は、スレッドローカルなカウンタを持つ `GlobalAlloc` で数える (R-24: テストハーネスは
//! `#[test]` を並列のスレッドで走らせるため)。カウンタは `const` 初期化の `thread_local!` なので、
//! それ自身は確保しない。

// このファイルだけ `GlobalAlloc` の実装のために unsafe を使う (plan.md §5.5)。
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;

use svsim_logic::literal::{Radix, parse_based, parse_unsized_decimal};
use svsim_logic::string::{
    atoi, atoi_u32, from_string_bytes, from_string_bytes2, write_string_bytes, write_string_bytes2,
};
use svsim_logic::{BitVec, Logic, LogicVec};

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
    // 幅 65 のリテラルは値をヒープに置くので確保する。
    let n = count_allocs(|| drop(black_box(parse_based(Some(65), false, Radix::Hex, "1"))));
    assert!(n >= 1);
}

/// 1. 幅 ≤ 64 のリテラル (§5.7.1)。
#[test]
fn literal_narrow_no_alloc() {
    let cases: &[(Option<u32>, Radix, &str)] = &[
        (Some(8), Radix::Bin, "x1"),
        (Some(64), Radix::Hex, "dead_beef_cafe_f00d"),
        (Some(4), Radix::Hex, "fff"),
        (Some(63), Radix::Oct, "7777_7777_7777_7777_7777_7"),
        (None, Radix::Hex, "x"),
        (None, Radix::Hex, "1_0000_0000"),
        (Some(8), Radix::Dec, "x_"),
        (Some(64), Radix::Dec, "18446744073709551616"),
        (
            Some(8),
            Radix::Dec,
            "0000000000000000000000000000000000000000000000000255",
        ),
        (None, Radix::Dec, "18446744073709551615"),
        (None, Radix::Dec, "9999999999999999999"),
        (None, Radix::Dec, "z"),
        // エラー
        (Some(0), Radix::Bin, "1"),
        (Some(8), Radix::Hex, "_1"),
        (Some(8), Radix::Hex, "g"),
        (Some(8), Radix::Dec, "1x"),
        (Some(8), Radix::Oct, ""),
    ];
    for &(size, radix, digits) in cases {
        assert_no_alloc(
            &format!("parse_based({size:?}, {radix:?}, {digits:?})"),
            || {
                drop(black_box(parse_based(
                    black_box(size),
                    false,
                    radix,
                    black_box(digits),
                )));
            },
        );
    }
    for digits in [
        "0",
        "123",
        "4294967296",
        "18446744073709551615",
        "x",
        "1a",
        "",
    ] {
        assert_no_alloc(&format!("parse_unsized_decimal({digits:?})"), || {
            drop(black_box(parse_unsized_decimal(black_box(digits))));
        });
    }
}

/// 2. 文字列変換と atoi 系 (§5.9, §6.16, §6.16.9)。
#[test]
fn string_no_alloc() {
    let s: &[u8] = b"Hello, world! 0123456789";
    for w in [1u32, 8, 63, 64] {
        assert_no_alloc(&format!("from_string_bytes w={w}"), || {
            black_box(LogicVec::from_string_bytes(w, black_box(s)));
            black_box(BitVec::from_string_bytes(w, black_box(s)));
            black_box(LogicVec::from_string_bytes(w, black_box(b"")));
        });
    }
    // 任意幅のスライスカーネルに、事前に確保した出力先を渡す。
    for w in [65u32, 129, 1024] {
        let mut d = LogicVec::filled(w, Logic::X);
        let mut d2 = BitVec::filled(w, true);
        let mut out = Vec::with_capacity(w as usize / 8 + 1);
        assert_no_alloc(&format!("string kernels w={w}"), || {
            from_string_bytes(d.as_mut(), black_box(s));
            from_string_bytes2(d2.as_mut(), black_box(s));
            out.clear();
            write_string_bytes(d.as_ref(), &mut out);
            out.clear();
            write_string_bytes2(d2.as_ref(), &mut out);
        });
        assert_eq!(out, d2.to_string_bytes());
    }
    assert_no_alloc("atoi", || {
        black_box(atoi(black_box(b"4294967297"), Radix::Dec));
        black_box(atoi_u32(black_box(b"dead_beef"), Radix::Hex));
        black_box(atoi_u32(black_box(b"777"), Radix::Oct));
        black_box(atoi_u32(black_box(b"1012"), Radix::Bin));
    });
}
