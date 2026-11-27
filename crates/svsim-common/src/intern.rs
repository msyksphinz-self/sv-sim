//! 文字列インターン (単一スレッド版、plan.md §4.3、タスク M0-T11)。
//!
//! 識別子などの文字列を [`Symbol`] (32 ビットの番号) に置き換え、比較とハッシュを安くする。
//!
//! # 規則: `Symbol` の番号順を出力の順序に使わない
//! [`Symbol`] の番号は「登録した順」に振られる。単一スレッドで同じ順に登録すれば同じ番号になるが、
//! 将来パースを並列化すると、登録の順序 (= 番号) が実行ごとに変わりうる。
//! 決定性 (PLAN §1 原則 2) を保つため、**診断・ダンプ・ファイル出力などの順序を `Symbol` の番号で
//! 決めてはならない**。順序が必要なら、文字列そのもの ([`Interner::resolve`]) やソース位置で並べる。
//! この理由で `Symbol` は `Ord` / `PartialOrd` を実装しない。
//!
//! # スレッド安全性
//! M0 の API は `&mut self` を取る単一スレッド版。並列パース用の版は M1 以降で決める
//! (plan.md §4.3)。

use rustc_hash::FxHashMap;

/// インターンした文字列の番号。
///
/// 同じ [`Interner`] から得た `Symbol` どうしは、等しい ⇔ 文字列が等しい。
/// 別の `Interner` の `Symbol` と比べても意味は無い。
///
/// 番号の順序を出力の順序に使ってはならない (モジュールの doc を参照)。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Symbol(u32);

impl Symbol {
    /// 内部の番号 (0 始まり、登録順)。
    ///
    /// 表のキーなどに使ってよいが、出力の順序には使わないこと。
    #[inline]
    pub fn as_u32(self) -> u32 {
        self.0
    }
}

/// 文字列インターン表 (単一スレッド版)。
///
/// 同じ文字列の列を同じ順に [`intern`](Self::intern) すれば、同じ `Symbol` の列が得られる
/// (ハッシュ関数は FxHash でシード無し。結果はハッシュに依存しない)。
#[derive(Default, Clone, Debug)]
pub struct Interner {
    map: FxHashMap<Box<str>, Symbol>,
    strings: Vec<Box<str>>,
}

impl Interner {
    /// 空の表を作る。
    pub fn new() -> Self {
        Self::default()
    }

    /// `s` を登録して、その `Symbol` を返す。登録済みなら既存の `Symbol` を返す。
    ///
    /// # Panics
    /// 登録数が `u32::MAX` 個を超える場合。
    pub fn intern(&mut self, s: &str) -> Symbol {
        if let Some(&sym) = self.map.get(s) {
            return sym;
        }
        let n = u32::try_from(self.strings.len())
            .ok()
            .filter(|&n| n < u32::MAX)
            .expect("Interner::intern: too many symbols (u32 overflow)");
        let sym = Symbol(n);
        let boxed: Box<str> = s.into();
        self.strings.push(boxed.clone());
        self.map.insert(boxed, sym);
        sym
    }

    /// `s` が登録済みならその `Symbol` を返す。登録はしない。
    pub fn get(&self, s: &str) -> Option<Symbol> {
        self.map.get(s).copied()
    }

    /// `sym` の文字列。
    ///
    /// # Panics
    /// `sym` がこの表から得たものでない (番号が範囲外の) 場合。
    pub fn resolve(&self, sym: Symbol) -> &str {
        match self.strings.get(sym.0 as usize) {
            Some(s) => s,
            None => panic!(
                "Interner::resolve: symbol {} is not from this interner (len {})",
                sym.0,
                self.strings.len()
            ),
        }
    }

    /// 登録した文字列の数。
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// 1 つも登録していなければ `true`。
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_and_resolve() {
        let mut i = Interner::new();
        assert!(i.is_empty());
        let a = i.intern("foo");
        let b = i.intern("bar");
        assert_ne!(a, b);
        assert_eq!(i.intern("foo"), a);
        assert_eq!(i.resolve(a), "foo");
        assert_eq!(i.resolve(b), "bar");
        assert_eq!(i.len(), 2);
        assert_eq!(i.get("foo"), Some(a));
        assert_eq!(i.get("baz"), None);
        // get は登録しない
        assert_eq!(i.len(), 2);
    }

    #[test]
    fn numbering_is_registration_order() {
        let mut i = Interner::new();
        assert_eq!(i.intern("x").as_u32(), 0);
        assert_eq!(i.intern("y").as_u32(), 1);
        assert_eq!(i.intern("x").as_u32(), 0);
        assert_eq!(i.intern("z").as_u32(), 2);
    }

    #[test]
    #[should_panic(expected = "not from this interner")]
    fn resolve_foreign_symbol_panics() {
        let mut big = Interner::new();
        big.intern("a");
        let s = big.intern("b");
        let mut small = Interner::new();
        small.intern("a");
        let _ = small.resolve(s);
    }
}
