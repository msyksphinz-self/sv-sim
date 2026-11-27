# M0「基盤」テスト報告 (M0-TR)

- 実施: テスト実行エージェント (test-runner)、2026-11-27
- 入力: `docs/milestones/M0/test-plan.md` v2 (M0-TC1〜TC82)、`docs/milestones/M0/plan.md` v2.1 §9 (受け入れ基準 26 項目)
- 対象コード: コミット `7456487` (テスト中にマネージャが `bd9008a` をコミットしたが、差分は `docs/STATUS.md` だけで、コードは同一。`git diff --stat 7456487 bd9008a` で確認)。作業ツリーは実行前後とも `git status --short` が空。
- 環境: Linux 6.18、4 コア、rustc 1.97.0 (2d8144b78 2026-07-07)、cargo 1.97.0、git 2.x、cargo-deny 0.22.1 (スクラッチにビルド済みのもの)、jq あり、python3 あり。`gh` は使えない。
- ビルドディレクトリ: リポジトリの `target/` とレビュー担当の作業を邪魔しないよう、主に `CARGO_TARGET_DIR=<scratch>/tr/target` (と CLI 用の `<scratch>/tr/target2`) を使った。
- コードとテストは一切変更していない。git 操作はしていない。

> **補足: テスト実行エージェントによる独立検算 (スクラッチ、リポジトリ外)**
> test-plan の期待値表のうち、リポジトリのテストに入っていない、またはテストの置き場所が test-plan と違う項目を確かめるため、
> スクラッチに小さな crate (`<scratch>/tr/indep`、`svsim-logic` / `svsim-common` / `svsim-logic-ref` に path 依存) を作り、
> test-plan の表の値を**そのまま写して**本体の公開 API と比べた。リポジトリのコードとテストは変更していない。
> - `indep` (main): TC13〜TC21, TC24〜TC27, TC29, TC31〜TC33, TC36〜TC43, TC45〜TC47, TC50 の表の値 → **831 件すべて一致**
> - `tc9_10`: TC9, TC10 の表 → **31 件すべて一致**
> - `common`: TC61, TC62, TC63 (a.sv の JSON)、TC64 (追加ゴールデン 3 件)、TC65 の固定例 → **42 件すべて一致**
> - `tc52`: TC52 の幅 × 模様で T4/T5 の演算を参照モデルと比較 → **63,689 件すべて一致**
> - `tc53`: TC53 の全 5 操作 (幅 2^20) → すべて期待どおり
> - `tc57`: release ビルドで幅違いの `add`/`eq`/`cond_merge`/`div` が panic し、メッセージに関数名と幅が出ること → 期待どおり
>
> `<scratch>` = `/tmp/claude-0/-home-user-sv-sim/00310433-a401-5ce1-bb7a-1b482c9ffcd2/scratchpad`

---

## 1. 結果の要約

| 区分 | 件数 | 項目 |
|---|---|---|
| 合格 | 78 | 下記以外の全部 |
| **不合格** | **1** | **TC55** (`PROPTEST_CASES=100000` で `diff_real_fmt::trunc_round_trip` が "Too many global rejects" で失敗) |
| 未実施 | 2 | TC6 の手順 1 (CI の結果)、TC82 (CI の `corpus-fetch` ジョブ)。`gh` が使えないため。**マネージャが GitHub MCP で確認する** |
| 一部未実施 | 1 | TC60 の手順 1 (担当が別個体であることのセッション記録の確認はマネージャ)。STATUS の記載は確認済み |

テスト実行の件数: `cargo test --workspace --locked` (debug) で **583 件合格 / 0 失敗 / 4 ignored**、`cargo test --workspace --release --locked` で **582 件合格 / 0 失敗 / 4 ignored** (差の 1 件は `#[cfg(debug_assertions)]` 付きの `scalar::tests::bit_rejects_index_64_in_debug`)。

### 受け入れ基準 (plan.md §9) ごとの合否

| 基準 | 内容 (要約) | 判定 | 根拠の TC |
|---|---|---|---|
| A-1 | build/test/fmt/clippy/doc/deny が 1.97.0 で通る | **合格** | TC1〜TC5 |
| A-2 | CI の push/PR ジョブ全部が緑、action は SHA 固定 | **未実施 (CI の結果)** / 静的部分は合格 | TC6 (手順 2・3 は合格、手順 1 は未実施) |
| A-3 | unsafe の方針 | **合格** | TC7, TC8 |
| L-1 | 全演算が 3 層で参照モデルと一致、`_assign` も | **合格** | TC9〜TC51 |
| L-2 | `PROPTEST_CASES=100000` で通る | **不合格** | TC55 |
| L-3 | 網羅テストとゴールデンテスト | **合格** | TC13〜TC50 の G/E |
| L-4 | 正規化不変条件 | **合格** | TC12 |
| L-5 | 幅の境界と幅 > 1000、幅 0 の連結・複製 | **合格** (注 1) | TC52, TC32, TC53 |
| L-6 | ヒープ確保 0 回 | **合格** | TC54 |
| L-7 | `abi.rs` の往復・配置・マスク | **合格** | TC50, TC51 |
| L-8 | 公開 API の doc、crate ドキュメント | **合格** | TC3, TC59 |
| L-9 | 参照モデルの独立性、参照モデルの golden | **合格** (担当の確認は STATUS の記載による) | TC60 |
| C-1 | ソースマップの行・列 | **合格** | TC61, TC62 |
| C-2 | 診断 JSON / 人間向け表示の golden | **合格** (注 2) | TC63, TC64 |
| C-3 | `into_sorted` が発行順に依存しない | **合格** | TC65 |
| C-4 | インターン | **合格** | TC66 |
| C-5 | `WordArena` | **合格** | TC67〜TC71 |
| K-1 | sv-tests の固定コミット取得、rev 検証、2 回目はネットワーク不要 | **合格** | TC73, TC74, TC78 |
| K-2 | discover が 1028 件、解析エラー 0、should_fail 79、`:type:` 分布 | **合格** | TC76, TC79 |
| K-3 | run が JSONL と summary.json を出し全件 `Preprocess: NotImplemented`、2 回で同一 | **合格** | TC77, TC80 |
| K-4 | マニフェストの誤りがエラー | **合格** | TC72 |
| K-5 | 古いロックの自動除去、生きているロックでエラー | **合格** | TC75 |
| K-6 | CI の `corpus-fetch` ジョブが緑 | **未実施** | TC82 |

**26 項目中: 合格 23、不合格 1 (L-2)、未実施 2 (A-2 の CI 結果、K-6)。**

- 注 1 (L-5 / TC52): リポジトリ内の TC52 の固定幅テスト (幅 1〜4096 × 値の模様) は `diff_struct.rs` (T6) と `diff_real_fmt.rs` (T8) にだけあり、T4 (ビット演算・比較・リダクション・`?:`) と T5 (算術) には無い (test-plan は「各 `diff_*.rs` に置く」)。L-5 の文言 (境界幅と幅 > 1000 を含むテストがある) は満たしている。T4/T5 については本エージェントがスクラッチで TC52 の手順どおりに参照モデルと比較し、63,689 件すべて一致した。
- 注 2 (C-2 / TC64): test-plan が「`diag_render.rs` に追加する」とした追加ゴールデン 3 件 (a.sv、複数行、マルチバイト) と TC63 の a.sv の JSON 例はリポジトリに無い。本エージェントがスクラッチで同じ値を確かめ、すべて一致した。既存のゴールデン (`json_full` ほか 8 件) は合格。

---

## 2. 項目ごとの結果

### 2.1 共通 (A 系)

| TC | 判定 | 実行したコマンド / 手順 | 出力の要点 |
|---|---|---|---|
| TC1 | 合格 | `cargo build --workspace --locked` → `cargo test --workspace --locked` (新しい target ディレクトリ) | build 終了コード 0 (6.2 s)。test 終了コード 0 (23.8 s)、583 passed / 0 failed / 4 ignored。`FAILED` なし。ignored は `tc53_hex_2_pow_20`・`smoke_width_2_20` (TC53)、`fetch_real_sv_tests` (ネットワーク)、`parent_env_child` (子プロセス用の補助)。ネットワーク用の `fetch_real_sv_tests` も `SVSIM_CORPUS_CACHE=<取得済みキャッシュ> cargo test -p svsim-corpus --test fetch -- --ignored fetch_real_sv_tests` で実行し合格 |
| TC1 (release) | 合格 | `cargo test --workspace --release --locked` | 終了コード 0 (39.9 s)、582 passed / 0 failed / 4 ignored |
| TC2 | 合格 | `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings` | どちらも終了コード 0。fmt の出力は空 |
| TC3 | 合格 | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked` | 終了コード 0、4 crate のドキュメントを生成 |
| TC4 | 合格 | `cargo-deny check licenses bans sources`、`cargo-deny --locked check licenses bans sources` (CI と同じ)、`cargo-deny list -l license` | `bans ok, licenses ok, sources ok`、終了コード 0。`deny.toml` の許可は `MIT` `Apache-2.0` `Unicode-3.0` の 3 つだけ。`Unlicense` は単独で入っていない (memchr / same-file / walkdir / winapi-util は "Unlicense OR MIT" で MIT により充足)。`BSD-3-Clause` `Zlib` は無い。備考: `cargo-deny list` は 46 crate (dev 依存を含まない表示)、`check` は 72 crate を集めて検査している (`-L info` で確認)。検査対象としては dev 依存も含まれている |
| TC5 | 合格 | `cat rust-toolchain.toml`、`rustc --version`、ルート `Cargo.toml` | `channel = "1.97.0"`、components `rustfmt` `clippy`。`rustc 1.97.0`。`rust-version = "1.97"`、`edition = "2024"`、`resolver = "3"` |
| TC6 | **一部未実施** | 手順 2: `grep -nE '^\s*-?\s*uses:' .github/workflows/*.yml` と正規表現 `@[0-9a-f]{40}\s+#\s*v?\S+` に合わない行の抽出。手順 3: `ci.yml` の目視 | 手順 2 合格: `uses:` 14 行すべて 40 桁 SHA + `# vX.Y.Z` (actions/checkout v7.0.1、Swatinem/rust-cache v2.9.3、taiki-e/install-action v2.90.11、actions/cache v6.1.0)。合わない行は 0。手順 3 合格: `on:` に `schedule` (cron) と `workflow_dispatch`、`proptest-long` ジョブは `if: schedule || workflow_dispatch` で `PROPTEST_CASES: "100000"` の `cargo test -p svsim-logic --release --locked` と `-- --ignored` を実行。ジョブは `fmt clippy test doc deny proptest-long corpus-fetch`。**手順 1 (最新コミットの CI が success か) は未実施: `gh` が使えない。マネージャが GitHub MCP で確認する**。なお TC55 の失敗により、`proptest-long` を起動すると同じ理由で失敗するはず |
| TC7 | 合格 | grep によるスクリプト (下記) | 1. `unsafe_code = "deny"`、`missing_docs = "warn"`、`undocumented_unsafe_blocks = "deny"`。2. 4 crate すべて `[lints] workspace = true`。3. `lib.rs` 4 個と `svsim-corpus/src/main.rs` の先頭属性に `#![forbid(unsafe_code)]` (前に `//!` 以外の行なし)。4. `grep -rnw unsafe crates --include='*.rs'` の一致は `crates/svsim-logic/tests/no_alloc_*.rs` 6 ファイルだけ。各ファイルの先頭に `#![allow(unsafe_code)]`、`unsafe {` / `unsafe impl` 計 30 箇所すべての直前 3 行以内に `// SAFETY:` |
| TC8 | 合格 | 作業ツリーをスクラッチにコピーし (`.git` と `target` を除く)、`crates/svsim-logic/src/layout.rs` に `pub fn f() { unsafe {} }` を追記して `cargo build -p svsim-logic --locked` | `error: usage of an unsafe block` … `note: the lint level is defined here --> crates/svsim-logic/src/lib.rs:110:11 #![forbid(unsafe_code)]`。E0453 ではなく unsafe_code の lint エラー。終了コード 101。元のツリーは変更なし |

### 2.2 `svsim-logic` (L 系)

実行コマンドは TC1 の `cargo test --workspace --locked` (debug) と `--release` に含まれる。各テストファイルの結果 (debug):
`svsim_logic` 単体 111、`diff_arith` 10、`diff_bitwise` 19、`diff_compare` 16、`diff_core` 39、`diff_literal_string` 10、`diff_real_fmt` 10、`diff_struct` 13、`golden_arith` 12、`golden_bitwise` 20、`golden_fmt` 6 (+1 ignored)、`golden_literal` 23、`golden_real` 4、`golden_struct` 19 (+1 ignored)、`no_alloc_*` 計 24 — すべて合格。

| TC | 判定 | 対応するテスト (リポジトリ) | 補足 |
|---|---|---|---|
| TC9 | 合格 | `layout.rs`・`logic.rs` の単体テスト | 独立検算 (`tc9_10`) で `Logic as u8`、`Display`、`words_for` (2^24 → 262144)、`top_mask`、`MAX_WIDTH = 1<<24` を確認 |
| TC10 | 合格 | `diff_core`: `construction_golden`, `bits_string_round_trip`, `bits_string_rejects_invalid`, `ref_round_trip`, `to_u64_i64_match_ref` ほか | 独立検算で `from_i64(63,-1)` の正規化、`"10XZ01"`、空文字列 = 幅 0、`_`/`2`/空白/`?` のエラー、`to_u64`/`to_i64`、`Debug` = `8'b10xz_0001`、幅 64/65 で別経路の値の `==` と `Hash` 一致、`zero(MAX_WIDTH)` を確認 |
| TC11 | 合格 | `diff_core`: `planar_and_split_views_agree`, `from_planar_rejects_len_3/5`, `bit_rejects_out_of_range`, `set_bit_rejects_out_of_range`, `copy_from_rejects_width_mismatch`, `bit_set_get_at_word_boundaries`, `fill_matches_ref` | `should_panic` は release でも合格 |
| TC12 | 合格 | `diff_core`: `normalization_checker_detects_violations`, `normalization_of_risky_constructions`、`golden_bitwise`: `tc12_normalized_outputs_bitwise` | 手順 3 (コード確認): `tests/common/mod.rs` の比較関数 (`check_eq_ref` 系) が毎回 `check_normalized` / `check_normalized2` / `check_normalized_l64` を呼ぶ。`diff_*.rs` 全 7 ファイルがこれを使う (呼び出し数 4〜53) |
| TC13 | 合格 | `golden_bitwise`: `tc13_tables_11_4_8`, `tc13_vectors_11_4_8`、`bitwise.rs` 単体 `tables_11_4_8` | 独立検算: 4×4 の 64 組 + 単項 + ベクタ例 + 2 値例すべて一致 |
| TC14 | 合格 | `tc14_reductions_11_4_9`, `tc14_countbits_20_9` | 独立検算: 表の 15 行 (129 ビットの行を含む)、`count_bits` 7 通り、`onehot`/`onehot0`/`isunknown` 一致 |
| TC15 | 合格 | `tc15_logical_11_4_7`, `tc15_truth_12_4` | 独立検算: `truth` 6 例、`!4'b000x = x` 一致 |
| TC16 | 合格 | `tc16_equality_11_4_5` | 独立検算: 7 行 + 129 ビットの 2 行一致 ([V-15] 暫定値どおり) |
| TC17 | 合格 | `tc17_wildcard_11_4_6` | 独立検算: 8 行 × `==?`/`!=?` 一致 ([V-16]) |
| TC18 | 合格 | `tc18_casez_casex_12_5_1` | 独立検算: 8 行一致 ([V-17]) |
| TC19 | 合格 | `tc19_relational_11_4_4` | 独立検算: 9 行 (65/128 ビットを含む) 一致 |
| TC20 | 合格 | `tc20_cond_merge_11_4_11`, `tc20_cond_merge_bits_3_5_1` | 独立検算: `z?z → x` [V-8] を含め一致、`cond_merge_bits` = `8'b1xx0_1010` |
| TC21 | 合格 | `tc21_edge_table_9_4_2_all_16`, `tc21_edge2_9_4_2`, `tc21_edge_lsb_9_4_2` | 独立検算: 16 通りと `edge_lsb` 4 例一致 ([V-12]) |
| TC22 | 合格 | `diff_bitwise`: `tc22_exhaustive_*` 3 件、`diff_compare`: `tc22_exhaustive_compare_*` 2 件 | |
| TC23 | 合格 | `diff_bitwise`: `tc23_*` 14 件、`diff_compare`: `tc23_*` 14 件 | 層 1 (`*_scalar`)、2 値 (`*_two_state`)、混在 (`cond_merge_bits`) を含む |
| TC24 | 合格 | `golden_arith`: `add_sub_11_4_3`, `add_unknown_11_4_3`, `neg_inc_dec_11_4_2` | 独立検算: 表の全行一致 |
| TC25 | 合格 | `mul_11_4_3` | 独立検算: 7 行一致 |
| TC26 | 合格 | `div_rem_truncation_11_4_3`, `div_by_zero_11_4_3_v2`, `div_min_by_minus_one_v1`, `div_multiword_11_4_3` | 独立検算: 表の全行 (Knuth D の add back、129 ビット 2^128/3、[V-1] を含む) 一致 |
| TC27 | 合格 | `pow_unsigned_11_4_3`, `pow_special_cases_11_4_3`, `pow_unknown_11_4_3`, `pow_bits_mixed_3_5_1` | 独立検算: 24 行 (128 ビットの 3^100 を含む) 一致 |
| TC28 | 合格 | `diff_arith` 10 件 (`exhaustive_*` 3、`diff_*` 6、`prop_div_rem_identity`) | |
| TC29 | 合格 | `golden_struct`: `shift_11_4_10_*` 6 件 | 独立検算: 表の全行と `_const` 版 (n = 0, 7, 8, u64::MAX) 一致 ([V-3]) |
| TC30 | 合格 | `diff_struct`: `exhaustive_shift_w1_4`, `diff_shift`, `prop_shift_compose` | |
| TC31 | 合格 | `resize_11_6_1_truncation`, `resize_11_8_2_*`, `two_state_6_11_2` | 独立検算一致 |
| TC32 | 合格 | `concat_11_4_12`, `replicate_11_4_12_1` | 独立検算: 幅 0 の連結・複製を含め一致 |
| TC33 | 合格 | `select_11_5_1_*` 4 件、`index_value_11_5_1` | 独立検算: `i64::MAX/MIN`、`Unknown`、2 値の例外規則、飽和を含め一致 |
| TC34 | 合格 | `mixed_shift_3_5_1_v9`, `pow_bits_mixed_3_5_1`, `tc20_cond_merge_bits_3_5_1`、差分 `tc23_cond_merge_bits*`、`diff_shift`/`diff_pow` 内の混在 | |
| TC35 | 合格 | `diff_struct`: `diff_resize`, `diff_two_state`, `diff_concat`, `diff_replicate`, `diff_select`, `diff_index_value`, `exhaustive_struct_w1_3`, `prop_resize_round_trip` | |
| TC36 | 合格 | `clog2_20_8_1`, `diff_clog2` | 独立検算: k = 2〜62 の全境界、2^20 ビットを含め一致 ([V-10]) |
| TC37 | 合格 | `golden_literal`: `s5_7_1_*` | 独立検算: 表の全行 (truncated フラグ [V-18] を含む) 一致 |
| TC38 | 合格 | `s5_7_1_decimal_*`, `s5_7_1_unsized_decimal`, `s5_7_1_unsized_wider_than_32_v11` | 独立検算一致 |
| TC39 | 合格 | `diff_literal_string`: `diff_parse_based`, `diff_parse_unsized_decimal`, `parse_based_input_coverage`, `prop_format_parse_round_trip` | |
| TC40 | 合格 | `s11_10_3_*`, `s6_16_integer_to_string*`、差分 `diff_from_string_bytes`, `diff_to_string_bytes*`, `prop_string_round_trip` | 独立検算: 13 行一致 ([V-5]) |
| TC41 | 合格 | `s6_16_9_*` 5 件、`diff_atoi`, `prop_atoi_round_trip` | 独立検算: 24 行一致 ([V-6]) |
| TC42 | 合格 | `golden_real`: `tc42_int_to_real_6_12_1` | 独立検算: 19 行 (f64::MAX/∞ の境界、f32 の二重丸めを含む) 一致 |
| TC43 | 合格 | `tc43_real_to_int_6_12_1`, `tc43_bits_20_5`, `tc43_overwrites_dst` | 独立検算: 状態 4 種を含め一致 ([V-4]) |
| TC44 | 合格 (既定件数) | `diff_real_fmt`: `to_real_*` 3 件, `from_real_matches_ref`, `from_real_width0_status_matches_ref`, `trunc_round_trip`, `small_int_exact` | 256 件では合格。**100000 件では `trunc_round_trip` が失敗 (TC55 参照)** |
| TC45 | 合格 | `golden_fmt`: `tc45_bin_oct_hex_21_2_1`, `tc45_auto_width_table` | 独立検算: 21 行 + 自動幅表 11 行一致 ([V-13]) |
| TC46 | 合格 | `tc46_decimal_auto_21_2_1`, `tc46_decimal_auto_width_table` | 独立検算: 20 行 + 文字数表 15 行一致 |
| TC47 | 合格 | `tc47_field_width_v7` | 独立検算: 25 行一致 ([V-7]) |
| TC48 | 合格 | `tc48_itoa_6_16_11` | |
| TC49 | 合格 | `diff_real_fmt`: `format_matches_ref`, `format_and_real_boundary_widths` | |
| TC50 | 合格 | `diff_core`: `sv_logic_vec_val_golden`, `sv_vec_val_read_mask_and_lengths_golden` | 独立検算: 書き 8 例、読み (マスク) 3 例、2 値の読み書き一致 |
| TC51 | 合格 | `sv_round_trip`, `sv_logic_read_masks`, `sv_bit_read_masks`, `sv_bit_zero_copy_equivalence`, `sv_logic_layout_matches_ref` | |
| TC52 | 合格 (注 1) | `diff_struct::tc52_width_boundaries` (T6 の全演算)、`diff_real_fmt::format_and_real_boundary_widths` (T8) | 幅 63, 64, 65, 127, 128, 129, 1025, 4096 が `TC52_WIDTHS` (`diff_struct.rs:798`, `diff_real_fmt.rs:535`) に現れることを確認。**T4/T5 の演算 (ビット演算、比較、リダクション、`?:`、算術) の固定幅テストはリポジトリに無い**。本エージェントが `tc52` (スクラッチ) で TC52 の手順 (16 幅 × 11 模様の全組、`_assign` 版、正規化) を実施し 63,689 件すべて一致 |
| TC53 | 合格 | `cargo test -p svsim-logic --release --locked -- --ignored` | `smoke_width_2_20` と `tc53_hex_2_pow_20` が合格 (各 0.00 s 表示、テストバイナリ全体で 0.35 s)。**リポジトリの ignored テストに「全 1 + 1」の `add` が無い**ため、スクラッチ (`tc53`, release) で全 5 操作を実施: add 0.32 ms、`1 << (2^20−1)` 0.16 ms、連結 0.22 ms、`%h` (MSB) 1.1 ms、`%h` (全 x) 1.2 ms、いずれも期待どおり。参考: 2^20 ビットの `mul` 174 ms、`zero(MAX_WIDTH)` 0.03 ms |
| TC54 | 合格 | `no_alloc_core` 7、`no_alloc_bitwise` 3、`no_alloc_arith` 4、`no_alloc_struct` 3、`no_alloc_literal_string` 3、`no_alloc_real_fmt` 4 | 検査器の自己テストを含む。release でも合格 |
| TC55 | **不合格** | `PROPTEST_CASES=100000 cargo test -p svsim-logic --release --locked --no-fail-fast` | 下記 §3.1 |
| TC56 | 合格 | `diff_core::strategy_distribution` (10,000 個、`201..=1024` の比率 0.10〜0.30 を検査) | |
| TC57 | 合格 | `diff_core` の `should_panic` 13 件、`golden_bitwise::tc57_*` 4 件、release でも合格 | `add` の幅違いはリポジトリに `should_panic` が無いので、スクラッチ (`tc57`, release) で確認: `add: width mismatch (dst 8, x 8, y 9)`、`eq: width mismatch (64 vs 65)`、`cond_merge: width mismatch (8 vs 9)`、`div: width mismatch (dst 8, x 8, y 9)`。0 除算・x のシフト量・`u64::MAX` のシフト・NaN・不正な bits 文字列は panic しない |
| TC58 | 合格 | `diff_core::deterministic_across_threads` (`std::thread::scope` で分割) | |
| TC59 | 合格 | TC2・TC3 の合格 + `crates/svsim-logic/src/lib.rs` の `//!` を読んで確認 | 配置の契約 (4 値 `[a_0..a_{nw-1}, b_0..b_{nw-1}]`、2 値、LE、正規化不変条件、**適用範囲は packed 値 1 個で unpacked 配列の要素は対象外**、幅 0、`MAX_WIDTH`)、§3.6 の幅規則表の全行、代入文脈、符号の規則、評価の手順、`$signed`/`$unsigned`、混在規則 1〜5 (選択の例外を含む) がある。`svsim-common` の `lib.rs` と `intern.rs` に「`Symbol` の番号順を出力の順序に使わない」規則がある |
| TC60 | 合格 (手順 1 の一部はマネージャ) | 1. `docs/STATUS.md`: M0-T9「implementer (別個体)」、M0-T9b「本体を読まない別個体」。1b. `RefRealToIntStatus` は `Exact Rounded Wrapped NotFinite` の 4 種、`RefVec::from_bits_str` は TC10 の規則 (Q-22) どおり。2. `cargo tree -p svsim-logic-ref -e all` に `svsim-logic` が 0 件 (依存は num-bigint / num-traits のみ)。3. `cargo test -p svsim-logic-ref` 77 件合格 (golden 7 ファイル: §11.4.x, §12.5.1, §9.4.2, §5.7.1, §6.12.1, §20.x, §21.2.1)。4. plan §5.1 の関数一覧 60 個 (`reduce_{…}` と `logical_{…}` を展開) がすべて `pub fn` として存在 | セッション記録による別個体の確認はマネージャの担当 |

### 2.3 `svsim-common` (C 系)

実行: `cargo test --workspace --locked` 内。`svsim_common` 単体 38、`arena` 14、`arena_logic` 1、`diag_render` 8、`intern` 4 — すべて合格 (release も同じ)。

| TC | 判定 | 対応するテスト | 補足 |
|---|---|---|---|
| TC61 | 合格 | `source.rs`: `lf_lines`, `crlf_lines`, `lone_cr_is_not_a_line_break`, `no_trailing_newline`, `empty_file`, `utf8_columns_count_chars`, `invalid_utf8_counts_bytes` | 独立検算 (`common`): 表の 19 ケース (終端超え `u32::MAX` を含む) 一致 |
| TC62 | 合格 | `file_ids_are_sequential`, `snippet_and_span`, `line_text_out_of_range_panics` | 独立検算: `line_text` 7 ケース、範囲外 (0, 2) で panic、`snippet` 一致 |
| TC63 | 合格 | `diag_render`: `json_full`, `json_multiline_span`, `json_round_trips_through_text`、`render_json.rs` 単体 2 件 | test-plan の `a.sv` の例 (リポジトリには無い) を独立検算で確認し一致。`code: null`、空の `notes`/`fixits` も一致 |
| TC64 | 合格 (注 2) | `human_full`, `human_multiline_span_shows_start_line_only`, `human_tab_and_wide_gutter`, `human_empty_span_at_eof`, `human_utf8_and_invalid_bytes`、`render_human.rs` 単体 4 件 | test-plan が追加を求めた 3 つのゴールデン (a.sv の主 + 副 + 注記、複数行の主ラベル、マルチバイト) はリポジトリに未追加。独立検算で 3 件ともバイト単位で一致 |
| TC65 | 合格 | `diag.rs`: `into_sorted_orders_by_file_then_position`, `into_sorted_is_independent_of_emission_order`, `primary_span_choice`, `counts` | 独立検算: 固定例 `["none","a2","a9","b0"]`、ラベル無しはメッセージ順、主 span の選び方、`has_errors` 一致 |
| TC66 | 合格 | `intern`: `tc66_basic`, `tc66_special_strings`, `tc66_property_eq_iff_same_string`, `deterministic_ids_for_same_input` | |
| TC67 | 合格 | `arena::tc67_index_vec`、`index.rs` 単体 4 件 | |
| TC68 | 合格 | `tc68_alloc_alignment`, `tc68_alloc_overflow`, `tc68_alloc_zero_words`, `tc68_alloc_bad_align` | |
| TC69 | 合格 | `tc69_disjoint_ok_and_writes`, `tc69_disjoint_table`, `tc69_disjoint_unsorted_many` | |
| TC70 | 合格 | `tc70_content_hash_fixed_values`, `tc70_content_hash_matches_le_byte_definition`, `tc70_snapshot_restore`, `tc70_restore_wrong_length_panics`, `tc70_restore_wrong_length_leaves_arena_unchanged` | |
| TC71 | 合格 | `arena_logic::tc71_arena_with_logic_views` | |

### 2.4 コーパスマネージャ (K 系)

実行: `cargo test --workspace --locked` 内。`svsim_corpus` 単体 71、`main.rs` 単体 3、`cli_discover_run` 2、`cli_fetch` 7、`fetch` 16 (+2 ignored) — すべて合格 (release も同じ)。

| TC | 判定 | 対応するテスト / 手順 | 補足 |
|---|---|---|---|
| TC72 | 合格 | `manifest.rs`: `rejects_non_commit_rev`, `rejects_duplicate_name_in_one_file`, `rejects_duplicate_name_across_files`, `rejects_unknown_keys`, `rejects_unknown_discover_kind`, `rejects_missing_required_fields`, `rejects_unsupported_version`, `file_urls_require_test_flag`, `repository_manifest_is_valid` ほか、`cli_fetch::bad_manifests_are_reported`, `file_url_manifest_requires_flag` | 手動でも確認: 未知のキー `colour` を含むマニフェストで `list` → 終了コード 1、`unknown field` とキー名を表示 |
| TC73 | 合格 | `fetch.rs`: `fetches_non_tip_commit_and_is_idempotent`, `fetches_tip_commit`, `offline_without_cache_is_an_error`, `unknown_rev_fails_and_leaves_nothing_behind`、`fetch.rs` 単体 `finish_accepts_concurrently_completed_checkout`、`cli_fetch::fetch_then_offline_via_cli` | |
| TC74 | 合格 | 自動: `parent_git_config_env_is_not_inherited`, `git_command_restricts_protocols`, `cli_discover_run` (run.sh / conf/generators/x.py が実行されない)。手動 (スクラッチ): 下記 | 手順 1 と 3 は自動テストで直接は見ていないので手動で実施。1: 一時 `HOME/.gitconfig` に `core.hooksPath=<印を作る post-checkout があるディレクトリ>` を置く。同じ HOME の素の `git clone` では印が作られる (前提の確認) が、`svsim-corpus --allow-file-protocol --manifest m.toml fetch/discover/run demo` の後は印 (フック・run.sh とも) が無い。3: `git = "https://127.0.0.1:1/x.git"` のマニフェストで `timeout 30 svsim-corpus fetch auth < /dev/null` → 0.03 s で終了コード 1 (`Couldn't connect to server`)、プロンプトもハングも無し |
| TC75 | 合格 | `fetch.rs`: `stale_lock_is_removed_automatically`, `live_lock_blocks_fetch_and_force_unlock_overrides`, `unparsable_lock_blocks_fetch`, `legacy_lock_format_is_understood`, `leftovers_from_crashed_fetch_are_cleaned`、`cache.rs` 単体 18 件、`cli_fetch::locked_entry_via_cli` | |
| TC76 | 合格 | `discover.rs` 単体 19 件 (`defaults`, `crlf_values_are_trimmed`, `uppercase_key_and_should_fail_because`, `later_key_wins`, `unknown_keys_and_runner_flags`, `explicit_files_incdirs_defines_top`, `type_to_target`, `metadata_regex_edges`, `sv_extension`, `discovers_files_in_byte_order` ほか) | |
| TC77 | 合格 | `stage.rs` 単体 9 件 (`stage_order`, `empty_pipeline_is_not_implemented`, `stops_at_first_non_pass`, `stops_at_target`, `missing_runner_stops_with_not_implemented`, `verdict_table`, `judge_empty_stages_is_undetermined` ほか) | |
| TC78 | 合格 | 下記 §3.2 | 空のキャッシュへの実取得 1 回 + 取得済みキャッシュ |
| TC79 | 合格 | 下記 §3.3 | |
| TC80 | 合格 | 下記 §3.4 | |
| TC81 | 合格 | 自動: `cli_fetch` (`list_shows_default_and_extra_entries`, `unknown_entry_is_usage_error`, `missing_subcommand_is_usage_error`, `bad_manifests_are_reported`)、`cli_discover_run::usage_errors`、`main.rs` 単体 3 件。手動: 下表 | 手動の終了コード: `list` 0 (`sv-tests` を表示) / サブコマンドなし 2 / `frobnicate` 2 / `fetch --bogus` 2 / 空のキャッシュで `fetch --offline` 1 (キャッシュに何も作らない) / `discover` (NAME なし) 2 / `discover no-such-entry`・`fetch no-such-entry`・`run no-such-entry` 2 / 誤りのあるマニフェストで `list` 1 / `cargo corpus list` の出力は `svsim-corpus list` とバイト一致 |
| TC82 | **未実施** | — | `gh` が使えないため CI の結果は見ていない。**マネージャが GitHub MCP で確認する**。静的には: `corpus-fetch` ジョブは `actions/cache` のキーに `hashFiles('corpus/manifest.toml')` を含み、`cargo build -p svsim-corpus --locked`、`cargo corpus fetch sv-tests`、`cargo corpus discover sv-tests` を実行する (`ci.yml:138-160`) |

---

## 3. 詳細

### 3.1 TC55 (L-2) 不合格の詳細

**再現手順** (リポジトリの直下で):
```
PROPTEST_CASES=100000 cargo test -p svsim-logic --release --locked --no-fail-fast
```
**結果**: 終了コード 101。所要時間 46.4 s (1 回目は `--no-fail-fast` なしで 48.6 s、同じ失敗)。失敗は 1 件だけで、他のテストバイナリはすべて合格:

| バイナリ | 結果 (100000 件) | 所要時間 |
|---|---|---|
| 単体 (src) | 110 passed | 0.78 s |
| diff_arith | 10 passed | 7.23 s |
| diff_bitwise | 19 passed | 4.93 s |
| diff_compare | 16 passed | 9.96 s |
| diff_core | 39 passed | 8.79 s |
| diff_literal_string | 10 passed | 3.45 s |
| **diff_real_fmt** | **9 passed, 1 failed** | 3.51 s |
| diff_struct | 13 passed | 7.19 s |
| golden_* / no_alloc_* | すべて passed | < 0.1 s |

**失敗出力**:
```
---- trunc_round_trip stdout ----
proptest: FileFailurePersistence::SourceParallel set, but failed to find lib.rs or main.rs

thread 'trunc_round_trip' (13528) panicked at crates/svsim-logic/tests/diff_real_fmt.rs:347:1:
Test aborted: Too many global rejects
	successes: 23123
	local rejects: 0
	global rejects: 1024
		1024 times at crates/svsim-logic/tests/diff_real_fmt.rs:386:9: v.is_finite()
```

**原因の見立て**: テストの作り (T8 の担当ファイル `crates/svsim-logic/tests/diff_real_fmt.rs`) の問題で、実装の誤りではない。
- `trunc_round_trip` は戦略 `f64_value()` (NaN・±∞ を一定の割合で含む) から値を取り、`prop_assume!(v.is_finite())` で捨てている。捨てる割合はおよそ 4.4 % (1024 / (23123 + 1024))。proptest の既定の `max_global_rejects` は 1024 なので、件数がおよそ 23,000 を超えると必ず打ち切られる。既定の 256 件では起きない。
- 参考 (診断のための実行。コードは変えていない): `PROPTEST_CASES=100000 PROPTEST_MAX_GLOBAL_REJECTS=1000000 cargo test -p svsim-logic --release --locked --test diff_real_fmt -- trunc_round_trip` → **合格**。性質そのもの (`to_f64(from_f64_trunc(v), S) == v.trunc()`) は 100000 件で成り立っている。`PROPTEST_CASES=20000` でも合格。
- 直し方の候補 (implementer への差し戻し用): 有限の値だけを出す戦略を使う (例: `f64_value()` から NaN/∞ を除いた版、または `prop_filter` ではなく `prop_map` で有限値に写す)、または当該 `proptest!` に `ProptestConfig { max_global_rejects: …, .. }` を付ける。
- 影響: CI の定期ジョブ `proptest-long` (`PROPTEST_CASES=100000`) も同じ理由で失敗するはず。

**追加確認 (TC55)**: 失敗の再現ファイルの置き場所。上の警告 `SourceParallel set, but failed to find lib.rs or main.rs` は、proptest 1.12 が統合テスト (`tests/` に `lib.rs` が無い) で `WithSource("proptest-regressions")` に切り替えるときの表示 (`proptest-1.12.0/src/test_runner/failure_persistence/file.rs:349-354` で確認)。保存先はテストファイルの隣の `tests/<name>.proptest-regressions` になり、STATUS の決定と一致する。既存の `crates/svsim-logic/tests/diff_arith.proptest-regressions` と `diff_struct.proptest-regressions` は git の管理下にある (`git ls-files`)。今回の失敗は打ち切りなので再現ファイルは作られず、作業ツリーは変わっていない。

### 3.2 TC78 (K-1) 実際の sv-tests の取得

キャッシュはすべてスクラッチ内。ホームディレクトリのキャッシュ (`~/.cache/svsim`) は作っていない (実行前後とも存在しない)。

1. **空のキャッシュへの実取得 (1 回だけ)**: `SVSIM_CORPUS_CACHE=<scratch>/tr/fresh-cache cargo corpus fetch sv-tests` → 終了コード 0、`real 0m0.842s`、`sv-tests: fetched 7274242bcf99a2f6f8282493d0200c92eb87d347 -> …/fresh-cache/sv-tests/7274242b…`。
   - `git -C $D rev-parse HEAD` = `7274242bcf99a2f6f8282493d0200c92eb87d347` (一致)
   - `$D/.svsim-complete` あり (中身 `name=sv-tests` / `git=https://github.com/chipsalliance/sv-tests` / `rev=7274242b…`)
   - `<cache>/sv-tests/` には rev のディレクトリだけ (`.tmp-*` も `.lock` も無い)。サイズ 6.7 MB、`tests/` の `*.sv` 1028 個
2. **2 回目** (fresh-cache と、マネージャが用意した取得済みの `<scratch>/corpus-cache` の両方で):
   - `cargo corpus fetch sv-tests`: `sv-tests: cached …`、0.19 s / 0.09 s (cargo の起動を含む)。バイナリ直接では 0.006 s / 0.005 s。目安 2 秒以内
   - `HTTPS_PROXY=http://127.0.0.1:9 https_proxy=… HTTP_PROXY=… http_proxy=… svsim-corpus fetch sv-tests`: 終了コード 0 (`cached`)
   - `svsim-corpus fetch sv-tests --offline`: 終了コード 0 (`cached`)
3. **既定の場所**: ホームを汚さないよう、実取得はしていない。`SVSIM_CORPUS_CACHE` を外し `HOME=<scratch>/tr/fakehome` で `fetch sv-tests --offline` を実行すると、エラーメッセージの探索先が `<fakehome>/.cache/svsim/corpus/sv-tests/7274242b…` (既定の場所の規則どおり) で、終了コード 1、何も作らない。単体テスト `cache_root_prefers_env` も合格。

### 3.3 TC79 (K-2) 実際の sv-tests の列挙

コマンド: `SVSIM_CORPUS_CACHE=<scratch>/corpus-cache svsim-corpus discover sv-tests` と `… discover sv-tests --json > disc.json` (どちらも終了コード 0。fresh-cache でも JSON はバイト一致)。

表示:
```
sv-tests: 1028 cases (79 should_fail, 0 warnings, 0 errors)
  by target:  Preprocess: 90  Parse: 14  Elaborate: 577  Simulate: 347  Compare: 0
  by :type:   (none): 572  elaboration: 5  parsing: 5  preprocessing: 90  preprocessing parsing: 9
              simulation: 4  simulation elaboration: 53  simulation elaboration parsing: 289
              simulation elaboration preprocessing: 1
```
`disc.json` を Python で検査: `cases` 1028、`should_fail` 79 (`reason` あり 79)、`target` の内訳 Preprocess 90 / Parse 14 / Elaborate 577 / Simulate 347、`:type:` の内訳が test-plan の表と完全一致、`errors` `[]`、`warnings` `[]`、id は一意でバイト順に整列 (`LC_ALL=C sort -c` も成功)、`sv-tests/testbenches/*.sv` 11 件と `sv-tests/uvm/uvm_files.sv` を含む。

独立な検算 (チェックアウトで): `find tests -iname '*.sv' | wc -l` → 1028、`find tests -iname '*.sv' -print0 | xargs -0 grep -liE '^:should_fail_because:[[:space:]]*[^[:space:]]' | wc -l` → 79、`ls tests/testbenches/*.sv | wc -l` → 11。

### 3.4 TC80 (K-3) 実際の sv-tests の実行と決定性

コマンド: `svsim-corpus run sv-tests --out <scratch>/tr/r1`、同じく `r2` (取得済みキャッシュ)。
1. 終了コード 0 (2 回とも)。
2. `results.jsonl` 1028 行、全行が有効な JSON、id は一意。全行の `stages` が `[{"stage":"Preprocess","outcome":"NotImplemented"}]` だけ。
3. `summary.json`: `stages.Preprocess.NotImplemented = 1028`、他の値はすべて 0。`verdicts`: `Undetermined 1028`、`AsExpected 0`、`Unexpected 0` (should_fail の 79 件も Undetermined)。
4. 標準出力: `sv-tests: 1028 cases (79 should_fail)` / `Preprocess: NotImplemented 1028` / `verdict: AsExpected 0, Unexpected 0, Undetermined 1028` / `results: <dir>`。
5. 決定性: `jq -c 'del(.duration_us)'` 後の `results.jsonl` が r1 と r2 で同一 (行の順序を含む)。`sed` で `duration_us` を消したものも `cmp` でバイト一致。`summary.json` はバイト一致。
6. `--out` 省略: `/home/user/sv-sim/target/corpus-results/sv-tests/` に出力され、`summary.json` は r1 とバイト一致。(このディレクトリは `.gitignore` の `/target` に入る。この確認のために作成した)

---

## 4. 不合格・未実施の一覧と原因の見立て

| 項目 | 判定 | 原因の見立て / 担当 | 対応案 |
|---|---|---|---|
| **TC55 (L-2)** | 不合格 | `crates/svsim-logic/tests/diff_real_fmt.rs:386` の `prop_assume!(v.is_finite())` が戦略 `f64_value()` の NaN/±∞ を約 4.4 % 捨て、`PROPTEST_CASES=100000` では proptest の既定 `max_global_rejects = 1024` を超える。テストの作りの問題 (M0-T8 の担当ファイル)。実装 (`real.rs`) の誤りではない (上限を上げると 100000 件で合格) | 有限値だけを出す戦略にする、または `max_global_rejects` を設定する。修正後に TC55 を再実行 |
| TC6 手順 1 (A-2) | 未実施 | `gh` が使えない | マネージャが GitHub MCP で最新コミットの `fmt clippy test doc deny corpus-fetch` の結果を確認する |
| TC82 (K-6) | 未実施 | 同上 | 同上。`corpus-fetch` のログに fetch / discover と 1028 件、2 回目のキャッシュヒットを確認する |
| TC60 手順 1 の一部 (L-9) | マネージャ確認 | セッション記録はテスト実行エージェントから見えない | STATUS の記載 (T9・T9b とも別個体) は確認済み |

### 合格だが指摘しておくこと (テストの不足。実装の誤りではない)
1. **TC52**: T4/T5 (ビット演算・比較・リダクション・`?:`・算術) の固定幅 × 模様のテストがリポジトリに無い。`diff_bitwise.rs` / `diff_compare.rs` / `diff_arith.rs` に `tc52_*` を足すのが test-plan の意図に合う (担当: T4, T5)。本エージェントの補完で 63,689 件一致は確認済み。
2. **TC53**: `#[ignore]` のスモークテストに「幅 2^20 で全 1 + 1」(`add`) が無い (`golden_struct.rs::smoke_width_2_20` は shl / clog2 / concat、`golden_fmt.rs::tc53_hex_2_pow_20` は `%h`)。補完で合格は確認済み。
3. **TC63 / TC64**: test-plan の a.sv の JSON 例と、`diag_render.rs` に追加するとされた人間向け表示の追加ゴールデン 3 件が未追加。補完でバイト一致を確認済み。
4. **TC57**: `add` の幅違いの `should_panic` テストがリポジトリに無い (`eq` `cond_merge` `and2` `lt` はある)。補完で release の panic とメッセージを確認済み。
5. 参考: 層 1 の `L64::bit` の範囲外チェックは `debug_assertions` のときだけ (`scalar.rs:267` のテストも debug 限定)。TC57 の対象 (ビューと `LogicVec` の `bit`/`set_bit`) は release でも panic するので test-plan 上は問題ないが、Q-1 (「release でも panic」) の範囲に層 1 を含めるかはマネージャの判断事項。

---

## 5. 実行ログと作成物

- ログ: `<scratch>/tr/` の `tc1_build.log`、`tc1_test.log`、`rel_test.log`、`tc2_fmt.log`、`tc2_clippy.log`、`tc3_doc.log`、`tc4_deny.log`、`tc53_ignored.log`、`tc55.log`、`tc55b.log`、`tc78_fetch1.log`、`disc.json`、`run1.log`、`run2.log`、`r1/`、`r2/`
- 独立検算の crate: `<scratch>/tr/indep` (`src/main.rs`、`src/bin/{common,tc9_10,tc52,tc53,tc57}.rs`)
- リポジトリ内で作られたもの: `/home/user/sv-sim/target/corpus-results/sv-tests/` (TC80.6) と、`cargo corpus list` のエイリアス確認で `/home/user/sv-sim/target/debug/` に入ったビルド。どちらも `.gitignore` の対象

---

## 追記: 再テスト (マネージャ、M0-T15 適用後、コミット 8e6d74e)

| 項目 | 結果 |
|---|---|
| L-2 / TC55 `PROPTEST_CASES=100000 cargo test -p svsim-logic --release --locked --no-fail-fast` | **合格** (354 passed / 0 failed、"Too many" 0 件)。原因は戦略の NaN/∞ 棄却で、有限値だけの戦略に修正 |
| `cargo test --workspace` (debug) | 603 passed / 0 failed |
| clippy `-D warnings` / fmt | 合格 |
| テスト不足 (d)1〜4 | TC52 固定幅 (bitwise/compare/arith)、TC53 2^20 add、TC63/64 ゴールデン、TC57 should_panic を追加し合格 |
| (d)5 L64::bit の範囲外 | 層1 は debug_assert のみで可とマネージャ決定 (STATUS) |
| A-2 / TC6 手順1、K-6 / TC82 (GitHub Actions) | 下記の CI 実行結果を参照 |

### GitHub Actions (CI run #8, コミット 8e6d74e)
- 結果: **success**。fmt / clippy / test / doc / deny / corpus-fetch すべて success (proptest-long は schedule/dispatch 専用で skipped)。
- corpus-fetch: `cargo corpus fetch sv-tests` と `cargo corpus discover sv-tests` が成功。
- → **A-2 (TC6 手順1) 合格、K-6 (TC82) 合格**。

## 最終判定 (マネージャ)
受け入れ基準 26 項目すべて合格。L-9 (参照モデルの独立実装) は、T9 / T9b を本体を読まない別エージェントに割り当てたことをマネージャが確認。**M0 完了。**
