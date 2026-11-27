//! 文字列インターンの結合テスト (plan.md §4.3、test-plan M0-TC66)。

use svsim_common::{Interner, Symbol};

#[test]
fn tc66_basic() {
    let mut i = Interner::new();
    let foo = i.intern("foo");
    assert_eq!(i.intern("foo"), foo);
    let bar = i.intern("bar");
    assert_ne!(foo, bar);
    assert_eq!(i.resolve(foo), "foo");
    assert_eq!(i.resolve(bar), "bar");
    assert_eq!(i.get("baz"), None);
    assert_eq!(i.get("foo"), Some(foo));
}

#[test]
fn tc66_special_strings() {
    let long: String = "ab".repeat(5_000);
    assert_eq!(long.len(), 10_000);
    let inputs = ["", "あいう", "\0", long.as_str(), "Foo", "foo"];
    let mut i = Interner::new();
    let syms: Vec<Symbol> = inputs.iter().map(|s| i.intern(s)).collect();
    for (k, s) in inputs.iter().enumerate() {
        assert_eq!(i.resolve(syms[k]), *s);
        assert_eq!(i.get(s), Some(syms[k]));
        for l in 0..k {
            assert_ne!(syms[k], syms[l], "{s:?} vs {:?}", inputs[l]);
        }
    }
    // 大文字小文字だけ違う文字列は別の Symbol
    assert_ne!(i.get("Foo"), i.get("foo"));
    assert_eq!(i.len(), inputs.len());
}

/// 決定的な擬似乱数 (xorshift64*)。proptest は svsim-common の依存に無いので自前で生成する。
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// 重複の多い文字列の列を作る。
fn gen_strings(rng: &mut Rng, count: usize) -> Vec<String> {
    const ALPHA: &[char] = &['a', 'b', 'A', '_', '0', 'あ', '\0', '$'];
    (0..count)
        .map(|_| {
            let len = rng.below(4) as usize; // 長さ 0〜3 で衝突させる
            (0..len)
                .map(|_| ALPHA[rng.below(ALPHA.len() as u64) as usize])
                .collect()
        })
        .collect()
}

/// TC66 の性質: intern の結果が等しい ⇔ 文字列が等しい。全部 resolve で戻る。
#[test]
fn tc66_property_eq_iff_same_string() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for case in 0..64 {
        // 最初の 2 件は境界 (10,000 個と 1 個)、残りは 1〜300 個
        let count = match case {
            0 => 10_000,
            1 => 1,
            _ => 1 + rng.below(300) as usize,
        };
        let strs = gen_strings(&mut rng, count);
        let mut i = Interner::new();
        let syms: Vec<Symbol> = strs.iter().map(|s| i.intern(s)).collect();
        for (k, s) in strs.iter().enumerate() {
            assert_eq!(i.resolve(syms[k]), s.as_str());
        }
        // 全ペアは重いので、隣接と一定間隔のペアを見る
        for k in 0..strs.len() {
            for l in [k / 2, k.saturating_sub(1), k.saturating_sub(7)] {
                assert_eq!(syms[k] == syms[l], strs[k] == strs[l]);
            }
        }
        // 異なる文字列の数 = 登録数
        let mut uniq = strs.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(i.len(), uniq.len());
    }
}

/// 決定性 (PLAN §1 原則 2): 同じ入力列なら、別の Interner でも同じ Symbol 番号の列になる。
#[test]
fn deterministic_ids_for_same_input() {
    let mut rng = Rng(42);
    let strs = gen_strings(&mut rng, 2_000);
    let run = || {
        let mut i = Interner::new();
        strs.iter()
            .map(|s| i.intern(s).as_u32())
            .collect::<Vec<u32>>()
    };
    let first = run();
    for _ in 0..4 {
        assert_eq!(run(), first);
    }
    // 番号は登録順に 0 から密に振られる
    let max = *first.iter().max().unwrap();
    let mut seen = vec![false; max as usize + 1];
    let mut next = 0;
    for &n in &first {
        if !seen[n as usize] {
            assert_eq!(n, next, "new symbols are numbered in registration order");
            seen[n as usize] = true;
            next += 1;
        }
    }
}
