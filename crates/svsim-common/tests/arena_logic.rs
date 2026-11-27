//! アリーナとロジック値ビューの結合テスト (plan.md §4.4、test-plan M0-TC71)。
//!
//! アリーナ上に 4 値と 2 値の信号を置き、`svsim-logic` のビューで in-place に演算し、
//! スナップショット → 変更 → 復元で元に戻ることを示す。

use svsim_common::{ArenaLayoutBuilder, WordArena};
use svsim_logic::ops::arith::add_assign2;
use svsim_logic::ops::bitwise::and_assign;
use svsim_logic::{BitMut, BitRef, Logic, LogicMut, LogicRef, LogicVec, words_for};

const W4: u32 = 65; // 4 値信号 A, B の幅
const W2: u32 = 64; // 2 値信号 C の幅

#[test]
fn tc71_arena_with_logic_views() {
    // 1. 配置: A, B は 4 値 65 ビット (プレーン分離で 2 * 2 = 4 ワード)、C は 2 値 64 ビット (1 ワード)
    let planar = 2 * words_for(W4) as u32;
    assert_eq!(planar, 4);
    let mut b = ArenaLayoutBuilder::new();
    let a_off = b.alloc(planar, 2).unwrap();
    let b_off = b.alloc(planar, 2).unwrap();
    let c_off = b.alloc(words_for(W2) as u32, 1).unwrap();
    let mut arena = WordArena::new(&b.finish());
    assert_eq!(arena.len_words(), 9);

    // 0 初期化は 4 値でも 0 を意味する
    let zero = LogicVec::from_ref(LogicRef::from_planar(W4, arena.slice(a_off, planar)));
    assert_eq!(zero, LogicVec::filled(W4, Logic::L0));

    // 2. A = 65'h1_0000_0000_0000_0001 (bit 3 を x に)、B = 65'h0_ffff_ffff_ffff_ffff
    {
        let mut a = LogicMut::from_planar(W4, arena.slice_mut(a_off, planar));
        a.set_bit(0, Logic::L1);
        a.set_bit(64, Logic::L1);
        a.set_bit(3, Logic::X);
    }
    {
        let mut bv = LogicMut::from_planar(W4, arena.slice_mut(b_off, planar));
        bv.fill(Logic::L1);
        bv.set_bit(64, Logic::L0);
    }
    let b_before = arena.slice(b_off, planar).to_vec();

    // 3. disjoint_mut で A と B を同時に取り、A &= B を in-place で (IEEE 1800-2023 §11.4.8)
    {
        let [a_words, b_words] = arena
            .disjoint_mut([(a_off, planar), (b_off, planar)])
            .unwrap();
        and_assign(
            LogicMut::from_planar(W4, a_words),
            LogicRef::from_planar(W4, b_words),
        );
    }

    // 4. A は bit 0 = 1、bit 3 = x (x & 1 = x、表 11-7)、bit 64 = 1 & 0 = 0、他は 0。B は不変。
    let mut expected = LogicVec::filled(W4, Logic::L0);
    expected.set_bit(0, Logic::L1);
    expected.set_bit(3, Logic::X);
    let a_after = LogicVec::from_ref(LogicRef::from_planar(W4, arena.slice(a_off, planar)));
    assert_eq!(a_after, expected);
    assert_eq!(arena.slice(b_off, planar), b_before.as_slice());

    // 5. C を BitMut で書き、in-place に 1 を足して桁上げを確認する (add_assign2、§11.4.3)。
    {
        let mut c = BitMut::new(W2, arena.slice_mut(c_off, 1));
        c.fill(false);
        for i in 0..32 {
            c.set_bit(i, true);
        }
        let one = [1u64];
        add_assign2(c, BitRef::new(W2, &one));
    }
    let c = BitRef::new(W2, arena.slice(c_off, 1));
    assert_eq!(c.words(), &[0x0000_0001_0000_0000]);
    assert!(c.bit(32));
    assert!(!c.bit(31));

    // 6. snapshot → A を fill(X) → restore で、A が手順 4 の値に戻り、ハッシュも戻る
    let snap = arena.snapshot();
    let h0 = arena.content_hash();
    LogicMut::from_planar(W4, arena.slice_mut(a_off, planar)).fill(Logic::X);
    assert_ne!(arena.content_hash(), h0);
    assert_eq!(
        LogicVec::from_ref(LogicRef::from_planar(W4, arena.slice(a_off, planar))),
        LogicVec::filled(W4, Logic::X)
    );
    arena.restore(&snap);
    assert_eq!(arena.content_hash(), h0);
    let a_restored = LogicVec::from_ref(LogicRef::from_planar(W4, arena.slice(a_off, planar)));
    assert_eq!(a_restored, expected);
}
