//! 型付きインデックスと `IndexVec` (plan.md §4.4 (1)、タスク M0-T11)。
//!
//! コンパイラのデータ構造 (AST、型表、階層など) を「`Vec` + 型付きの添字」で持つための部品。
//! 添字の型を分けることで、別の表の添字を取り違えるとコンパイルエラーになる。
//!
//! ```
//! use svsim_common::{define_index, IndexVec};
//!
//! define_index! {
//!     /// AST ノードの ID。
//!     pub struct NodeId;
//! }
//!
//! let mut nodes: IndexVec<NodeId, &str> = IndexVec::new();
//! let a = nodes.push("a");
//! let b = nodes.push("b");
//! assert_eq!(nodes[b], "b");
//! nodes[a] = "A";
//! let all: Vec<_> = nodes.iter_enumerated().collect();
//! assert_eq!(all, vec![(a, &"A"), (b, &"b")]);
//! ```

use std::fmt;
use std::hash::Hash;
use std::marker::PhantomData;
use std::ops::{Index, IndexMut};

/// 型付きインデックス。`usize` の添字と相互に変換できる。
pub trait Idx: Copy + Eq + Ord + Hash {
    /// 添字 `i` から作る。
    ///
    /// # Panics
    /// 実装が表せる範囲を超える場合 ([`define_index!`](crate::define_index) の型は `u32` を超える場合)。
    fn new(i: usize) -> Self;
    /// `usize` の添字に戻す。
    fn index(self) -> usize;
}

impl Idx for usize {
    #[inline]
    fn new(i: usize) -> Self {
        i
    }
    #[inline]
    fn index(self) -> usize {
        self
    }
}

impl Idx for u32 {
    #[inline]
    fn new(i: usize) -> Self {
        u32::try_from(i).expect("Idx::new: index does not fit in u32")
    }
    #[inline]
    fn index(self) -> usize {
        self as usize
    }
}

/// `u32` の newtype の型付きインデックスを定義する。
///
/// 生成される型は `Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug` を実装し、
/// [`Idx`] を実装する。属性 (doc コメントなど) を前に付けられる。
/// `from_u32` / `as_u32` も生成する。
///
/// ```
/// svsim_common::define_index! {
///     /// 型表の ID。
///     pub struct TypeId;
/// }
/// use svsim_common::Idx;
/// assert_eq!(TypeId::new(3).index(), 3);
/// assert_eq!(TypeId::from_u32(7).as_u32(), 7);
/// ```
#[macro_export]
macro_rules! define_index {
    ($(#[$meta:meta])* $vis:vis struct $name:ident;) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        $vis struct $name(u32);

        impl $name {
            /// 生の番号から作る。
            #[inline]
            #[allow(dead_code)]
            $vis const fn from_u32(raw: u32) -> Self {
                $name(raw)
            }

            /// 生の番号。
            #[inline]
            #[allow(dead_code)]
            $vis const fn as_u32(self) -> u32 {
                self.0
            }
        }

        impl $crate::index::Idx for $name {
            #[inline]
            fn new(i: usize) -> Self {
                match u32::try_from(i) {
                    Ok(raw) => $name(raw),
                    Err(_) => panic!(
                        concat!(stringify!($name), "::new: index {} does not fit in u32"),
                        i
                    ),
                }
            }
            #[inline]
            fn index(self) -> usize {
                self.0 as usize
            }
        }
    };
}

/// 型付きインデックス `I` で引く `Vec<T>`。
///
/// [`push`](Self::push) は 0, 1, 2, … の順の添字を返す。
pub struct IndexVec<I: Idx, T> {
    raw: Vec<T>,
    _m: PhantomData<fn(&I)>,
}

impl<I: Idx, T> IndexVec<I, T> {
    /// 空の表を作る。
    #[inline]
    pub fn new() -> Self {
        IndexVec {
            raw: Vec::new(),
            _m: PhantomData,
        }
    }

    /// 容量を指定して空の表を作る。
    #[inline]
    pub fn with_capacity(cap: usize) -> Self {
        IndexVec {
            raw: Vec::with_capacity(cap),
            _m: PhantomData,
        }
    }

    /// `Vec` から作る。要素 `k` の添字は `I::new(k)`。
    #[inline]
    pub fn from_raw(raw: Vec<T>) -> Self {
        IndexVec {
            raw,
            _m: PhantomData,
        }
    }

    /// 末尾に追加して、その添字を返す。
    ///
    /// # Panics
    /// 添字が `I` で表せない場合。
    #[inline]
    pub fn push(&mut self, v: T) -> I {
        let id = I::new(self.raw.len());
        self.raw.push(v);
        id
    }

    /// 次に [`push`](Self::push) したときの添字。
    #[inline]
    pub fn next_index(&self) -> I {
        I::new(self.raw.len())
    }

    /// 要素数。
    #[inline]
    pub fn len(&self) -> usize {
        self.raw.len()
    }

    /// 空なら `true`。
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    /// 添字 `i` の要素。範囲外なら `None`。
    #[inline]
    pub fn get(&self, i: I) -> Option<&T> {
        self.raw.get(i.index())
    }

    /// 添字 `i` の要素 (可変)。範囲外なら `None`。
    #[inline]
    pub fn get_mut(&mut self, i: I) -> Option<&mut T> {
        self.raw.get_mut(i.index())
    }

    /// 要素を添字の順に返す。
    #[inline]
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.raw.iter()
    }

    /// 要素を添字の順に可変で返す。
    #[inline]
    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, T> {
        self.raw.iter_mut()
    }

    /// `(添字, &要素)` を添字の順に返す。
    #[inline]
    pub fn iter_enumerated(&self) -> impl DoubleEndedIterator<Item = (I, &T)> + ExactSizeIterator {
        self.raw.iter().enumerate().map(|(k, v)| (I::new(k), v))
    }

    /// `(添字, &mut 要素)` を添字の順に返す。
    #[inline]
    pub fn iter_enumerated_mut(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = (I, &mut T)> + ExactSizeIterator {
        self.raw.iter_mut().enumerate().map(|(k, v)| (I::new(k), v))
    }

    /// 全添字を順に返す。
    #[inline]
    pub fn indices(&self) -> impl DoubleEndedIterator<Item = I> + ExactSizeIterator + use<I, T> {
        (0..self.raw.len()).map(I::new)
    }

    /// 中身のスライス。
    #[inline]
    pub fn as_slice(&self) -> &[T] {
        &self.raw
    }

    /// 中身の可変スライス。
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.raw
    }

    /// 中身の `Vec` に戻す。
    #[inline]
    pub fn into_raw(self) -> Vec<T> {
        self.raw
    }
}

impl<I: Idx, T> Default for IndexVec<I, T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<I: Idx, T: Clone> Clone for IndexVec<I, T> {
    #[inline]
    fn clone(&self) -> Self {
        Self::from_raw(self.raw.clone())
    }
}

impl<I: Idx, T: PartialEq> PartialEq for IndexVec<I, T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

impl<I: Idx, T: Eq> Eq for IndexVec<I, T> {}

impl<I: Idx, T: fmt::Debug> fmt::Debug for IndexVec<I, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.raw, f)
    }
}

impl<I: Idx, T> Index<I> for IndexVec<I, T> {
    type Output = T;
    #[inline]
    fn index(&self, i: I) -> &T {
        &self.raw[i.index()]
    }
}

impl<I: Idx, T> IndexMut<I> for IndexVec<I, T> {
    #[inline]
    fn index_mut(&mut self, i: I) -> &mut T {
        &mut self.raw[i.index()]
    }
}

impl<I: Idx, T> FromIterator<T> for IndexVec<I, T> {
    fn from_iter<It: IntoIterator<Item = T>>(iter: It) -> Self {
        Self::from_raw(iter.into_iter().collect())
    }
}

impl<I: Idx, T> Extend<T> for IndexVec<I, T> {
    fn extend<It: IntoIterator<Item = T>>(&mut self, iter: It) {
        self.raw.extend(iter);
    }
}

impl<'a, I: Idx, T> IntoIterator for &'a IndexVec<I, T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.raw.iter()
    }
}

impl<'a, I: Idx, T> IntoIterator for &'a mut IndexVec<I, T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.raw.iter_mut()
    }
}

impl<I: Idx, T> IntoIterator for IndexVec<I, T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.raw.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    crate::define_index! {
        /// テスト用の ID。
        struct TestId;
    }

    #[test]
    fn push_returns_sequential_indices() {
        let mut v: IndexVec<TestId, u8> = IndexVec::new();
        assert_eq!(v.next_index(), TestId::from_u32(0));
        let ids: Vec<TestId> = (0..5u8).map(|k| v.push(k * 10)).collect();
        for (k, id) in ids.iter().enumerate() {
            assert_eq!(id.index(), k);
            assert_eq!(v[*id], k as u8 * 10);
        }
        assert_eq!(v.len(), 5);
        assert_eq!(v.get(TestId::new(5)), None);
        let idx: Vec<_> = v.indices().collect();
        assert_eq!(idx, ids);
    }

    #[test]
    fn index_mut_and_enumerated() {
        let mut v: IndexVec<TestId, String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
        v[TestId::new(1)].push('!');
        *v.get_mut(TestId::new(0)).unwrap() = "z".into();
        let got: Vec<(TestId, &String)> = v.iter_enumerated().collect();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0], (TestId::new(0), &"z".to_string()));
        assert_eq!(got[1], (TestId::new(1), &"b!".to_string()));
        for (id, s) in v.iter_enumerated_mut() {
            s.push_str(&id.index().to_string());
        }
        assert_eq!(v.as_slice(), &["z0".to_string(), "b!1".to_string()]);
    }

    #[test]
    fn index_ordering_follows_number() {
        assert!(TestId::new(1) < TestId::new(2));
        assert_eq!(format!("{:?}", TestId::new(3)), "TestId(3)");
    }

    #[test]
    #[should_panic(expected = "does not fit in u32")]
    #[cfg(target_pointer_width = "64")]
    fn new_overflow_panics() {
        let _ = TestId::new(u32::MAX as usize + 1);
    }

    #[test]
    #[should_panic]
    fn out_of_range_index_panics() {
        let v: IndexVec<TestId, u8> = IndexVec::new();
        let _ = v[TestId::new(0)];
    }
}
