//! 配置の契約の定数と補助関数: [`words_for`]、[`top_mask`]、[`MAX_WIDTH`]、正規化 (plan.md §3.2)。

/// 値の最大幅 (ビット)。暫定で 2^24 (plan.md §3.2, 未決 U-6)。
///
/// 規格は、ベクタ長の上限を設けるなら少なくとも 2^16 ビットにすることを求めている (§6.9.1)。
pub const MAX_WIDTH: u32 = 1 << 24;

/// 幅 `width` の 1 プレーンのワード数 `ceil(width / 64)`。
#[inline]
pub const fn words_for(width: u32) -> usize {
    (width as usize).div_ceil(64)
}

/// 幅 `width` の 4 値のプレーン分離配置 `[a.., b..]` のワード数 (`2 * words_for(width)`)。
#[inline]
pub const fn planar_len(width: u32) -> usize {
    2 * words_for(width)
}

/// 最上位ワードの有効ビットのマスク。`width % 64 == 0` なら `!0`。
///
/// 幅 0 のときもワードが無いので値に意味は無いが、`!0` を返す。
#[inline]
pub const fn top_mask(width: u32) -> u64 {
    let r = width % 64;
    if r == 0 { !0 } else { (1u64 << r) - 1 }
}

/// プレーンの `i` 番目のワードの有効ビットのマスク (最上位ワードなら [`top_mask`]、それ以外は `!0`)。
#[inline]
pub const fn word_mask(width: u32, i: usize) -> u64 {
    if i + 1 == words_for(width) {
        top_mask(width)
    } else {
        !0
    }
}

/// 1 プレーンを正規化する (最上位ワードの `width` ビット目以上を 0 にする)。
///
/// `words.len()` は `words_for(width)` であること。
#[inline]
pub fn normalize_plane(width: u32, words: &mut [u64]) {
    debug_assert_eq!(words.len(), words_for(width));
    if let Some(top) = words.last_mut() {
        *top &= top_mask(width);
    }
}

/// 1 プレーンが正規化不変条件を満たすか (長さも検査する)。
#[inline]
pub fn is_plane_normalized(width: u32, words: &[u64]) -> bool {
    if words.len() != words_for(width) {
        return false;
    }
    match words.last() {
        Some(top) => top & !top_mask(width) == 0,
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_for_boundaries() {
        assert_eq!(words_for(0), 0);
        assert_eq!(words_for(1), 1);
        assert_eq!(words_for(63), 1);
        assert_eq!(words_for(64), 1);
        assert_eq!(words_for(65), 2);
        assert_eq!(words_for(128), 2);
        assert_eq!(words_for(129), 3);
        assert_eq!(words_for(MAX_WIDTH), (MAX_WIDTH / 64) as usize);
        assert_eq!(words_for(u32::MAX), (u32::MAX as usize).div_ceil(64));
        assert_eq!(planar_len(65), 4);
    }

    #[test]
    fn top_mask_boundaries() {
        assert_eq!(top_mask(1), 1);
        assert_eq!(top_mask(3), 0b111);
        assert_eq!(top_mask(63), u64::MAX >> 1);
        assert_eq!(top_mask(64), u64::MAX);
        assert_eq!(top_mask(65), 1);
        assert_eq!(top_mask(128), u64::MAX);
        assert_eq!(top_mask(0), u64::MAX);
    }

    #[test]
    fn word_mask_values() {
        assert_eq!(word_mask(65, 0), !0);
        assert_eq!(word_mask(65, 1), 1);
        assert_eq!(word_mask(64, 0), !0);
        assert_eq!(word_mask(3, 0), 0b111);
    }

    #[test]
    fn normalize_and_check() {
        let mut w = [u64::MAX, u64::MAX];
        assert!(!is_plane_normalized(65, &w));
        normalize_plane(65, &mut w);
        assert_eq!(w, [u64::MAX, 1]);
        assert!(is_plane_normalized(65, &w));
        assert!(is_plane_normalized(128, &[u64::MAX, u64::MAX]));
        assert!(is_plane_normalized(0, &[]));
        // 長さが違えば不変条件違反。
        assert!(!is_plane_normalized(64, &[0, 0]));
        let mut empty: [u64; 0] = [];
        normalize_plane(0, &mut empty);
    }
}
