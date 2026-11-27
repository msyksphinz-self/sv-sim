//! `WordArena` と `IndexVec` の結合テスト (plan.md §4.4、test-plan M0-TC67〜TC70)。

use svsim_common::{
    ArenaError, ArenaLayoutBuilder, Idx, IndexVec, WordArena, WordOffset, define_index,
};

define_index! {
    /// テスト用のノード ID。
    pub struct NodeId;
}

// ---------------------------------------------------------------------------
// TC67 IndexVec と define_index!
// ---------------------------------------------------------------------------

#[test]
fn tc67_index_vec() {
    let mut v: IndexVec<NodeId, &str> = IndexVec::new();
    let a = v.push("a");
    let b = v.push("b");
    let c = v.push("c");
    assert_eq!([a.index(), b.index(), c.index()], [0, 1, 2]);
    assert_eq!(v.len(), 3);
    assert_eq!(v[b], "b");
    v[b] = "B";
    assert_eq!(v[b], "B");
    let got: Vec<(NodeId, &&str)> = v.iter_enumerated().collect();
    assert_eq!(got, vec![(a, &"a"), (b, &"B"), (c, &"c")]);
    assert_eq!(NodeId::new(2), c);
}

// ---------------------------------------------------------------------------
// TC68 ArenaLayoutBuilder::alloc (Q-15)
// ---------------------------------------------------------------------------

#[test]
fn tc68_alloc_alignment() {
    let mut b = ArenaLayoutBuilder::new();
    assert_eq!(b.alloc(3, 1), Ok(WordOffset(0)));
    assert_eq!(b.alloc(1, 8), Ok(WordOffset(8)));
    assert_eq!(b.alloc(2, 2), Ok(WordOffset(10)));
    let layout = b.finish();
    assert_eq!(layout.len_words(), 12);
    let arena = WordArena::new(&layout);
    assert_eq!(arena.len_words(), 12);
    assert!(arena.words().iter().all(|&w| w == 0));
}

#[test]
fn tc68_alloc_overflow() {
    let mut b = ArenaLayoutBuilder::new();
    assert_eq!(b.alloc(u32::MAX, 1), Ok(WordOffset(0)));
    assert_eq!(b.alloc(1, 1), Err(ArenaError::Overflow));

    // 位置合わせで溢れる
    let mut b = ArenaLayoutBuilder::new();
    assert_eq!(b.alloc(u32::MAX - 3, 1), Ok(WordOffset(0)));
    assert_eq!(b.alloc(1, 8), Err(ArenaError::Overflow));
}

#[test]
fn tc68_alloc_zero_words() {
    // alloc(0, align) は align の検査だけ行い、整列せずに現在位置を返し、次の位置を進めない
    // (Q-15、マネージャ決定。返るオフセットは常にレイアウト長以下)
    let mut b = ArenaLayoutBuilder::new();
    assert_eq!(b.alloc(3, 1), Ok(WordOffset(0)));
    let z = b.alloc(0, 8).unwrap();
    assert_eq!(z, WordOffset(3));
    assert_eq!(b.alloc(0, 3), Err(ArenaError::BadAlign));
    assert_eq!(b.alloc(0, 0), Err(ArenaError::BadAlign));
    let layout = b.finish();
    assert_eq!(layout.len_words(), 3);
    // そのまま長さ 0 のビューに使える
    let mut a = WordArena::new(&layout);
    assert!(a.slice(z, 0).is_empty());
    assert!(a.disjoint_mut([(z, 0), (WordOffset(0), 3)]).is_ok());

    // 溢れる位置でも整列しないので Ok
    let mut b = ArenaLayoutBuilder::new();
    assert_eq!(b.alloc(u32::MAX, 1), Ok(WordOffset(0)));
    assert_eq!(b.alloc(0, 8), Ok(WordOffset(u32::MAX)));

    let mut b = ArenaLayoutBuilder::new();
    assert_eq!(b.alloc(0, 1), Ok(WordOffset(0)));
    assert_eq!(b.alloc(2, 1), Ok(WordOffset(0)));
}

#[test]
fn tc68_alloc_bad_align() {
    for align in [3, 0, 6] {
        let mut b = ArenaLayoutBuilder::new();
        assert_eq!(
            b.alloc(1, align),
            Err(ArenaError::BadAlign),
            "align {align}"
        );
    }
    let mut b = ArenaLayoutBuilder::new();
    assert_eq!(b.alloc(1, 1 << 31), Ok(WordOffset(0)));
}

// ---------------------------------------------------------------------------
// TC69 disjoint_mut (Q-16)
// ---------------------------------------------------------------------------

fn arena12() -> WordArena {
    let mut b = ArenaLayoutBuilder::new();
    b.alloc(12, 1).unwrap();
    WordArena::new(&b.finish())
}

fn r(off: u32, len: u32) -> (WordOffset, u32) {
    (WordOffset(off), len)
}

#[test]
fn tc69_disjoint_ok_and_writes() {
    let mut a = arena12();
    {
        let [x, y] = a.disjoint_mut([r(0, 4), r(4, 4)]).unwrap();
        assert_eq!((x.len(), y.len()), (4, 4));
        x.fill(1);
        y.fill(2);
    }
    assert_eq!(a.words(), &[1, 1, 1, 1, 2, 2, 2, 2, 0, 0, 0, 0]);

    {
        let [x, y] = a.disjoint_mut([r(4, 4), r(0, 4)]).unwrap();
        x[0] = 20;
        y[0] = 10;
    }
    assert_eq!(a.words(), &[10, 1, 1, 1, 20, 2, 2, 2, 0, 0, 0, 0]);
}

#[test]
fn tc69_disjoint_table() {
    let mut a = arena12();
    assert_eq!(
        a.disjoint_mut([r(0, 4), r(3, 2)]).err(),
        Some(ArenaError::Overlap)
    );
    assert_eq!(
        a.disjoint_mut([r(0, 4), r(0, 4)]).err(),
        Some(ArenaError::Overlap)
    );
    assert_eq!(
        a.disjoint_mut([r(10, 4)]).err(),
        Some(ArenaError::OutOfRange)
    );
    assert_eq!(a.disjoint_mut([r(0, 12)]).unwrap()[0].len(), 12);
    let [p, q, s] = a.disjoint_mut([r(0, 1), r(1, 1), r(2, 1)]).unwrap();
    p[0] = 7;
    q[0] = 8;
    s[0] = 9;
    assert_eq!(&a.words()[..3], &[7, 8, 9]);
    // 長さ 0 の区間は重ならない扱い (Q-16)
    let [x, z] = a.disjoint_mut([r(0, 4), r(2, 0)]).unwrap();
    assert_eq!((x.len(), z.len()), (4, 0));
    assert!(a.disjoint_mut([r(5, 0), r(5, 0)]).is_ok());
    assert!(a.disjoint_mut([r(12, 0)]).is_ok());
    assert_eq!(
        a.disjoint_mut([r(13, 0)]).err(),
        Some(ArenaError::OutOfRange)
    );
    // 範囲外の検査は重なりより先
    assert_eq!(
        a.disjoint_mut([r(0, 4), r(0, 4), r(11, 2)]).err(),
        Some(ArenaError::OutOfRange)
    );
    // 非常に大きいオフセットでも溢れずに範囲外になる
    assert_eq!(
        a.disjoint_mut([r(u32::MAX, u32::MAX)]).err(),
        Some(ArenaError::OutOfRange)
    );
    // N = 0
    assert!(a.disjoint_mut::<0>([]).is_ok());
}

#[test]
fn tc69_disjoint_unsorted_many() {
    let mut a = arena12();
    let [w, x, y, z] = a
        .disjoint_mut([r(9, 3), r(0, 2), r(5, 0), r(3, 4)])
        .unwrap();
    w.copy_from_slice(&[90, 91, 92]);
    x.copy_from_slice(&[0, 1]);
    assert!(y.is_empty());
    z.copy_from_slice(&[30, 31, 32, 33]);
    assert_eq!(a.words(), &[0, 1, 0, 30, 31, 32, 33, 0, 0, 90, 91, 92]);
}

// ---------------------------------------------------------------------------
// TC70 スナップショット・復元・content_hash (Q-17、決定性)
// ---------------------------------------------------------------------------

fn arena_of(words: &[u64]) -> WordArena {
    let mut b = ArenaLayoutBuilder::new();
    b.alloc(words.len() as u32, 1).unwrap();
    let mut a = WordArena::new(&b.finish());
    a.words_mut().copy_from_slice(words);
    a
}

/// 固定値 (test-plan TC70。Python で計算済み)。プラットフォーム非依存であることの確認を兼ねる。
#[test]
fn tc70_content_hash_fixed_values() {
    assert_eq!(arena_of(&[]).content_hash(), 0xcbf29ce484222325);
    assert_eq!(arena_of(&[0]).content_hash(), 0xa8c7f832281a39c5);
    assert_eq!(arena_of(&[1]).content_hash(), 0x89cd31291d2aefa4);
    assert_eq!(
        arena_of(&[0, 0x0123456789abcdef]).content_hash(),
        0x2d71366c8ba60175
    );
}

/// content_hash の定義そのもの (LE バイト列の FNV-1a 64) を、ネイティブエンディアンに
/// 依存しない別実装と比べる。
#[test]
fn tc70_content_hash_matches_le_byte_definition() {
    let words = [
        0u64,
        1,
        u64::MAX,
        0x0102_0304_0506_0708,
        0x8000_0000_0000_0000,
    ];
    let mut bytes = Vec::new();
    for w in words {
        for k in 0..8 {
            bytes.push((w >> (8 * k)) as u8); // LE: 下位バイトから
        }
    }
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    assert_eq!(arena_of(&words).content_hash(), h);
}

#[test]
fn tc70_snapshot_restore() {
    let mut a = arena12();
    for (k, w) in a.words_mut().iter_mut().enumerate() {
        *w = (k as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
    let original = a.words().to_vec();
    let s = a.snapshot();
    let h0 = a.content_hash();
    assert_eq!(s.content_hash(), h0);
    assert_eq!(s.len_words(), 12);

    a.slice_mut(WordOffset(3), 2).copy_from_slice(&[1, 2]);
    a.words_mut()[11] ^= 1;
    let h1 = a.content_hash();
    assert_ne!(h1, h0);

    a.restore(&s);
    assert_eq!(a.content_hash(), h0);
    assert_eq!(a.words(), original.as_slice());
}

#[test]
#[should_panic(expected = "snapshot length differs")]
fn tc70_restore_wrong_length_panics() {
    let mut a = arena12();
    let other = arena_of(&[1, 2, 3]);
    a.restore(&other.snapshot());
}

#[test]
fn tc70_restore_wrong_length_leaves_arena_unchanged() {
    let mut a = arena12();
    a.words_mut()[5] = 55;
    let before = a.words().to_vec();
    let snap = arena_of(&[1, 2, 3]).snapshot();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| a.restore(&snap)));
    assert!(r.is_err());
    assert_eq!(a.words(), before.as_slice());
}

/// 決定性: 同じ手順で作ったアリーナは同じハッシュ。
#[test]
fn deterministic_hash_for_same_steps() {
    let run = || {
        let mut b = ArenaLayoutBuilder::new();
        let x = b.alloc(4, 2).unwrap();
        let y = b.alloc(7, 8).unwrap();
        let mut a = WordArena::new(&b.finish());
        a.slice_mut(x, 4).copy_from_slice(&[1, 2, 3, 4]);
        for (k, w) in a.slice_mut(y, 7).iter_mut().enumerate() {
            *w = u64::MAX >> k;
        }
        a.content_hash()
    };
    let h = run();
    for _ in 0..3 {
        assert_eq!(run(), h);
    }
}
