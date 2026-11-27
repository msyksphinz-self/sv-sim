//! シフト・幅の変更・2 値化・連結・複製・選択・`$clog2` (plan.md §3.5(d)(e))。
//!
//! どれも `Vec` の要素の並べ替えとして素朴に書く。データを移すだけの演算なので、
//! X/Z はそのまま運ぶ。

use num_bigint::{BigUint, Sign};
use num_traits::{One, ToPrimitive, Zero};

use crate::value::{RefLogic, RefVec};

/// シフト量を読む。X/Z があれば `None`。シフト量は常に符号なし (§11.4.10)。
/// 値が `usize` に収まらなければ `usize::MAX` に飽和させる (どのみち幅以上)。
fn shift_amount(amt: &RefVec) -> Option<usize> {
    let a = amt.to_biguint()?;
    Some(a.to_usize().unwrap_or(usize::MAX))
}

/// `x << amt` / `x <<< amt` (§11.4.10)。空いたビットは 0。シフト量に X/Z があれば全 X。
pub fn shl(x: &RefVec, amt: &RefVec) -> RefVec {
    let w = x.width();
    let Some(n) = shift_amount(amt) else {
        return RefVec::all_x(w);
    };
    // 結果のビット i は x のビット i - n (i < n なら 0)。
    RefVec(
        (0..w)
            .map(|i| if i >= n { x.bit(i - n) } else { RefLogic::Zero })
            .collect(),
    )
}

fn shr_fill(x: &RefVec, amt: &RefVec, fill: RefLogic) -> RefVec {
    let w = x.width();
    let Some(n) = shift_amount(amt) else {
        return RefVec::all_x(w);
    };
    // 結果のビット i は x のビット i + n (範囲外なら fill)。
    RefVec(
        (0..w)
            .map(|i| match i.checked_add(n) {
                Some(j) if j < w => x.bit(j),
                _ => fill,
            })
            .collect(),
    )
}

/// `x >> amt` (§11.4.10)。空いたビットは 0。
pub fn shr(x: &RefVec, amt: &RefVec) -> RefVec {
    shr_fill(x, amt, RefLogic::Zero)
}

/// `x >>> amt` (§11.4.10)。結果が符号付き (`signed`) なら符号ビットの値 (X/Z もそのまま) で埋める。
/// 符号なしなら `>>` と同じ。符号ビットが z なら z で埋める [要VCS確認 V-3]。
pub fn ashr(x: &RefVec, amt: &RefVec, signed: bool) -> RefVec {
    let fill = if signed {
        x.msb().unwrap_or(RefLogic::Zero)
    } else {
        RefLogic::Zero
    };
    shr_fill(x, amt, fill)
}

/// 幅 `width` への拡張・切り詰め (§11.6.1, §11.8.2, §6.24.1)。
/// 切り詰めは下位を残す。拡張は符号なしなら 0、符号付きなら符号ビットの値 (x/z を含む) で埋める。
pub fn resize(width: usize, x: &RefVec, signed: bool) -> RefVec {
    let fill = if signed {
        x.msb().unwrap_or(RefLogic::Zero)
    } else {
        RefLogic::Zero
    };
    RefVec(
        (0..width)
            .map(|i| if i < x.width() { x.bit(i) } else { fill })
            .collect(),
    )
}

/// 4 値 → 2 値 (§6.11.2)。X/Z は 0 になる。結果は 0/1 だけの `RefVec`。
pub fn to_two_state(x: &RefVec) -> RefVec {
    RefVec(
        x.0.iter()
            .map(|b| {
                if *b == RefLogic::One {
                    RefLogic::One
                } else {
                    RefLogic::Zero
                }
            })
            .collect(),
    )
}

/// 連結 `{parts[0], parts[1], ...}` (§11.4.12)。先頭の要素が MSB 側。幅 0 の要素は無視される。
pub fn concat(parts: &[RefVec]) -> RefVec {
    let mut bits = Vec::new();
    // LSB 側 (末尾の要素) から積む。
    for p in parts.iter().rev() {
        bits.extend_from_slice(&p.0);
    }
    RefVec(bits)
}

/// 複製 `{n{x}}` (§11.4.12.1)。n = 0 は幅 0 の値。
pub fn replicate(x: &RefVec, n: u32) -> RefVec {
    let parts: Vec<RefVec> = (0..n).map(|_| x.clone()).collect();
    concat(&parts)
}

/// 選択のインデックス (plan.md §3.5(e))。`Value` は LSB を 0 とする物理オフセット。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RefIndex {
    /// インデックスに X/Z がある。
    Unknown,
    /// 物理オフセット。
    Value(i64),
}

/// インデックスの値を解釈する (§11.5.1)。X/Z があれば `Unknown`。
/// `i64` に収まらない値は飽和させる。
pub fn index_value(idx: &RefVec, signed: bool) -> RefIndex {
    match idx.to_bigint(signed) {
        None => RefIndex::Unknown,
        Some(v) => RefIndex::Value(v.to_i64().unwrap_or(if v.sign() == Sign::Minus {
            i64::MIN
        } else {
            i64::MAX
        })),
    }
}

/// 位置 `offset + i` が `[0, w)` の中ならその添字。
fn in_range(offset: i64, i: usize, w: usize) -> Option<usize> {
    let p = offset as i128 + i as i128;
    if p >= 0 && p < w as i128 {
        Some(p as usize)
    } else {
        None
    }
}

/// ビット選択・部分選択の読み (§11.5.1)。結果の幅は `width`。
/// 範囲外のビットは x。`Unknown` なら全 x。2 値の場合 (範囲外は 0) は `to_two_state` で確かめる。
pub fn get_slice(width: usize, src: &RefVec, idx: RefIndex) -> RefVec {
    match idx {
        RefIndex::Unknown => RefVec::all_x(width),
        RefIndex::Value(off) => RefVec(
            (0..width)
                .map(|i| match in_range(off, i, src.width()) {
                    Some(p) => src.bit(p),
                    None => RefLogic::X,
                })
                .collect(),
        ),
    }
}

/// ビット選択・部分選択への書き込み (§11.5.1)。範囲内のビットだけを書き、範囲外は無視する。
/// `Unknown` なら何も書かない。新しい値を返す。
pub fn set_slice(dst: &RefVec, idx: RefIndex, src: &RefVec) -> RefVec {
    let mut out = dst.clone();
    if let RefIndex::Value(off) = idx {
        for i in 0..src.width() {
            if let Some(p) = in_range(off, i, dst.width()) {
                out.0[p] = src.bit(i);
            }
        }
    }
    out
}

/// `$clog2(x)` (§20.8.1)。x は符号なし。`ceil(log2(x))`、`$clog2(0)` = 0、`$clog2(1)` = 0。
/// X/Z を含めば `None` [要VCS確認 V-10]。
pub fn clog2(x: &RefVec) -> Option<u32> {
    let v = x.to_biguint()?;
    if v.is_zero() {
        return Some(0);
    }
    // x >= 1 のとき ceil(log2 x) は (x - 1) のビット長。
    let m: BigUint = v - BigUint::one();
    Some(m.bits() as u32)
}
