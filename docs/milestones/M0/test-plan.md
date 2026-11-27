# M0「基盤」テスト計画 (M0-TP)

- 作成: テスト計画エージェント (test-planner)、2026-11-27
- 入力: `CLAUDE.md`、`docs/PLAN.md` v0.4、`docs/milestones/M0/plan.md` v2.1 (§11 VCS 確認リスト V-1〜V-18、§12 Q-1〜Q-22 の回答)、`docs/milestones/M0/plan-review.md`、`docs/STATUS.md` (決定事項ログ)
- 状態: v2 (ドキュメントのみ。テストコードは各実装タスクと、マネージャの指示があれば後でテスト計画エージェントが書く)

## 改訂履歴
| 版 | 日付 | 内容 |
|---|---|---|
| v1 | 2026-11-27 | 初版。未確定事項 Q-1〜Q-22 を §8 に列挙 |
| v2 | 2026-11-27 | plan.md v2.1 §12 と STATUS の決定に従い Q-1〜Q-22 を解消し、各 TC の期待値を確定 (§8 は回答の反映先の一覧に変更)。§7 を V-1〜V-18 に合わせた (V-15〜V-18 を追加)。TC64 は承認済みの T10 の書式 (rustc 風) でゴールデンを固定。STATUS の決定 (ops の命名、proptest 回帰ファイルの置き場所、file:// URL の拒否) を反映 |

> **方針**
> 1. 期待値は**計画 (plan.md v2.1) と IEEE 1800 の意味論から導いた**。svsim-logic と svsim-corpus の実装のコードは見ていない。例外は svsim-common の診断表示で、マネージャの指示により、承認済みの T10 の書式 (`crates/svsim-common/tests/diag_render.rs` の出力) を仕様として読んだ (TC63, TC64)。
> 2. 規格本文はこの環境から参照できない。条番号の確度は plan.md 冒頭の注記と plan-review.md §1.1 に従う。**表番号は書かない**。
> 3. 規格の確度が低いもの、規格が規定していないもの、他実装で分かれるものは「**暫定期待値 [V-n]**」と書き、plan.md §11 の VCS 確認リストと紐付けた (§7 に一覧)。VCS の結果が出たら、その期待値だけを差し替える。
> 4. 計画に書かれていない細部 (エラーにするか panic にするか、等) は v1 では期待値を作らず、§8 に質問として挙げた。v2 では plan.md v2.1 §12 の回答 (マネージャが採用を決定) に従って期待値を確定した。回答のうち Q-11・Q-12・Q-13・Q-14 と TC64 の表示書式は、マネージャが T10 の実装済みの挙動を仕様として承認したもの。
> 5. 数値の期待値のうち手計算が面倒なもの (多倍長の剰余、FNV-1a ハッシュ、浮動小数の丸め) は、Python の任意精度整数と `struct` で計算して確かめた (実装とは無関係の検算)。

---

## 1. テストの種類と配置

| 種類 | 記号 | 内容 | 置き場所 (plan.md §3.7 / §8.1 の担当ファイル) |
|---|---|---|---|
| 単体テスト | U | 各モジュールの API の基本動作 | 各 crate の `src/**` 内 `#[cfg(test)]` |
| ゴールデンテスト | G | 規格の表・例と、本計画で導いた固定値。テスト名かコメントに条番号と TC ID を書く | `crates/svsim-logic/tests/golden_*.rs` ほか |
| 網羅テスト | E | 小さい幅の全値 × 全組み合わせ | `golden_*.rs` / `diff_*.rs` (各タスクの担当ファイル) |
| 差分テスト (proptest) | D | 本体の 3 層 (層 1 スカラ、層 2 スライス、層 3 `LogicVec`) と `_assign` 版・2 値版・混在カーネルを、参照モデル `svsim-logic-ref` と比べる (plan §5.2) | `crates/svsim-logic/tests/diff_*.rs` |
| 性質ベーステスト | P | 参照モデルを使わない代数的な性質 (往復、交換則、恒等式) | `diff_*.rs` に同居してよい |
| ヒープ確保検査 | N | 確保回数を数える `GlobalAlloc` (plan §5.5) | `crates/svsim-logic/tests/no_alloc_*.rs` |
| 結合テスト | I | アリーナ × ロジック、コーパスの取得 (file://) など crate をまたぐもの | `crates/svsim-common/tests/`、`crates/svsim-corpus/tests/` |
| E2E (CLI) | X | `cargo corpus ...` を実際の sv-tests で動かす | 手順 (テスト実行エージェントが実行) |
| 静的検査 | S | grep・設定ファイルの確認 | 手順 |
| 決定性テスト | R | 同じ入力で同じ結果 (スレッド、実行回数、発行順に依存しない) | 各所 |
| スモーク (大幅) | W | 幅 2^20 (`#[ignore]`、定期ジョブ) | `diff_struct.rs` 等 |
| 手動 | M | 人の確認 (STATUS、CI の画面、ドキュメントの内容) | — |

**M0 で行わないもの** (plan §1.3 に従う)
- SV ソースを入力にする E2E テスト: パーサが M1 なので無い。M1 の VCS 確認用 SV テスト集の候補は付録 A に書いた。
- Icarus / Verilator / VCS との出力比較: M1 以降。M0 の外部オラクルは参照モデル (num-bigint) だけ。
- 性能ベンチマーク: M5 で本格化 (plan §1.3)。M0 では TC53 (幅 2^20) で所要時間を**記録するだけ**にし、合否には使わない。

**トレーサビリティの規約** (実装タスクへの依頼)
- テスト関数名に TC ID を小文字で入れる (例: `tc13_and_table_11_4_8`)。1 つの TC を複数の関数で実装してよい。
- 本文の関数名 (`and`、`shl` 等) は plan §3.5 の名前。実際のパスは STATUS の決定により `ops::<分野>::<名前>` (例: `ops::bitwise::and`)、同じファイル内の層 1 のスカラ版は接尾辞 `_l64` (例: `ops::bitwise::and_l64`)。`ops` の glob 再エクスポートは無い。
- ゴールデンテストのコメントに条番号 (例: `// IEEE 1800-2023 §11.4.8`) を書き、表番号は書かない。暫定期待値には `// [要VCS確認 V-n] 暫定` と書く。

---

## 2. 実行環境と外部ツール

| ツール | 用途 | 必須の TC |
|---|---|---|
| rustup + Rust 1.97.0 (`rust-toolchain.toml` で自動導入、rustfmt / clippy) | 全般 | 全部 |
| cargo-deny | ライセンス・ソースの検査 | TC4 |
| git (2.x、`git fetch --depth 1 <sha>` が使えるもの。レビューでは 2.43 で確認) | コーパスの取得、file:// のテスト | TC73〜TC80 |
| ネットワーク (github.com への HTTPS) | 実際の sv-tests の取得 | TC78〜TC80 (初回のみ) |
| gh (GitHub CLI) またはブラウザ | CI の結果の確認 | TC6, TC82 |
| jq | `results.jsonl` / `summary.json` の検査 (手順の補助。無ければ Python で代用) | TC79, TC80 |
| GNU coreutils / grep / find | 静的検査、件数の独立な検算 | TC7, TC59, TC79 |
| Icarus / Verilator / VCS | **M0 では不要** (付録 A は M1 用) | — |

環境変数: `PROPTEST_CASES` (既定 256、長時間ジョブ 100000)、`SVSIM_CORPUS_CACHE` (既定 `~/.cache/svsim/corpus`)。

---

## 3. 全テストに共通の検査規則 (svsim-logic)

差分テスト・ゴールデンテストのすべてで、次を毎回検査する (plan §5.2)。個々の TC では繰り返さない。

| ID | 規則 | 根拠 |
|---|---|---|
| CR-1 | 結果を参照モデル (または固定値) と、**aval・bval とも全ビット同一**で比べる (`===` の意味)。 | plan §5.2 |
| CR-2 | 出力の**正規化不変条件**: 最上位ワードの `w` ビット目以上が両プレーンとも 0。`LogicVec` の場合はヒープ版でもインライン版でも。 | plan §3.2、L-4 |
| CR-3 | 層 1 (幅 ≤ 64 のときだけ)、層 2、層 3 をそれぞれ比べる。層 2 は**プレーンが隣接しないビュー** (a と b を別のバッファに置いたもの) でも 1 回は比べる。 | plan §3.3, §5.2 |
| CR-4 | `_assign` 版がある演算は、同じ入力で非 assign 版と同じ結果。 | plan §3.3 規約 2 |
| CR-5 | 2 値版は X/Z を含まない入力で、「参照モデルの結果 → `to_two_state`」と比べる。 | plan §5.2 |
| CR-6 | 混在カーネル (§3.5.1) は「`from_two_state` してから 4 値カーネル」と参照モデルの両方に一致する (選択の例外 `get_slice2`/`set_slice2` を除く)。 | plan §3.5.1, §5.2 |
| CR-7 | 入力の値を変更しない (入力の `LogicVec` を実行前に複製し、実行後に `===` で比べる)。 | plan §3.3 (借用規則) |
| CR-8 | 参照モデルとの受け渡しは bits 文字列 (`to_bits_string` ⇔ `RefVec::from_bits_str`) だけで行う。 | plan §3.7 |

**表記**: `w'b...` は MSB が左。`x`/`z` は小文字で書くが、期待値は「その値を持つビット」を意味する。`S` は符号付き (`signed = true`)、`U` は符号なし。

---

## 4. 受け入れ基準とテスト項目の対応表

| 受け入れ基準 (plan §9) | 内容 (要約) | テスト項目 | 種別 |
|---|---|---|---|
| A-1 | build/test/fmt/clippy/doc/deny が 1.97.0 で通る | TC1, TC2, TC3, TC4, TC5 | S |
| A-2 | CI の push/PR ジョブ全部が緑、action は SHA 固定 | TC6 | M/S |
| A-3 | unsafe の方針 | TC7, TC8 | S/M |
| L-1 | §3.5・§3.5.1 の全演算 (4 値・2 値・混在) が 3 層で参照モデルと一致、`_assign` も | TC9〜TC51 (チェックリストは §5.2.0)。差分の中心は TC23, TC28, TC30, TC34, TC35, TC39, TC44, TC49, TC51 | D/G/E/U |
| L-2 | `PROPTEST_CASES=100000` で通る | TC55 | D |
| L-3 | §5.4 の網羅テストとゴールデンテスト (条番号付き) が通る | TC13〜TC22, TC24〜TC27, TC29, TC31〜TC34, TC36〜TC38, TC40〜TC43, TC45〜TC48, TC50 | G/E |
| L-4 | 全演算の出力が正規化不変条件を満たす | TC12 (+ CR-2 を全差分テストで) | D/U |
| L-5 | 幅の境界 (1,63,64,65,127,128,129) と幅 > 1000、幅 0 の連結・複製 | TC52, TC32, TC53 | G/D/W |
| L-6 | 幅 ≤ 64 の `LogicVec` と、事前確保 `Scratch` のスライスカーネルがヒープ確保しない | TC54 | N |
| L-7 | `abi.rs` の往復、`svLogicVecVal` 配置の一致、未使用ビットのマスク | TC50, TC51 | G/D |
| L-8 | 公開 API の doc、crate ドキュメントに配置の契約・幅/符号規則・混在規則 | TC3, TC59 | S/M |
| L-9 | 参照モデルは別エージェント、参照モデルの golden が通る | TC60 | M/G |
| C-1 | ソースマップの行・列 (CRLF、最終行改行なし、空ファイル、不正 UTF-8) | TC61, TC62 | U |
| C-2 | 診断 JSON の golden、単一行の人間向け表示の golden | TC63, TC64 | G |
| C-3 | `into_sorted` が発行順に依存しない | TC65 | P/R |
| C-4 | インターン | TC66 | U/P |
| C-5 | `WordArena` (位置合わせ・溢れ、`disjoint_mut`、スナップショット/ハッシュ、in-place の結合) | TC68, TC69, TC70, TC71 (+ 補助 TC67) | U/G/I |
| K-1 | sv-tests の固定コミット取得、rev 検証、2 回目はネットワーク不要 | TC73, TC74, TC78 | I/X |
| K-2 | discover が 1028 件、解析エラー 0、should_fail 79、`:type:` 分布 | TC76, TC79 | U/X |
| K-3 | run が JSONL と summary.json を出し全件 `Preprocess: NotImplemented`、2 回で同一 | TC77, TC80 | U/X/R |
| K-4 | マニフェストの誤りがエラーになる | TC72 | U |
| K-5 | 古いロックの自動除去、生きているロックでエラー | TC75 | I |
| K-6 | CI の `corpus-fetch` ジョブが緑 | TC82 | M |
| (計画 §7.5) | CLI の終了コード | TC81 | X |
| (計画 §3.3 規約 3) | カーネルは幅の assert 以外で panic しない | TC57 | U/D |
| (計画 §3.10 並列、PLAN §1 原則 2) | 決定性 | TC58 (+ TC65, TC70, TC80) | R |
| (計画 §5.3) | proptest の入力分布 | TC56 | U |

カバー状況: plan §9 の受け入れ基準 **26 項目 (A-1〜A-3、L-1〜L-9、C-1〜C-5、K-1〜K-6) をすべてカバー**する。自動化できないのは A-2 / K-6 (CI 画面の確認。`gh` で半自動)、A-3 の否定テスト (TC8)、L-8 の内容確認 (TC59 の一部)、L-9 の担当確認 (TC60 の一部)。

---

## 5. テスト項目の詳細

### 5.1 共通 (A 系)

#### M0-TC1 ビルドとテスト (`--locked`)
- 目的: A-1。依存が `Cargo.lock` で固定され、全テストが通る。
- 入力 / 手順: クリーンな checkout で `cargo build --workspace --locked`、`cargo test --workspace --locked`。
- 期待結果: どちらも終了コード 0。`cargo test` の出力に `FAILED` が無い。`ignored` は TC53 などの `#[ignore]` だけ。
- 根拠: plan §9.1 A-1、§2.2 (Cargo.lock の確定)。
- 実行: `cargo build --workspace --locked && cargo test --workspace --locked` / 自動 (CI `test` ジョブ)。

#### M0-TC2 fmt と clippy
- 目的: A-1。
- 手順: `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`。
- 期待結果: 終了コード 0。clippy は `--all-targets` なのでテストとベンチも含む。`missing_docs` が warn → `-D warnings` でエラーになる構成なので、doc の無い公開項目があれば失敗する (L-8 の一部)。
- 根拠: plan §2.2、§9.1。
- 実行: 上記コマンド / 自動 (CI `fmt`, `clippy`)。

#### M0-TC3 rustdoc
- 目的: A-1, L-8。リンク切れ・doc の警告が無い。
- 手順: `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`。
- 期待結果: 終了コード 0。
- 実行: 自動 (CI `doc`)。

#### M0-TC4 ライセンスとソースの検査
- 目的: A-1。
- 手順: `cargo deny check licenses bans sources`。
- 期待結果: 終了コード 0。加えて `deny.toml` の許可リストに `Unlicense` が**単独では入っていない**こと、`BSD-3-Clause` `Zlib` など M0 の依存で不要なものが入っていないこと (`cargo deny list` の結果と照合)。
- 根拠: plan §6 `deny` 行 (R-25)、CLAUDE.md §3。
- 実行: 自動 (CI `deny`) + 許可リストの照合は手動。

#### M0-TC5 ツールチェーンの固定
- 目的: A-1 の前提 (決定 U-3)。
- 手順: `cat rust-toolchain.toml`、`rustc --version`。
- 期待結果: `channel = "1.97.0"`、components に `rustfmt` と `clippy`。`rustc --version` が `rustc 1.97.0`。ルート `Cargo.toml` の `rust-version = "1.97"`、`edition = "2024"`、`resolver = "3"`。
- 実行: 手順 / 自動化可 (S)。

#### M0-TC6 CI が緑で、action が SHA 固定
- 目的: A-2 (決定 U-4, U-5)。
- 手順:
  1. `gh run list --branch <作業ブランチ> --limit 5` と `gh run view <id>` で、最新のコミットの `fmt clippy test doc deny` (T13 後は `corpus-fetch` も) が success。
  2. `grep -nE '^\s*-?\s*uses:' .github/workflows/*.yml` の全行が `uses: <owner>/<repo>[/path]@<40 桁の小文字 16 進> # <タグ>` の形 (正規表現 `@[0-9a-f]{40}\s+#\s*v?\S+`)。`@v4` や `@main` が 1 つも無い。
  3. `proptest-long` ジョブが `schedule` と `workflow_dispatch` を持ち、`PROPTEST_CASES=100000` と `--ignored` を実行する (L-2 / TC53 の定期実行)。
- 期待結果: 上記すべて。
- 根拠: plan §6、§9.1 A-2。
- 実行: 2, 3 は自動化可 (S)、1 は `gh` で半自動 (M)。

#### M0-TC7 unsafe の方針 (静的検査)
- 目的: A-3。
- 手順と期待結果:
  1. ルート `Cargo.toml` の `[workspace.lints.rust]` に `unsafe_code = "deny"` (`forbid` ではない)、`missing_docs = "warn"`、`[workspace.lints.clippy]` に `undocumented_unsafe_blocks = "deny"`。
  2. 全 crate の `Cargo.toml` に `[lints]` と `workspace = true`。
  3. `crates/*/src/lib.rs` と `crates/*/src/main.rs` (存在するもの全部) の先頭の属性に `#![forbid(unsafe_code)]`。
  4. `grep -rnw unsafe crates --include='*.rs'` の一致が `crates/*/tests/no_alloc*.rs` の中だけ。そのファイルの先頭に `#![allow(unsafe_code)]`、各 `unsafe` の直前に `// SAFETY:`。
- 根拠: plan §2.1 unsafe の方針 (R-1)、§9.1 A-3、CLAUDE.md §3。
- 実行: シェルスクリプト / 自動化可 (S)。

#### M0-TC8 unsafe の方針 (否定テスト)
- 目的: A-3 が実際に効いていること。
- 手順: 作業ツリーの**一時コピー** (スクラッチ領域) で、`crates/svsim-logic/src/layout.rs` に `pub fn f() { unsafe {} }` を足して `cargo build -p svsim-logic`。元のツリーは変更しない。
- 期待結果: `forbid(unsafe_code)` によるコンパイルエラー (E0453 ではなく unsafe_code の lint エラー)。
- 実行: 手動 (M)。1 回だけ。

---

### 5.2 `svsim-logic` (L 系)

#### 5.2.0 L-1 のチェックリスト (演算 × 版)
「要」はその版のテストが必要なもの。「—」は計画上その版が無いもの。テスト実行エージェントは、各「要」に対応するテストが存在し通ることを確認する。

| 演算 (plan §3.5) | 4 値 層1 | 4 値 層2 | 4 値 層3 | `_assign` | 2 値 (層1/2/3) | 混在 | TC |
|---|---|---|---|---|---|---|---|
| `not` | 要 | 要 | 要 | 要 | 要 | — | 13, 23 |
| `and or xor xnor` | 要 | 要 | 要 | 要 | 要 | — | 13, 22, 23 |
| `reduce_and/nand/or/nor/xor/xnor` | 要 | 要 | 要 | — | 要 | — | 14, 23 |
| `truth`、`Logic::{not,and,or,implies,equiv}`、`logical_not` | 要 | 要 | 要 | — | 要 | — | 15, 23 |
| `count_ones count_bits onehot onehot0 is_unknown` | 要 | 要 | 要 | — | 要 | — | 14, 23 |
| `eq ne` | 要 | 要 | 要 | — | 要 | — | 16, 22, 23 |
| `case_eq case_ne` | 要 | 要 | 要 | — | 要 | — | 16, 22, 23 |
| `wildcard_eq wildcard_ne` | 要 | 要 | 要 | — | 要 | — | 17, 22, 23 |
| `casez_match casex_match` | 要 | 要 | 要 | — | 要 | — | 18, 22, 23 |
| `lt le gt ge` (S/U) | 要 | 要 | 要 | — | 要 | — | 19, 22, 23 |
| `cond_merge` / `cond_merge_bits` | 要 | 要 | 要 | — | — | 要 (`cond_merge_bits`) | 20, 22, 23, 34 |
| `edge edge_lsb edge2` | 要 (scalar 版) | 要 (`edge_lsb`) | — | — | 要 (`edge2`) | — | 21 |
| `add sub neg` | 要 | 要 | 要 | 要 | 要 | — | 24, 28 |
| `inc dec` | 要 | 要 | 要 | 要 (本体が assign) | 要 | — | 24, 28 |
| `mul` | 要 | 要 | 要 | (あれば要) | 要 | — | 25, 28 |
| `div rem` (S/U) | 要 | 要 | 要 | (あれば要) | 要 (`div2 rem2`) | — | 26, 28 |
| `pow` (base S/U × exp S/U) | 要 | 要 | 要 | — | 要 (`pow2`) | 要 (`pow_bits`) | 27, 28, 34 |
| `shl shr ashr` と `_const` 版 | 要 | 要 | 要 | 要 | 要 | 要 (`shl_bits shr_bits ashr_bits`) | 29, 30, 34 |
| `resize` (S/U) | 要 | 要 | 要 | — | 要 | — | 31, 35 |
| `to_two_state from_two_state` | 要 | 要 | 要 | — | — | — | 31, 35 |
| `concat replicate` | — (幅が変わる) | 要 | 要 | — | 要 | — | 32, 35 |
| `get_slice get_bit set_slice set_bit index_value` | 要 | 要 | 要 | — | 要 (`get_slice2 set_slice2`) | 例外規則 | 33, 34, 35 |
| `clog2` | 要 | 要 | 要 | — | 要 | — | 36 |
| リテラル `parse_based parse_unsized_decimal` | (単一 API) | | | | | | 37, 38, 39 |
| 文字列 `from_string_bytes to_string_bytes atoi` | (単一 API) | | | | | | 40, 41 |
| 実数 `to_f64 to_f32 from_f64_round from_f64_trunc` と bits 系 | (単一 API) | | | | | | 42, 43, 44 |
| 表示 `format_int format_int2 itoa` | (単一 API) | | | | | | 45〜49 |
| ABI `write/read_sv_logic_vec_val`、`write/read_sv_bit_vec_val` | (単一 API) | | | | | | 50, 51 |

「層 1」は幅 ≤ 64 のスカラ関数 (`scalar` / 各演算ファイル内のスカラ版)。比較・リダクションなど 1 ビットを返す演算の層 1 は `L64` を受けて `Logic` を返す版。

#### M0-TC9 `Logic` と配置の補助関数
- 目的: 符号化と配置の基礎 (plan §3.1, §3.2)。L-1, L-7 の前提。
- 入力と期待結果 (G/U):
  | 項目 | 期待 |
  |---|---|
  | `Logic::L0 as u8, L1, Z, X` | `0, 1, 2, 3` (svdpi.h の `sv_0 sv_1 sv_z sv_x`) |
  | `Logic` の `Display` | `'0' '1' 'z' 'x'` |
  | `words_for(w)` for w = 0,1,63,64,65,128,129, 2^24 | 0,1,1,1,2,2,3, 262144 |
  | `top_mask(w)` for w = 1, 2, 63, 64, 65, 128, 129 | `0x1`, `0x3`, `0x7fff_ffff_ffff_ffff`, `!0`, `0x1`, `!0`, `0x1` |
  | `MAX_WIDTH` | `1 << 24` (決定 U-6) |
- 根拠: plan §3.1, §3.2、Annex I (svdpi.h)。
- 実行: `cargo test -p svsim-logic` (src 内の単体テスト) / 自動。

#### M0-TC10 `LogicVec` / `BitVec` の構築と変換
- 目的: 層 3 の基本 API。全差分テストの土台 (bits 文字列)。
- 入力と期待結果 (G/U/P):
  | 呼び出し | 期待 |
  |---|---|
  | `from_u64(4, 0x1f)` | `4'b1111` |
  | `from_u64(128, 0xdead)` | 下位 16 ビットが `dead`、bit 16..127 は 0 |
  | `from_i64(4, -1)` / `from_i64(4, 8)` / `from_i64(65, -1)` / `from_i64(63, -1)` | `4'b1111` / `4'b1000` / 65 ビット全 1 (bit 64 も 1) / 63 ビット全 1 (bit 63 以上は 0: CR-2) |
  | `zero(w)`, `filled(w, X)`, `filled(w, Z)` (w = 1, 64, 65, 129) | 全 0 / 全 x / 全 z。正規化されている |
  | `from_bits_str("10xz01")` | 幅 6、`10xz01` |
  | `from_bits_str("10XZ01")` | 幅 6、`10xz01` (大文字を受ける。Q-22) |
  | `from_bits_str("")` | `Ok`、幅 0 (Q-22) |
  | `from_bits_str("10xz_01")`、`"102"`、`"1 0"`、`"?"` | `Err` (`_` も含め 0/1/x/X/z/Z 以外はエラー。Q-22) |
  | `from_bits_str` → `to_bits_string` | 小文字にした元の文字列に戻る (P: 任意の 0/1/x/X/z/Z 列、幅 0〜1024) |
  | `RefVec::from_bits_str` | 上の 4 行と同じ結果 (参照モデルとの受け渡しの前提。TC60) |
  | `to_u64()`: 65 ビットで bit 64=0・下位=5 / bit 64=1 / X を含む | `Some(5)` / `None` / `None` |
  | `to_i64(true)` / `to_i64(false)` for `8'hff` | `Some(-1)` / `Some(255)` |
  | `to_i64(true)` for 65 ビット全 1 | `Some(-1)` |
  | `to_bit_vec()` for `4'b1xz0` | `4'b1000` (§6.11.2) |
  | `LogicVec::from(&BitVec)` | bval 全 0 |
  | `Debug` for `8'b10xz0001` | `8'b10xz_0001` |
  | `PartialEq`: `4'b0001` vs `8'b0000_0001` | 等しくない (幅が違う) |
  | `PartialEq`/`Hash`: 幅 64 と幅 65 の境界で、同じ幅・同じ値を別経路 (`from_u64`、`from_bits_str`、演算結果) で作ったもの | 等しく、ハッシュも等しい (plan §8.3 T3a: 表現は幅だけで決まる) |
- 根拠: plan §3.3 主要な型、§6.11.2。
- 実行: `cargo test -p svsim-logic` (vec.rs 内) と `--test diff_core` / 自動。

#### M0-TC11 ビュー (`LogicRef` / `LogicMut` / `BitRef` / `BitMut`)
- 目的: 層 2 のビューの基本。
- 入力と期待結果:
  - `from_planar(65, &[a0, a1, b0, b1])` の `aval() == [a0,a1]`、`bval() == [b0,b1]`。長さが `2*words_for(w)` でなければ **panic** (release でも。Q-1)。長さ 3 と 5 で `#[should_panic]`。
  - `bit(i)` / `set_bit(i, v)`: w = 129 で i = 0, 63, 64, 127, 128 に 0/1/x/z を書いて読み戻すと同じ。他のビットは変わらない。i = 129 (範囲外) は `bit` も `set_bit` も **panic** (Q-1)。
  - `fill(X)` (w = 65) → 全 x、bit 65 以上は 0 (CR-2)。
  - `has_unknown()`: 全 0 → false、bit 128 だけ z → true。
  - `copy_from`: 幅が違うと assert で panic。
  - **プレーン非隣接**: a と b を別の `Vec<u64>` に置いた `LogicRef { a, b }` でも、`from_planar` と同じ結果。
- 根拠: plan §3.3。
- 実行: `--test diff_core` / 自動。

#### M0-TC12 正規化不変条件 (L-4)
- 目的: L-4。差分テストの検査器そのものが正しいことと、違反しやすい演算での確認。
- 内容:
  1. **検査器の自己テスト**: `tests/common/mod.rs` の検査関数に、最上位ワードの未使用ビットに 1 を入れた手作りのワード列 (aval 側、bval 側それぞれ) を渡すと**違反を検出する**。正しい値では検出しない。
  2. **違反しやすい演算の固定テスト** (幅 1, 63, 65, 127, 129): `not`、`xnor`、`neg`、`sub` (0 − 1)、`dec` (0 → 全 1)、符号付き `resize` での拡張、`ashr` (符号 1 / x / z)、`cond_merge` (X の生成)、`fill(X)`、`from_i64(w, -1)`、`filled(w, Z)`、`read_sv_logic_vec_val` (未使用ビットに 1)、`parse_based` の切り詰め、`from_f64_round(-1.0)`、`to_two_state`。いずれも出力が正規化されている。
  3. 全差分テスト (TC23, 28, 30, 34, 35, 39, 44, 51) で CR-2 を検査していることを、コードレビューで確認する。
- 根拠: plan §3.2 正規化不変条件、§5.2。
- 実行: `cargo test -p svsim-logic` / 自動 (+ 3 は M)。

#### M0-TC13 ビット演算の真理値表 (§11.4.8)
- 目的: L-1, L-3。
- 入力: 1 ビットの 4 値 {0,1,x,z} の全 16 組 (単項は 4 通り)。期待値は §11.4.8 の表 (z は入力として x と同じ行、結果に z は出ない)。
  | `&` | 0 | 1 | x | z |
  |---|---|---|---|---|
  | 0 | 0 | 0 | 0 | 0 |
  | 1 | 0 | 1 | x | x |
  | x | 0 | x | x | x |
  | z | 0 | x | x | x |

  | `\|` | 0 | 1 | x | z |
  |---|---|---|---|---|
  | 0 | 0 | 1 | x | x |
  | 1 | 1 | 1 | 1 | 1 |
  | x | x | 1 | x | x |
  | z | x | 1 | x | x |

  | `^` | 0 | 1 | x | z |
  |---|---|---|---|---|
  | 0 | 0 | 1 | x | x |
  | 1 | 1 | 0 | x | x |
  | x | x | x | x | x |
  | z | x | x | x | x |

  `~^` と `^~` は `^` の結果の 0 と 1 を入れ替えたもの (x は x)。`~`: 0→1, 1→0, x→x, z→x。
- 追加のベクタ例: `8'b0101_xz10 & 8'b0011_0zz1` → `8'b0001_0xx0`、`|` → `8'b0111_xx11` (bit 1 は 1 | z = 1)、`^` → `8'b0110_xxx1`。
  - 2 値: `8'hf0 & 8'h3c` = `8'h30`、`|` = `8'hfc`、`^` = `8'hcc`、`~^` = `8'h33`、`~8'hf0` = `8'h0f`。
- 根拠: §11.4.8、plan §3.4 (式は手計算で表と一致済み: plan-review §1.2)。
- 実行: `cargo test -p svsim-logic --test golden_bitwise` / 自動。

#### M0-TC14 リダクションと `$countbits` 系 (§11.4.9, §20.9)
- 目的: L-1, L-3。オペランドは自己決定 (幅はそのまま)。
- 期待結果:
  | 入力 | `&` | `~&` | `\|` | `~\|` | `^` | `~^` |
  |---|---|---|---|---|---|---|
  | `4'b1111` | 1 | 0 | 1 | 0 | 0 | 1 |
  | `4'b0000` | 0 | 1 | 0 | 1 | 0 | 1 |
  | `4'b1011` | 0 | 1 | 1 | 0 | 1 | 0 |
  | `4'b1x11` | x | x | 1 | 0 | x | x |
  | `4'b0x11` | 0 | 1 | 1 | 0 | x | x |
  | `4'b000x` | 0 | 1 | x | x | x | x |
  | `4'bz111` | x | x | 1 | 0 | x | x |
  | `4'b000z` | 0 | 1 | x | x | x | x |
  | `1'b1` / `1'bz` | 1 / x | 0 / x | 1 / x | 0 / x | 1 / x | 0 / x |
  | 129 ビット全 1 | 1 | 0 | 1 | 0 | 1 (129 個) | 0 |
  | 128 ビット全 1 | 1 | 0 | 1 | 0 | 0 | 1 |
  | 129 ビット全 1 で bit 128 だけ 0 | 0 | 1 | 1 | 0 | 0 | 1 |
  | 129 ビット全 1 で bit 128 だけ x | x | x | 1 | 0 | x | x |
  | 129 ビット全 0 で bit 128 だけ 1 | 0 | 1 | 1 | 0 | 1 | 0 |
- `$countbits` 系 (入力 `8'b10xz_01xz`): `count_bits({1})` = 2、`{0}` = 2、`{x}` = 2、`{z}` = 2、`{x,z}` = 4、`{0,1,x,z}` = 8、空集合 = 0 (Q-2)。`count_ones` = 2。
- `$onehot`: `8'b0001_0000` → 1、`0` → 0、`8'b0001_0001` → 0、`8'b0001_x000` → 1 (定義 `$countbits(e,'1)==1` から)。`$onehot0`: `0` → 1、`8'b0000_0001` → 1、`8'b0000_0011` → 0。`$isunknown`: `8'b0000_000z` → 1、`8'h00` → 0。幅 129 の bit 128 だけ 1 → `onehot` 1。
- 根拠: §11.4.9, §20.9、plan §3.5(a)。
- 実行: `--test golden_bitwise` / 自動。

#### M0-TC15 真理値と論理演算 (§11.4.7, §12.4)
- 目的: L-1, L-3。
- `truth`: `4'b0000` → 0、`4'b0100` → 1、`4'b000x` → x、`4'b0z00` → x、`4'b100x` → 1、129 ビットで bit 128 だけ 1 → 1。`logical_not` は `truth` の否定 (`!4'b000x` = x)。
- `Logic` 上の表 (入力は {0,1,x}。z は `truth` を通ると x):
  | a, b | `&&` | `\|\|` | `->` | `<->` |
  |---|---|---|---|---|
  | 0,0 | 0 | 0 | 1 | 1 |
  | 0,1 | 0 | 1 | 1 | 0 |
  | 0,x | 0 | x | 1 | x |
  | 1,0 | 0 | 1 | 0 | 0 |
  | 1,1 | 1 | 1 | 1 | 1 |
  | 1,x | x | 1 | x | x |
  | x,0 | 0 | x | x | x |
  | x,1 | x | 1 | 1 | x |
  | x,x | x | x | x | x |
  (`a -> b` = `!a || b`、`a <-> b` = `(a->b) && (b->a)`)
- 根拠: §11.4.7、plan §3.5(a)。
- 実行: `--test golden_bitwise` / 自動。

#### M0-TC16 論理等価・ケース等価 (§11.4.5)
- 入力と期待結果 (幅は呼び出し側で揃えた後):
  | x | y | `==` | `!=` | `===` | `!==` |
  |---|---|---|---|---|---|
  | `4'b1010` | `4'b1010` | 1 | 0 | 1 | 0 |
  | `4'b1010` | `4'b1011` | 0 | 1 | 0 | 1 |
  | `4'b1x00` | `4'b0x00` | 0 (既知ビットの不一致) | 1 | 0 | 1 |
  | `4'b1x00` | `4'b1000` | x | x | 0 | 1 |
  | `4'b1z00` | `4'b1z00` | x | x | 1 | 0 |
  | `4'b1z00` | `4'b1x00` | x | x | 0 | 1 |
  | `4'bxxxx` | `4'bxxxx` | x | x | 1 | 0 |
  | 129 ビット: 同値で bit 128 だけ異なる (0/1) | | 0 | 1 | 0 | 1 |
  | 129 ビット: 同値で bit 128 が x、他の既知ビットで 1 か所不一致 | | 0 | 1 | 0 | 1 |
- 根拠: §11.4.5、plan §3.5(b) (plan-review: 「既知ビットの不一致で 0」は未検証だが主要実装と一致)。`4'b1x00 == 4'b0x00` → 0 と `4'b1x00 == 4'b1000` → x の 2 行は **[V-15 暫定]**。
- 実行: `--test diff_compare` 内の golden または `golden_bitwise` / 自動。

#### M0-TC17 ワイルドカード等価 (§11.4.6)
- 規則: **右**オペランドの x/z ビットだけがワイルドカード。左の x/z (ワイルドカード位置以外) → x。既知の不一致があれば 0 が優先。
  | 左 | 右 | `==?` | `!=?` |
  |---|---|---|---|
  | `4'b1010` | `4'b1x1z` | 1 | 0 |
  | `4'b1x10` | `4'b1x1z` | 1 (左の x はワイルドカード位置) | 0 |
  | `4'b1x10` | `4'b1010` | x | x |
  | `4'b0x10` | `4'b1010` | 0 | 1 |
  | `4'b1010` | `4'b10zz` | 1 | 0 |
  | `4'b1z10` | `4'b1010` | x | x |
  | `4'b1x1z` | `4'b1010` | x (左右を入れ替えると対称でない) | x |
  | `4'b0000` | `4'bxxxx` | 1 | 0 |
- 根拠: §11.4.6 (未検証)、plan §3.5(b)。表全体が **[V-16 暫定]**。
- 実行: 同上 / 自動。

#### M0-TC18 casez / casex の照合 (§12.5.1)
  | 式 | 項目 | `casez_match` | `casex_match` |
  |---|---|---|---|
  | `4'b1z01` | `4'b1101` | true (z は無視) | true |
  | `4'b1101` | `4'b1z01` | true (項目側の z も無視) | true |
  | `4'b1x01` | `4'b1101` | false (casez で x は無視しない) | true |
  | `4'b1x01` | `4'b1x01` | true (=== で一致) | true |
  | `4'b0x01` | `4'b1101` | false | false (既知ビットの不一致) |
  | `4'b0000` | `4'bzzzz` | true | true |
  | `4'b0000` | `4'bxxxx` | false | true |
  | `4'b1100` | `4'b1100` | true | true |
- 根拠: §12.5.1 (未検証)、plan §3.5(b)。表全体が **[V-17 暫定]**。
- 実行: 同上 / 自動。

#### M0-TC19 関係演算 (§11.4.4, §11.8.1)
  | x | y | signed | `<` | `<=` | `>` | `>=` |
  |---|---|---|---|---|---|---|
  | `8'h80` | `8'h7f` | true | 1 | 1 | 0 | 0 |
  | `8'h80` | `8'h7f` | false | 0 | 0 | 1 | 1 |
  | `8'hff` | `8'h00` | true | 1 | 1 | 0 | 0 |
  | `8'h05` | `8'h05` | true/false | 0 | 1 | 0 | 1 |
  | `4'b0001` | `4'b001x` | false | x (未知があれば常に x) | x | x | x |
  | `4'bz000` | `4'b0000` | true | x | x | x | x |
  | 65 ビット: x = 2^64, y = 2^64 − 1 | | false | 0 | 0 | 1 | 1 |
  | 65 ビット: x = −2^64 (bit 64 だけ 1), y = 0 | | true | 1 | 1 | 0 | 0 |
  | 128 ビット: x = 2^127, y = 1 | | true / false | 1 / 0 | 1 / 0 | 0 / 1 | 0 / 1 |
- 根拠: §11.4.4、§11.8.1 (両方符号付きのときだけ符号付き比較。判定は呼び出し側で `signed` に反映)。
- 実行: 同上 / 自動。

#### M0-TC20 条件演算子の X マージ (§11.4.11)
- 規則: 両方 0 → 0、両方 1 → 1、それ以外 → x。**z と z も x** [V-8 暫定]。
  | t | f | `cond_merge` |
  |---|---|---|
  | `4'b0110` | `4'b0101` | `4'b01xx` |
  | `4'b1x0z` | `4'b1x0z` | `4'b1x0x` (x,x → x。z,z → x [V-8]) |
  | `1'bz` | `1'bz` | `1'bx` **[V-8 暫定。Icarus は z]** |
  | `1'bz` | `1'b0` | `1'bx` |
  | `1'b1` | `1'bz` | `1'bx` |
  | 129 ビット: t = f = 任意の 0/1 列 | | t と同じ |
  | 129 ビット: t = 全 0、f = bit 128 だけ 1 | | bit 128 だけ x、残り 0 |
- `cond_merge_bits` (2 値の t, f): `t = 8'b1100_1010`, `f = 8'b1010_1010` → 4 値 `8'b1xx0_1010`。
- 条件の判定 (評価器の責務だが `truth` で確認): `c = 1'bz` → `truth` = x なのでマージ。`c = 4'b01x0` → 1 なので t。
- 根拠: §11.4.11、plan §3.5(f)、決定 R-10。
- 実行: `--test golden_bitwise` / 自動。

#### M0-TC21 エッジ検出 (§9.4.2) — 4×4 の全 16 通り
- 期待結果 (`edge(old, new)`、scalar 版も同じ) [V-12 は x↔z・z→1・1→z を含む表全体を VCS で確認する項目。暫定期待値は次の表]:
  | # | old → new | 期待 | | # | old → new | 期待 |
  |---|---|---|---|---|---|---|
  | 1 | 0 → 0 | None | | 9 | x → 0 | Neg |
  | 2 | 0 → 1 | Pos | | 10 | x → 1 | Pos |
  | 3 | 0 → x | Pos | | 11 | x → x | None |
  | 4 | 0 → z | Pos | | 12 | x → z | None [V-12] |
  | 5 | 1 → 0 | Neg | | 13 | z → 0 | Neg |
  | 6 | 1 → 1 | None | | 14 | z → 1 | Pos [V-12] |
  | 7 | 1 → x | Neg | | 15 | z → x | None [V-12] |
  | 8 | 1 → z | Neg [V-12] | | 16 | z → z | None |
- `edge2(old, new)`: (false,true) → Pos、(true,false) → Neg、同値 → None。
- `edge_lsb`: 多ビットの式は LSB だけを見る。`4'b0000 → 4'b1110` → None (LSB は 0→0)、`4'b1110 → 4'b0001` → Pos、`8'bxxxx_xxx1 → 8'b0000_0000` → Neg、幅 129 で bit 0 だけ 0→x、他は全部変化 → Pos [V-12: LSB だけを見るか]。
- 性質 (P): Pos と Neg が同時に真になる組は無い。`edge(a,a)` は常に None。
- 根拠: §9.4.2 (未検証)、plan §3.5(k)。
- 実行: `--test golden_bitwise` / 自動。

#### M0-TC22 T4 範囲の網羅テスト
- 目的: L-3 (plan §5.4)。
- 入力: 幅 1〜3 の 4 値の全値 (幅 w で 4^w 値)。同じ幅の全組 (幅 3 で 64 × 64 = 4096 組)。
- 対象: `not and or xor xnor`、`eq ne case_eq case_ne wildcard_eq wildcard_ne casez_match casex_match`、`lt le gt ge` (S/U)、`cond_merge`、リダクション 6 種、`truth`。
- 期待結果: 参照モデルと一致 (CR-1〜CR-4)。2 値版は 2^w 値の全組で CR-5。
- 実行: `--test diff_bitwise --test diff_compare` / 自動。

#### M0-TC23 T4 範囲の差分テスト (proptest)
- 目的: L-1。
- 入力: plan §5.3 の幅・値・X/Z 密度の戦略。幅の等しいオペランド組。
- 期待結果: §5.2.0 の T4 の行の全「要」について CR-1〜CR-7。
- 実行: `cargo test -p svsim-logic --test diff_bitwise --test diff_compare` / 自動。

#### M0-TC24 加減算・符号反転・増減 (§11.4.3, §11.4.2)
  | 演算 | 入力 | 期待 |
  |---|---|---|
  | add | 8: `8'hff + 8'h01` | `8'h00` (桁あふれは捨てる) |
  | add | 64: `2^64−1 + 1` | 0 |
  | add | 65: `2^64−1 + 1` | `65'h1_0000_0000_0000_0000` (ワード境界の桁上げ) |
  | add | 128: `2^128−1 + 1` | 0 |
  | add | 129: `2^128−1 + 1` | bit 128 だけ 1 |
  | add | 4 値: `4'b000x + 4'b0001` | `4'bxxxx` (1 ビットでも未知なら全 x) |
  | add | 4 値: `4'b0z00 + 4'b0000` | `4'bxxxx` |
  | add | 129: 1 オペランドの bit 128 だけ x | 129 ビット全 x |
  | sub | 8: `8'h00 − 8'h01` | `8'hff` |
  | sub | 65: `2^64 − 1` | 下位 64 ビット全 1、bit 64 = 0 (ワード境界の借り) |
  | neg | 8: `−8'h01` | `8'hff`、`−8'h80` → `8'h80`、`−8'h00` → `8'h00` |
  | neg | 1: `−1'b1` | `1'b1` |
  | inc / dec | 8: `8'hff`++ / `8'h00`-- | `8'h00` / `8'hff` |
  | inc | 128: `2^64−1`++ | `2^64` |
  | inc | 4 値: `4'b1x00`++ | `4'bxxxx` |
- 2 値版も同じ数値 (X の行を除く)。
- 根拠: §11.4.3、§11.4.2、plan §3.5(c)。
- 実行: `--test golden_arith` / 自動。

#### M0-TC25 乗算
  | 幅 | 入力 | 期待 |
  |---|---|---|
  | 8 | `8'h10 * 8'h10` | `8'h00` |
  | 8 | `8'hff * 8'hff` | `8'h01` (S/U 共通: 下位ビットは符号に依らない) |
  | 64 | `(2^64−1)^2` | 1 |
  | 65 | `(2^65−1)^2` | 1 |
  | 128 | `(2^64−1) * (2^64−1)` | `128'hffff_ffff_ffff_fffe_0000_0000_0000_0001` |
  | 128 | `(2^128−1)^2` | 1 |
  | 4 値 | `4'b0011 * 4'b000z` | `4'bxxxx` |
  | 1 | `1*1`, `1*0` | 1, 0 |
- 根拠: §11.4.3、plan §3.5(c)。
- 実行: `--test golden_arith` / 自動。

#### M0-TC26 除算と剰余 (§11.4.3)
- 規則: 0 方向へ切り捨て、剰余の符号は被除数、除数 0 → 全 x (4 値)、未知 → 全 x。
  | 幅 | x / y | signed | `/` | `%` |
  |---|---|---|---|---|
  | 8 | 200 / 7 | U | 28 | 4 |
  | 8 | −7 / 2 (`8'hf9`, `8'h02`) | S | −3 (`8'hfd`) | −1 (`8'hff`) |
  | 8 | 7 / −2 | S | −3 | 1 |
  | 8 | −7 / −2 | S | 3 | −1 |
  | 8 | `8'hf9 / 8'h02` | U | 124 (`8'h7c`) | 1 |
  | 8 | −128 / −1 (`8'h80`, `8'hff`) | S | `8'h80` **[V-1 暫定]** | 0 **[V-1 暫定]** |
  | 8 | `8'h80 / 8'hff` | U | 0 | 128 |
  | 8 | 5 / 0 | S/U | `8'hxx` | `8'hxx` |
  | 8 | 0 / 0 | S/U | `8'hxx` | `8'hxx` |
  | 8 | `8'b0000_x101 / 8'h01` | U | 全 x | 全 x |
  | 65 | 最小値 (bit 64 だけ 1) / −1 | S | 最小値 [V-1] | 0 [V-1] |
  | 128 | (2^128−1) / (2^64+1) | U | 2^64−1 | 0 |
  | 256 | u / v (Knuth D の補正 "add back" が起きる入力。下記) | U | 下記 | 下記 |
  | 129 | 2^128 / 3 | U | 0x5555…5555 (128 ビット) | 1 |
  | 1 | 1 / 1、1 / 0 | U | 1、x | 0、x |
  - Knuth D の add back: `u = 256'h7fff_ffff_ffff_ffff_8000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000`、`v = 256'h8000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0001` (上位は 0)。`u / v = 0xffff_ffff_ffff_fffe`、`u % v = 0x7fff_ffff_ffff_ffff_ffff_ffff_ffff_ffff_0000_0000_0000_0002`。(Python で `q*v+r == u` を確認済み)
- 2 値版 (`div2`, `rem2`): 除数 0 → **0** **[V-2 暫定]**。最小値 / −1 → 最小値、剰余 0 [V-1]。
- 性質 (P, X/Z なし、除数 ≠ 0、最小値/−1 を除く): `x == (x/y)*y + x%y` (幅 w の剰余環で)、`|x%y| < |y|`、`x%y` の符号は 0 か x と同じ。
- 根拠: §11.4.3、plan §3.5(c)、plan-review §1.2 (slang/Verilator と一致)。
- 実行: `--test golden_arith` / 自動。

#### M0-TC27 べき乗 (§11.4.3 のべき乗の規則)
- 幅 8 (結果幅 = base の幅)。exp の幅は自己決定なので別の幅 (4, 8, 65, 71 等) も使う。
  | base (符号) | exp (符号) | 期待 |
  |---|---|---|
  | 0 (S) | 0 (S) | 1 |
  | 0 (S) | −1 (S) | 全 x (2 値版: 0 **[V-2 暫定]**) |
  | 0 (S) | 3 (S) | 0 |
  | 1 (S) | −5 (S) | 1 |
  | −1 (S) | −1 (S) | −1 (`8'hff`) |
  | −1 (S) | −2 (S) | 1 |
  | −1 (S) | 3 (S) | −1 |
  | 2 (S) | −1 (S) | 0 |
  | −2 (S) | −3 (S) | 0 |
  | 2 (U) | 0 (U) | 1 |
  | 2 (U) | 7 (U) | `8'h80` |
  | 2 (U) | 8 (U) | 0 |
  | 3 (U) | 5 (U) | 243 (`8'hf3`) |
  | 3 (U) | 6 (U) | 217 (`8'hd9`) |
  | 3 (U) | `8'hff` (U, 255) | 171 (`8'hab`) |
  | `8'hff` (**U**, = 255) | `8'hff` (S, = −1) | 0 (base が符号なしなので −1 扱いしない。\|base\| > 1) |
  | −1 (S) | `8'hff` (**U**, = 255) | −1 (指数は非負の 255、奇数) |
  | 3 (U) | 2^64 (65 ビット U) | 1 (3^(2^64) mod 256) |
  | 2 (U) | 2^70 (71 ビット U) | 0 |
  | 1 (U) | 2^70 (71 ビット U) | 1 |
  | `8'b0000_00x1` | 2 | 全 x |
  | 3 | `4'b00z1` | 全 x |
  | 幅 128: 3 (U) | 100 (U) | `128'h6737_6856_5b41_f775_d694_7d55_cf38_13d1` (3^100 mod 2^128) |
  | 幅 1: 1 (U) | 0 | 1 |
- 根拠: §11.4.3、plan §3.5(c)、plan-review §1.2 (Verilator・slang と一致)。
- 実行: `--test golden_arith` / 自動。

#### M0-TC28 算術の網羅テストと差分テスト
- 網羅 (E): 幅 1〜6 の 2 値の全組み合わせ (幅 6 で 4096 組) × S/U で `add sub mul div rem pow neg inc dec` (pow は exp 幅 1〜4 の全値も)。参照モデルと一致 (CR-5)。
- 差分 (D): plan §5.3 の戦略 (算術は X/Z なしの比率を高く。除数 0, 1, −1, 2 のべき。指数は負・0・1・大きい値)。§5.2.0 の算術の全「要」で CR-1〜CR-7。`pow_bits` は TC34。
- 4 値の未知 (P): 入力に未知ビットが 1 つでもあれば、`+ - * / % **` の結果は全 x (幅 1〜1024)。
- 実行: `cargo test -p svsim-logic --test diff_arith` / 自動。

#### M0-TC29 シフト (§11.4.10)
- 規則: シフト量は自己決定・常に符号なし。シフト量に未知 → 全 x。シフト量 ≥ w → `<< >>` は全 0、`>>>` (符号付き) は全ビットが符号ビットの値。データの x/z はそのまま運ぶ。
  | 幅 | 式 | 期待 |
  |---|---|---|
  | 8 | `8'b1001_x0z1 << 2` (`<<<` も同じ) | `8'b01x0_z100` |
  | 8 | `8'b1001_x0z1 >> 2` | `8'b0010_01x0` |
  | 8 | `8'b1001_x0z1 >>> 2` (S) | `8'b1110_01x0` |
  | 8 | `8'b1001_x0z1 >>> 2` (U) | `8'b0010_01x0` (`>>` と同じ) |
  | 8 | `8'h01 << 8`、`<< 9`、`<< 2^70` | 0 |
  | 8 | `8'h80 >>> 100` (S) | `8'hff` |
  | 8 | `8'h7f >>> 100` (S) | `8'h00` |
  | 4 | `4'bx001 >>> 1` (S) | `4'bxx00` |
  | 4 | `4'bz000 >>> 1` (S) | `4'bzz00` **[V-3 暫定]** |
  | 4 | `4'bz000 >>> 8` (S) | `4'bzzzz` **[V-3 暫定]** |
  | 8 | `8'h01 << 3'bx01`、`<< 3'b0z1`、`>>> 3'bx` | `8'hxx` (全 x) |
  | 8 | `8'h01 << 4'b1111` (シフト量の型が符号付きでも 15 として扱う) | 0 |
  | 8 | `8'h80 >> 3'b111` | `8'h01` |
  | 64 | `1 << 63` / `1 << 64` | bit 63 / 0 |
  | 65 | `1 << 64` | bit 64 |
  | 128 | `64'h8000…` を下位ワードに置いて `<< 1` | bit 64 だけ 1 (ワード境界をまたぐ) |
  | 129 | bit 128 = 1 (S) を `>>> 64` | bit 64〜128 が 1 |
  | 8 | シフト量 = 128 ビットで 2^64 (上位ワードに 1) | `<<` → 0 (切り詰めない) |
  | 8 | シフト量 0 | 入力と同じ (x/z も含め) |
- `_const` 版 (`shl_const(dst, x, n: u64)`): n = 0, 7, 8, `u64::MAX` で同じ規則。
- 根拠: §11.4.10、plan §3.5(d)、決定 (V-3 暫定)。
- 実行: `--test golden_struct` / 自動。

#### M0-TC30 シフトの網羅テストと差分テスト
- 網羅 (E): 幅 1〜4 の 4 値の全値 × シフト量 `0..=w+1` と、x を含むシフト量 (`2'bx0`, `3'bz11` 等) で `shl shr ashr(S/U)`。
- 差分 (D): シフト量の戦略 (plan §5.3: 小さい値、64 の倍数の前後、64 ビット超の巨大値、X/Z 入り)。`_assign` と `_const` 版も CR-4。
- 性質 (P): X/Z なしで `shl(x, a+b) == shl(shl(x,a), b)` (a+b がオーバフローしない範囲)、`shr(shl(x,n),n)` は上位 n ビットを 0 にした x。
- 実行: `--test diff_struct` / 自動。

#### M0-TC31 拡張・切り詰め・2 値/4 値の変換 (§11.6.1, §11.8.2, §6.11.2)
  | 呼び出し | 期待 |
  |---|---|
  | `resize(8, 4'b1010, S)` / `U` | `8'b1111_1010` / `8'b0000_1010` |
  | `resize(8, 4'bx010, S)` | `8'bxxxx_x010` |
  | `resize(8, 4'bz010, S)` | `8'bzzzz_z010` |
  | `resize(8, 4'bz010, U)` | `8'b0000_z010` |
  | `resize(4, 8'b1x0z_0110, S/U)` | `4'b0110` (下位を残す) |
  | `resize(65, 64'h8000_0000_0000_0000, S)` | bit 63 と bit 64 が 1 |
  | `resize(65, 64'h8000_0000_0000_0000, U)` | bit 63 だけ 1 |
  | `resize(129, 1'b1, S)` | 129 ビット全 1 |
  | `resize(129, 1'bx, S)` | 129 ビット全 x |
  | `resize(64, 65 ビット全 1, *)` | 64 ビット全 1 |
  | `resize(w, x, *)` (同じ幅) | x と同じ |
  | `to_two_state(4'b1xz0)` | `4'b1000` |
  | `from_two_state(8'ha5)` | 4 値 `8'ha5`、bval 全 0 |
- 性質 (P): 拡張してから元の幅に切り詰めると元に戻る。`from_two_state ∘ to_two_state` は X/Z なしの入力で恒等。
- 根拠: §11.6.1, §11.8.2, §6.11.2、plan §3.5(e)。
- 実行: `--test golden_struct` / 自動。

#### M0-TC32 連結・複製 (§11.4.12, §11.4.12.1) — 幅 0 を含む (L-5)
  | 呼び出し | 期待 |
  |---|---|
  | `concat([4'b10xz, 幅 0, 2'b01])` | `6'b10xz01` (先頭が MSB 側、幅 0 は無視) |
  | `concat([幅 0])` | 幅 0 の値 |
  | `concat([64'h0123_4567_89ab_cdef, 1'b1])` | 65 ビット: bit 0 = 1、bit 64..1 = `0123…cdef` |
  | `concat([1'b1, 64'h0])` | 65 ビット: bit 64 だけ 1 |
  | `concat` に `1'bz` を 129 個 | 129 ビット全 z |
  | `concat([63 ビット全 1, 3'bx0z])` | 66 ビット、下位 3 ビット `x0z` |
  | `replicate(2'b1z, 3)` | `6'b1z1z1z` |
  | `replicate(x, 0)` | 幅 0 (§11.4.12.1) |
  | `replicate(1'b1, 129)` | 129 ビット全 1 |
  | `replicate(65 ビットの値, 2)` | 130 ビット、上下が同じ |
- 2 値版も同様 (z の行を除く)。
- 根拠: §11.4.12, §11.4.12.1、plan §3.2 (幅 0)、§3.5(e)。
- 実行: `--test golden_struct` / 自動。

#### M0-TC33 選択とインデックス (§11.5.1)
- 入力 `src = 8'b1010_x01z` (b7..b0 = 1,0,1,0,x,0,1,z)。オフセットは LSB=0 の物理オフセット。
  | 呼び出し | 期待 |
  |---|---|
  | `get_slice(幅4, src, Value(2))` | `4'b10x0` |
  | `get_slice(幅4, src, Value(6))` | `4'bxx10` (範囲外は x) |
  | `get_slice(幅4, src, Value(-2))` | `4'b1zxx` |
  | `get_slice(幅4, src, Value(100))` / `Value(i64::MAX)` / `Value(i64::MIN)` | `4'bxxxx` (オーバフローしない) |
  | `get_slice(幅4, src, Unknown)` | `4'bxxxx` |
  | `get_bit(src, Value(0))` / `Value(8)` / `Unknown` | z / x / x |
  | 2 値 `get_slice2(幅4, 8'b1010_0011, Value(6))` | `4'b0010` (範囲外は 0) |
  | 2 値 `get_slice2(幅4, 8'b1010_0011, Unknown)` | `4'b0000` (例外規則、決定 R-2) |
  | `set_slice(8'h00, Value(6), 4'b1111)` | `8'b1100_0000` (範囲外は書かない) |
  | `set_slice(8'h00, Value(-2), 4'b1011)` | `8'b0000_0010` |
  | `set_slice(8'h00, Unknown, 4'b1111)` | `8'h00` (何も書かない) |
  | `set_slice(8'h00, Value(i64::MAX), 4'b1111)` | `8'h00` |
  | 129 ビットで `Value(126)` 幅 4 の get/set | bit 126..128 が範囲内、bit 129 は範囲外 |
  | `index_value(4'b0101, U)` / `(4'b1011, S)` / `(4'b1011, U)` | `Value(5)` / `Value(-5)` / `Value(11)` |
  | `index_value(4'b01x0, *)` / `(4'b01z0, *)` | `Unknown` |
  | `index_value(64 ビット 2^63, U)` | `Value(i64::MAX)` (飽和) |
  | `index_value(128 ビット 2^100, U)` / `(128 ビット −2^100, S)` | `Value(i64::MAX)` / `Value(i64::MIN)` (飽和) |
- 根拠: §11.5.1、plan §3.5(e)、決定 R-2 例外。
- 実行: `--test golden_struct` / 自動。

#### M0-TC34 混在カーネル (§3.5.1、決定 R-2)
- 入力: `bit [7:0] b = 8'ha5` (`BitRef`)。
  | 呼び出し | 期待 |
  |---|---|
  | `shl_bits(b, 3'b001)` | 4 値 `8'h4a` (bval 0) |
  | `shl_bits(b, 3'bx01)` / `shr_bits(b, 3'bz00)` | `8'hxx` |
  | `ashr_bits(b, 3'b010, S)` / `U` | `8'he9` / `8'h29` |
  | `ashr_bits(b, 3'bxxx, S)` | `8'hxx` |
  | `pow_bits(base=8'h03 U, exp=4'b0010 U)` | `8'h09` |
  | `pow_bits(base=8'h03, exp=4'b00x0)` | `8'hxx` |
  | `pow_bits(base=8'h00 S, exp=4'b1111 S)` | `8'hxx` (4 値結果なので x。2 値変数への代入で 0 になるのは呼び出し側) |
  | `cond_merge_bits(8'b1100_1010, 8'b1010_1010)` | `8'b1xx0_1010` |
- CR-6: 各混在カーネルが `from_two_state` + 4 値カーネルと一致 (D、幅 1〜1024)。
- 例外規則の確認: `get_slice2`/`set_slice2` は 2 値の結果 (TC33)。
- 暫定: これらを `$display("%b")` で表示した結果が VCS で x になるか **[V-9]**。M0 のテストは計画の規則どおりの期待値で書く。
- 根拠: plan §3.5.1。
- 実行: `--test diff_struct --test diff_arith --test golden_struct` / 自動。

#### M0-TC35 構造演算の差分テスト
- 対象: `resize` (S/U、拡大・縮小・同幅)、`to_two_state`、`from_two_state`、`concat` (要素数 0〜8、幅 0 を含む)、`replicate` (n = 0〜5)、`get_slice/set_slice/get_bit/set_bit` (オフセット戦略: 負、0、境界、完全に範囲外、`i64` の端、`Unknown`)、`index_value`。
- 期待: CR-1〜CR-7。
- 実行: `--test diff_struct` / 自動。

#### M0-TC36 `$clog2` (§20.8.1)
- 引数は符号なしとして扱う。
  | 入力 | 期待 |
  |---|---|
  | 0 | `Some(0)` **[V-10: $clog2(0)]** |
  | 1 | `Some(0)` |
  | 2 | `Some(1)` |
  | 3 | `Some(2)` |
  | 4 / 5 | `Some(2)` / `Some(3)` |
  | 2^k − 1、2^k、2^k + 1 (k = 2〜62) | k、k、k+1 |
  | 64 ビット: 2^63、2^63 + 1、2^64 − 1 | 63、64、64 |
  | 65 ビット: 2^64、2^64 + 1 | 64、65 |
  | 128 ビット: 2^100 − 1、2^100、2^100 + 1、2^128 − 1 | 100、100、101、128 |
  | 8 ビット符号付き型の `8'h80` | 7 (符号なしの 128 として扱う) |
  | 幅 1: `1'b1` | 0 |
  | `4'b01x0`、`4'bz000` | `None` **[V-10]** |
  | 2^20 ビットで MSB だけ 1 | 2^20 − 1 |
- 2 値版があればそれも (X 行を除く)。差分 (D): 幅 1〜1024 の乱数で参照モデルと一致。
- 根拠: §20.8.1 (未検証)、plan §3.5(e)。
- 実行: `--test golden_struct --test diff_struct` / 自動。

#### M0-TC37 基数付きリテラル 2/8/16 進 (§5.7.1)
  | `parse_based(size, signed, radix, digits)` | value | フラグ |
  |---|---|---|
  | `(8, U, Bin, "1x")` | `8'b0000_001x` (最左が 1 なので 0 で埋める) | truncated=false |
  | `(8, U, Bin, "x1")` | `8'bxxxx_xxx1` (最左が x → x で埋める) | |
  | `(8, U, Bin, "z")` | `8'bzzzz_zzzz` | |
  | `(8, U, Hex, "x")` | `8'hxx` | |
  | `(8, U, Hex, "?")` | `8'hzz` (`?` は z) | |
  | `(12, U, Hex, "x1")` | `12'bxxxx_xxxx_0001` | |
  | `(8, U, Hex, "XZ")`、`(8, U, Hex, "xz")` | `8'bxxxx_zzzz` (大文字小文字を区別しない) | |
  | `(8, U, Hex, "AB")` / `"ab"` | `8'hab` | |
  | `(8, U, Bin, "1_0_1")` | `8'b0000_0101` | |
  | `(4, U, Bin, "1_0101")` | `4'b0101` | truncated=true (1 が落ちた) |
  | `(4, U, Hex, "fff")` / `"ff"` | `4'hf` | truncated=true |
  | `(3, U, Oct, "7")` | `3'b111` | |
  | `(4, U, Oct, "17")` | `4'b1111` | truncated=**false** (落ちるのは 0 の 2 ビットだけ。Q-3) **[V-18 暫定]** |
  | `(8, U, Hex, "0ff")` | `8'hff` | truncated=**false** **[V-18 暫定]** |
  | `(4, U, Bin, "0000_1111")` | `4'b1111` | truncated=false **[V-18]** |
  | `(4, U, Hex, "xx")` / `"z0"` | `4'bxxxx` / `4'b0000` | truncated=true (x/z が落ちた。Q-3) **[V-18]** |
  | `(4, U, Hex, "1f")` | `4'hf` | truncated=true **[V-18]** |
  | `(2, U, Bin, "??")` | `2'bzz` | |
  | `(65, U, Hex, "1_0000_0000_0000_0000")` | bit 64 だけ 1 | |
  | `(4, S, Bin, "1")` | `4'b0001`、signed=true (リテラル段階では 0 で埋める) | |
  | `(4, S, Bin, "1111")` | `4'b1111`、signed=true | |
  | `(None, U, Hex, "x")` | 幅 32 全 x、`xz_extend=true`、`sized=false` | |
  | `(None, U, Bin, "1")` | 幅 32 の 1、`xz_extend=false` | |
  | `(None, U, Hex, "1_0000_0000")` | 幅 33 (bit 32 = 1)、`widened_unsized=true` **[V-11 暫定]** | |
  | `(Some(0), ...)` | `Err` | |
  | `(8, U, Bin, "102")`、`(8, U, Oct, "8")`、`(8, U, Hex, "g")` | `Err` (基数に無い数字) | |
- 根拠: §5.7.1、plan §3.5(g)、R-11、決定 Q-3。
- 実行: `--test golden_literal` / 自動。

#### M0-TC38 10 進リテラル・サイズなし整数
  | 呼び出し | 期待 |
  |---|---|
  | `parse_based(4, U, Dec, "20")` | `4'd4`、truncated=true (値で切り詰め) |
  | `parse_based(8, U, Dec, "0255")` | `8'd255`、truncated=false |
  | `parse_based(8, U, Dec, "256")` | `8'd0`、truncated=true |
  | `parse_based(8, U, Dec, "x")` / `"x_"` / `"?"` / `"z"` / `"X"` | `8'hxx` / `8'hxx` / `8'hzz` / `8'hzz` / `8'hxx` |
  | `parse_based(8, U, Dec, "1x")`、`"xx"` | `Err` (x/z は 1 桁だけ。R-11) |
  | `parse_based(64, U, Dec, "18446744073709551615")` | 64 ビット全 1 |
  | `parse_based(65, U, Dec, "36893488147419103231")` | 65 ビット全 1 |
  | `parse_based(64, U, Dec, "18446744073709551616")` | 0、truncated=true |
  | `parse_based(128, U, Dec, "340282366920938463463374607431768211455")` | 128 ビット全 1 |
  | `parse_based(8, U, Dec, "1_000")` | 1000 mod 256 = 232、truncated=true |
  | `parse_unsized_decimal("123")` | 幅 32、123、signed=true |
  | `parse_unsized_decimal("2147483648")` | 幅 32、`32'h8000_0000`、signed=true (32 ビットに収まる) |
  | `parse_unsized_decimal("4294967296")` | 幅 33 (bit 32 = 1)、`widened_unsized=true` **[V-11 暫定]** |
  | `parse_unsized_decimal("0")` | 幅 32 の 0 |
- 根拠: §5.7.1、plan §3.5(g)。
- 実行: `--test golden_literal` / 自動。

#### M0-TC39 リテラルの差分テスト
- 入力 (D): 乱数の桁列 (各基数の数字、`_`、`x X z Z ?`、10 進は 1 桁の x/z/?)、size は None / 1〜1024、signed の真偽。
- 期待: 参照モデルの `parse_based` / `parse_unsized_decimal` と、Ok/Err の別、value、全フラグが一致。
- 性質 (P): X/Z なしの値 v (幅 w) を `%0b`/`%0h`/`%0o`/`%0d` で書式化した文字列を `parse_based(Some(w), …)` に戻すと v (TC45〜48 と組み合わせた往復)。
- 実行: `--test diff_literal_string` / 自動。

#### M0-TC40 文字列 ⇔ 整数 (§5.9, §11.10.3, §6.16)
  | 呼び出し | 期待 |
  |---|---|
  | `from_string_bytes(幅16, "AB")` | `16'h4142` |
  | `from_string_bytes(幅24, "AB")` | `24'h00_4142` (左を 0 で埋める) |
  | `from_string_bytes(幅8, "AB")` | `8'h42` (先頭側の文字を切り詰める) |
  | `from_string_bytes(幅20, "AB")` | `20'h0_4142` |
  | `from_string_bytes(幅12, "AB")` | `12'h142` |
  | `from_string_bytes(幅8, "")` | `8'h00` (`"\0"` として扱う) |
  | `from_string_bytes(幅16, "\x00\xff")` | `16'h00ff` (任意のバイト) |
  | `to_string_bytes(16'h4142)` | `b"AB"` |
  | `to_string_bytes(12'h041)` | `b"A"` (左を 0 で 16 ビットに埋め、0 のバイトは捨てる) |
  | `to_string_bytes(24'h41_0042)` | `b"AB"` (途中の 0 のバイトも捨てる) |
  | `to_string_bytes(8'h00)` | `b""` |
  | `to_string_bytes(16'b0100_0001_xxxx_xxxx)` | `b"A"` (x を 0 として扱い、0 のバイトになって捨てる) **[V-5 暫定]** |
  | `to_string_bytes(16'b0100_0001_0100_00z1)` | `b"AA"` (`z` を 0 として `0x41`) **[V-5 暫定]** |
- 性質 (P): 0 を含まないバイト列 s (長さ n) で、`to_string_bytes(from_string_bytes(幅 8n, s)) == s`。
- 実行: `--test golden_literal --test diff_literal_string` / 自動。

#### M0-TC41 `atoi` 系 (§6.16.9)
- 結果は 32 ビット。数字と `_` を先頭から読み、他の文字か終端で止まる。符号・空白・接頭辞は解釈しない **[V-6 暫定]**。
  | 関数 | 入力 | 期待 |
  |---|---|---|
  | atoi | `"123"` / `"12_3"` / `"12a3"` | 123 / 123 / 12 |
  | atoi | `""` / `"abc"` | 0 / 0 |
  | atoi | `"-12"` / `"+5"` / `" 12"` | 0 / 0 / 0 **[V-6 暫定。Verilator は -12 / 5 / 12]** |
  | atoi | `"_12"` / `"_"` / `"__"` / `"1__2"` | 12 / 0 / 0 / 12 (`_` を読み飛ばす。数字が無ければ 0。Q-4) **[V-6 暫定]** |
  | atoi | `"4294967295"` / `"4294967296"` / `"4294967297"` | `32'hffff_ffff` / 0 / 1 (超えた分は切り詰め) **[V-6 暫定]** |
  | atohex | `"ff"` / `"FF"` / `"1g"` | 255 / 255 / 1 |
  | atohex | `"0xff"` | 0 (`0` を読み、`x` で止まる) **[V-6 暫定]** |
  | atohex | `"1_0000_0001"` | 1 (33 ビット目を切り詰め) |
  | atooct | `"17"` / `"18"` | 15 / 1 |
  | atobin | `"1012"` / `"2"` | 5 / 0 |
- 性質 (P): 0〜2^32−1 の n で、`atoi(itoa(n))` = n (非負)、`atohex(hextoa(n))` = n、`atooct/atobin` も同様。
- 実行: `--test golden_literal --test diff_literal_string` / 自動。

#### M0-TC42 整数 → 実数 (§6.12.1, §20.5)
  | 呼び出し | 期待 |
  |---|---|
  | `to_f64(32'hffff_ffff, S)` / `U` | −1.0 / 4294967295.0 |
  | `to_f64(64 ビット全 1, U)` | 18446744073709551616.0 (= 2^64、最近接偶数) |
  | `to_f64(2^53 + 1, U)` | 2^53 (同距離 → 仮数が偶数) |
  | `to_f64(2^53 + 3, U)` | 2^53 + 4 |
  | `to_f64(128 ビット 2^100, U)` | 2^100 (正確) |
  | `to_f64(1024 ビット全 1, U)` | +∞ (2^1024 に丸まり、IEEE 754 の RNE でオーバフロー。最大値に飽和させない。Q-5) |
  | `to_f64(1025 ビット: 2^1024 のビットだけ 1, S)` | −∞ (−2^1024。Q-5) |
  | `to_f64(1024 ビット: 2^1024 − 2^971, U)` | `f64::MAX` (正確に表せる) |
  | `to_f64(1024 ビット: 2^1024 − 2^970, U)` | +∞ (f64::MAX と 2^1024 のちょうど中間 → 仮数が偶数の 2^1024 → 溢れ。Q-5) |
  | `to_f64(1024 ビット: 2^1024 − 2^970 − 1, U)` | `f64::MAX` |
  | `to_f64(1024 ビット全 1, S)` | −1.0 |
  | `to_f64(8'b1x01, U)` / `to_f64(4'b1x01, S)` | 9.0 / −7.0 (x を 0 として扱う) **[V-14 暫定]** |
  | `to_f64(8'bzzzz_zzzz, U)` | 0.0 **[V-14]** |
  | `to_f32(2^24 + 1, U)` | 16777216.0 |
  | `to_f32(2^54 + 2^30 + 1, U)` | 2^54 + 2^31 = 18014400656965632.0 (**二重丸めなら 2^54 になる**。R-12) |
  | `to_f32(64'h8000_0000_0000_0000, S)` | −2^63 |
  | `to_f32(128 ビット: 2^128 − 2^104, U)` | `f32::MAX` |
  | `to_f32(128 ビット: 2^128 − 2^104 + 2^103 − 1, U)` | `f32::MAX` |
  | `to_f32(128 ビット: 2^128 − 2^104 + 2^103, U)` | +∞ (中間 → 偶数側の 2^128 → 溢れ。Q-5) |
  | `to_f32(129 ビット: 2^128, U)` | +∞ |
- 根拠: §6.12.1、§20.5、plan §3.5(i)、R-12。
- 実行: `--test diff_real_fmt` の golden 部 / 自動。

#### M0-TC43 実数 → 整数と bits 変換 (§6.12.1, §20.5)
- `RealToIntStatus` は 4 種 (Q-6): `Exact` (整数で dst に収まった)、`Rounded` (小数部を丸めたが収まった)、`Wrapped` (丸めた後の整数が dst の符号付きの範囲にも符号なしの範囲にも収まらず、2 の補数で切り詰めた。丸めの有無によらない)、`NotFinite` (NaN/±∞、0 を返す)。幅 w の dst に「収まる」とは、整数 n が −2^(w−1) ≤ n ≤ 2^w − 1 であること。
  | 呼び出し (dst の幅) | 値 | 状態 |
  |---|---|---|
  | `from_f64_round(32, 35.5)` / `35.4` / `36.0` | 36 / 35 / 36 | Rounded / Rounded / Exact |
  | `from_f64_round(32, -1.5)` / `-2.5` / `2.5` / `0.5` / `-0.5` | −2 / −3 / 3 / 1 / −1 (0 から遠い方へ。偶数丸めではない) | Rounded |
  | `from_f64_round(32, 0.49999999999999994)` | 0 (`floor(x+0.5)` だと 1 になる落とし穴) | Rounded |
  | `from_f64_round(32, 1.4999999999999998)` | 1 | Rounded |
  | `from_f64_round(32, -0.0)` / `5e-324` | 0 / 0 | Exact / Rounded |
  | `from_f64_round(64, 4503599627370495.5)` | 4503599627370496 | Rounded |
  | `from_f64_round(8, -1.0)` | `8'hff` | Exact (符号付きの範囲に収まる) |
  | `from_f64_round(8, 255.0)` / `-128.0` | `8'hff` / `8'h80` | Exact (符号なし / 符号付きの範囲) |
  | `from_f64_round(8, 254.6)` | `8'hff` | Rounded |
  | `from_f64_round(8, 255.5)` | `8'h00` (256 を切り詰め) | Wrapped |
  | `from_f64_round(8, 300.0)` / `-129.0` | 44 / `8'h7f` (2 の補数で切り詰め) | Wrapped / Wrapped |
  | `from_f64_round(64, 1e20)` | 7766279631452241920 (1e20 mod 2^64) | Wrapped |
  | `from_f64_round(128, 1e20)` | 100000000000000000000 (正確) | Exact |
  | `from_f64_round(128, 2^100)` / `-2^100` | 2^100 / 2^128 − 2^100 | Exact / Exact |
  | `from_f64_round(1100, 1e300)` | 1e300 の正確な整数値 (997 ビット) | Exact |
  | `from_f64_round(32, NaN)` / `+∞` / `−∞` | 0 | NotFinite **[V-4 暫定]** |
  | `from_f64_trunc(32, 35.9)` / `-35.9` / `-0.5` / `7.0` | 35 / −35 / 0 / 7 | Rounded / Rounded / Rounded / Exact (小数部を捨てたものは Rounded) |
  | `from_f64_trunc(8, 256.7)` | `8'h00` | Wrapped |
  | `from_f64_trunc(32, NaN)` | 0 | NotFinite **[V-4 暫定]** |
  | `$realtobits(1.0)` / `$bitstoreal(64'h3ff0_0000_0000_0000)` | `64'h3ff0_0000_0000_0000` / 1.0 |
  | `$shortrealtobits(1.0)` / 逆 | `32'h3f80_0000` / 1.0 |
  | `$realtobits(-0.0)` / NaN のビットパターン | `64'h8000_0000_0000_0000` / 入力の NaN のビットをそのまま保つ |
- 注意 (STATUS の決定): 参照モデルの状態は現在 2 種なので、T8 の着手時に参照モデルも 4 種に合わせる。それまで TC44 の状態の比較は保留し、上の固定値 (TC43) でだけ検査する。
- 実行: `--test diff_real_fmt` / 自動。

#### M0-TC44 実数の差分テスト
- 入力 (D): 幅 1〜1024 の整数 (S/U、X/Z を少し混ぜる) と、f64 のビットパターンの一様乱数 (非正規化数、±0、±∞、NaN、2 の冪 ± 0.5 を多めに)。
- 期待: `to_f64 to_f32 from_f64_round from_f64_trunc` が参照モデルとビット単位で一致 (浮動小数は `to_bits` で比較。NaN は「どちらも NaN」)。`RealToIntStatus` も一致 (参照モデルが 4 種に揃った後。TC43 の注意)。
- 性質 (P、状態): 返した状態が定義と矛盾しない。`Exact` ⇔ v が整数で範囲内、`Wrapped` ⇔ 丸めた整数が範囲外。
- 性質 (P): 有限の f64 v と十分広い dst (1100 ビット) で、`to_f64(from_f64_trunc(v), S) == v.trunc()`。|n| ≤ 2^53 の整数 n で `to_f64(n) as i64 == n`。
- 実行: `--test diff_real_fmt` / 自動。

#### M0-TC45 表示 `%b %o %h` (§21.2.1.2〜§21.2.1.4、plan §3.9 規則 1, 3)
- 桁の規則: 全ビット x → `x`、全ビット z → `z`、一部 x → `X`、x が無く一部 z → `Z`、既知 → 数字 (16 進は小文字)。最上位の端数桁は存在するビットだけで判定。自動幅は 0 埋め (`%b` = w 文字、`%o` = ⌈w/3⌉、`%h` = ⌈w/4⌉)。
  | 書式 | 値 | 表示 |
  |---|---|---|
  | `%b` | `4'b10xz` | `10xz` |
  | `%b` | `8'h05` | `00000101` |
  | `%h` | `8'bxxxx_01z0` | `xZ` |
  | `%h` | `8'bzzzz_xxxx` | `zx` |
  | `%h` | `8'b0x01_0000` | `X0` |
  | `%h` | `4'bxz01` | `X` (x と z が混在 → X 優先) **[V-13 暫定]** |
  | `%h` | `5'bx_0000` | `x0` (端数桁は 1 ビットだけで全 x) |
  | `%h` | `5'bz_1111` | `zf` |
  | `%h` | `8'hab` / `8'h0f` / `8'h00` | `ab` / `0f` / `00` |
  | `%h` | `8'hxx` | `xx` |
  | `%h` | `1'bz` | `z` |
  | `%h` | 65 ビットで bit 64 だけ 1 | `10000000000000000` (17 文字) |
  | `%o` | `8'o377` / `9'h1ff` | `377` / `777` |
  | `%o` | `4'b1x01` | `1X` |
  | `%o` | `4'bz000` | `z0` |
  | `%o` | `3'bxz1` | `X` **[V-13 暫定]** |
  | `%o` | 64 ビット全 1 | `1777777777777777777777` (22 文字) |
  | `%o` | 64 ビット全 x | `xxxxxxxxxxxxxxxxxxxxxx` (22 文字。最上位の端数桁 1 ビットも x) |
- 自動幅の表 (`%b`/`%o`/`%h` の文字数): w = 1 → 1/1/1、8 → 8/3/2、31 → 31/11/8、32 → 32/11/8、33 → 33/11/9、63 → 63/21/16、64 → 64/22/16、65 → 65/22/17、127 → 127/43/32、128 → 128/43/32、129 → 129/43/33、2^20 → 2^20 / 349526 / 262144。
- 2 値 (`format_int2`) も同じ表示 (X/Z の行を除く)。
- 実行: `--test golden_fmt` / 自動。

#### M0-TC46 表示 `%d` と自動幅 (plan §3.9 規則 2, 3)
- 値全体で判定: 全 x → `x`、全 z → `z`、一部 x → `X`、x が無く一部 z → `Z`。自動幅は空白で右詰め。文字数は U: `2^w−1` の桁数、S: `−2^(w−1)` の文字数 (符号込み)。X/Z の 1 文字も同じ幅で右詰め。
  | 値 | signed | 表示 (`␣` は空白) |
  |---|---|---|
  | `8'bx` (全 x) | U | `␣␣x` |
  | `8'b0000_000x` | U | `␣␣X` |
  | `8'bzzzz_zzzz` | U | `␣␣z` |
  | `8'b0000_000z` | U | `␣␣Z` |
  | `8'bz000_000x` | U | `␣␣X` (X 優先) |
  | `8'hff` | U / S | `255` / `␣␣-1` |
  | `8'h80` | S | `-128` |
  | `8'h00` | S | `␣␣␣0` |
  | 32 ビットの 1 | S | `␣␣␣␣␣␣␣␣␣␣1` (11 文字) |
  | 32 ビットの 1 | U | `␣␣␣␣␣␣␣␣␣1` (10 文字) |
  | `1'b1` | U / S | `1` / `-1` |
  | `1'b0` | S | `␣0` |
  | `4'b1000` | S | `-8` |
  | 64 ビット全 1 | U | `18446744073709551615` |
  | 64 ビット `2^63` | S | `-9223372036854775808` |
  | 128 ビット全 1 | U | `340282366920938463463374607431768211455` |
  | 128 ビット `2^127` | S | `-170141183460469231731687303715884105728` |
  | 129 ビット全 x | S | `␣` × 39 + `x` (40 文字) |
- 自動幅の文字数表 (U / S): w = 1 → 1/2、2 → 1/2、3 → 1/2、4 → 2/2、8 → 3/4、16 → 5/6、31 → 10/11、32 → 10/11、33 → 10/11、63 → 19/20、64 → 20/20、65 → 20/21、127 → 39/39、128 → 39/40、129 → 39/40。
- 実行: `--test golden_fmt` / 自動。

#### M0-TC47 フィールド幅指定 (`%0d` 等、`%Nd`、`%0Nd`) (plan §3.9 規則 4, 5) **[V-7 暫定]**
  | `FieldWidth` | 書式 | 値 | 表示 |
  |---|---|---|---|
  | `Min{0}` | `%0d` | −5 (S) | `-5` |
  | `Min{0}` | `%0d` | 0 | `0` |
  | `Min{0}` | `%0d` | `8'bx` / `8'b0000_000z` | `x` / `Z` |
  | `Min{0}` | `%0h` | `16'h00ab` / `8'h00` | `ab` / `0` |
  | `Min{0}` | `%0h` | `12'bxxxx_0000_0001` | `x01` (x の桁は 0 ではないので止まる) |
  | `Min{0}` | `%0h` | `12'b0000_xxxx_0001` | `x1` |
  | `Min{0}` | `%0b` / `%0o` | `8'h05` / 0 | `101` / `0` |
  | `Min{5, false}` | `%5d` | 42 / −5 (S) | `␣␣␣42` / `␣␣␣-5` |
  | `Min{5, true}` | `%05d` | 42 | `00042` |
  | `Min{5, true}` | `%05d` | −5 (S) | `-0005` (符号を先頭に置き、その後を 0 で埋める。Q-7) |
  | `Min{5, false}` | `%5d` | `8'bx` | `␣␣␣␣x` |
  | `Min{5, true}` | `%05d` | `8'bx` | `0000x` (Q-7) |
  | `Min{5, *}` | `%5h` | `8'hab` | `000ab` (`%h` は常に 0 埋め) |
  | `Min{5, *}` | `%5h` | `8'hxx` | `000xx` (Q-7) |
  | `Min{2, false}` | `%2d` | 12345 | `12345` (切り詰めない) |
  | `Min{3, *}` | `%3h` | `16'h1234` | `1234` |
  | `Min{1, *}` | `%1h` | `8'h0a` | `a` (最小表示を元に N 文字まで埋める。自動幅の `0a` には戻さない。Q-8) |
  | `Min{2, *}` | `%2h` | `16'h00ab` | `ab` (Q-8) |
  | `Min{1, false}` | `%1d` | `8'd5` | `5` (自動幅なら `␣␣5`。Q-8) |
  | `Min{3, false}` | `%3d` | `8'd5` | `␣␣5` |
  | `Min{4, *}` | `%4b` | `8'h05` | `0101` (Q-8) |
- 実行: `--test golden_fmt` / 自動。

#### M0-TC48 `itoa` 系 (§6.16.11〜§6.16.14)
  | 呼び出し | 期待 |
  |---|---|
  | `itoa(32'sd-5, S, Dec)` | `b"-5"` |
  | `itoa(0, *, Dec)` / `Hex` / `Oct` / `Bin` | `b"0"` |
  | `itoa(32'd255, U, Hex)` (hextoa) | `b"ff"` |
  | `itoa(32'd8, U, Oct)` (octtoa) | `b"10"` |
  | `itoa(32'd5, U, Bin)` (bintoa) | `b"101"` |
  | `itoa(32'hffff_ffff, S, Dec)` / `U` | `b"-1"` / `b"4294967295"` |
- 性質 (P): 任意の値で `itoa(x, s, r)` == `format_int(x, r, s, Min{0,false})` のバイト列。
- 実行: `--test golden_fmt --test diff_real_fmt` / 自動。

#### M0-TC49 表示の差分テスト
- 入力 (D): 幅 1〜1024、X/Z 密度の全種類 (桁の中で x と z が混ざるものを多く)、4 種の基数 × S/U × `FieldWidth` (`Auto`、`Min{0}`、`Min{n, zp}` n ∈ {1..40})。
- 期待: `format_int` が参照モデルの `format_int` と文字列で一致。`format_int2` は X/Z なしの入力で一致。
- 性質 (P): `Auto` の文字数は値に依らず幅と基数と符号だけで決まる。
- 実行: `--test diff_real_fmt` / 自動。

#### M0-TC50 DPI 形式との変換 (L-7、Annex I `svdpi.h`)
- 配置: `svLogicVecVal` は u32 の `[aval0, bval0, aval1, bval1, …]`、要素数 `(w+31)>>5`。符号化 0=(0,0)、1=(1,0)、z=(0,1)、x=(1,1)。
  | 値 | `write_sv_logic_vec_val` の出力 (u32) |
  |---|---|
  | `1'bx` | `[1, 1]` |
  | `1'bz` | `[0, 1]` |
  | `32'h89ab_cdef` | `[0x89abcdef, 0]` |
  | 40 ビット: bit 39..32 = `x0z1_0000`、下位 = `32'h89ab_cdef` | `[0x89abcdef, 0x0, 0x90, 0xa0]` |
  | 64 ビット: bit 63 だけ z | `[0, 0, 0, 0x80000000]` |
  | 64 ビット全 1 | `[0xffffffff, 0, 0xffffffff, 0]` |
  | 65 ビット: bit 64 だけ 1 | `[0, 0, 0, 0, 1, 0]` |
  | 129 ビット全 x | `[0xffffffff, 0xffffffff] × 4` の後に `[1, 1]` (計 10 要素) |
- マスク (`read_sv_logic_vec_val`): 
  - 幅 33、入力 `[0xffffffff, 0, 0xffffffff, 0xfffffffe]` → 下位 32 ビット全 1、bit 32 = 1 (a=1, b=0)。結果が正規化されている (上位を読み捨て)。
  - 幅 1、入力 `[0xfffffffe, 0xffffffff]` → `1'bz`。
  - 幅 64、入力の全要素に `0xffffffff` → 64 ビット全 x (未使用ビットは無い)。
- `svBitVecVal`: `write_sv_bit_vec_val(33'h1_1234_5678)` → `[0x12345678, 0x1]`。`read_sv_bit_vec_val(幅33, [0x12345678, 0xffffffff])` → `33'h1_1234_5678`。
- 出力配列の長さ: 4 値は `2*((w+31)>>5)`、2 値は `(w+31)>>5` (w = 1, 31, 32, 33, 63, 64, 65, 127, 128, 129)。
- 根拠: Annex I (svdpi.h、plan-review で照合済み)、plan §3.8。
- 実行: `--test diff_core` (golden 部) / 自動。

#### M0-TC51 DPI 変換の往復とゼロコピー同値性
- 性質 (P, D):
  1. 任意の 4 値 v (幅 1〜1024): `read(write(v)) == v`、2 値も同様。
  2. 任意の u32 配列 (未使用ビットに乱数を入れたもの) a: `write(read(a))` は a の未使用ビットを 0 にしたものと一致。`read(a)` は CR-2 を満たす。
  3. 任意の 2 値 `BitVec` (正規化済み) で、aval の u64 ワード列をリトルエンディアンのバイト列にした先頭 `4*((w+31)>>5)` バイトが、`write_sv_bit_vec_val` の出力 (LE バイト列) と一致 (input のゼロコピーの根拠。plan §3.2)。
  4. `write_sv_logic_vec_val` の要素 `2i` / `2i+1` が、参照モデルの「ビット 32i..32i+31 の aval / bval」と一致。
- 実行: `--test diff_core` / 自動。

#### M0-TC52 幅境界の横断テスト (L-5)
- 目的: L-5。乱数に頼らず、決まった幅で全演算を通す。
- 入力: 幅 **1, 2, 31, 32, 33, 63, 64, 65, 127, 128, 129, 1000, 1023, 1024, 1025, 4096** の各幅で、値のパターン {全 0、全 1、`1`、MSB だけ 1、MSB 以外全 1、交互 `0101…`、bit 63/64 の両側だけ 1、全 x、全 z、MSB だけ x、LSB だけ z}。
- 対象: §5.2.0 の全演算 (2 項は同じ幅の全パターン組、シフト量は {0, 1, 63, 64, 65, w−1, w, w+1, 2^64})。
- 期待: 参照モデルと一致 (CR-1〜CR-7)。幅 0 は TC32 の連結・複製だけ。
- 実行: 各 `diff_*.rs` に置く / 自動。テスト実行エージェントは、上の幅のうち 63, 64, 65, 127, 128, 129 と 1000 超 (1025, 4096) がテストに現れることを確認する。

#### M0-TC53 上限近くの幅のスモークテスト (2^20、R-27)
- 入力: 幅 2^20 = 1048576。
  | 操作 | 期待 |
  |---|---|
  | 全 1 + 1 (`add`) | 全 0 |
  | `1 << (2^20 − 1)` | MSB だけ 1 |
  | `concat([2^19 ビット全 1, 2^19 ビット全 0])` | 上半分が全 1、下半分が全 0 |
  | `%h` (`Auto`) で MSB だけ 1 | `8` の後に `0` が 262143 個 (計 262144 文字) |
  | `%h` で全 x | `x` が 262144 個 |
- 追加 (任意): `LogicVec::zero(MAX_WIDTH)` (2^24) が作れて、`words_for` どおりの長さ。
- 所要時間を記録する (合否には使わない。性能の参考)。
- 実行: `cargo test -p svsim-logic --release -- --ignored` / 自動 (定期ジョブ `proptest-long`)。

#### M0-TC54 ヒープ確保 0 回 (L-6、plan §5.5)
- 方法: スレッドローカルなカウンタの `GlobalAlloc` (R-24)。計測区間の前後でカウンタを比べる。
- 対象と期待 (確保回数 = 0):
  1. 幅 1, 32, 63, 64 の `LogicVec` / `BitVec` の**全**演算 (構築 `zero filled from_u64 from_i64`、§5.2.0 の T4/T5/T6 の全メソッド、`to_u64`、`to_bit_vec`、`clone`)。結果の型が幅 ≤ 64 の場合に限る (幅を広げる `concat` / `replicate` で結果が 64 超になるものは除く)。
  2. 任意幅 (65, 129, 1024) のスライスカーネル (層 2) に、`Scratch::reserve_for(width)` で事前確保した `Scratch` を渡したもの: `mul div rem pow`、シフト、選択、連結、比較、リダクション、`clog2`。
  3. 表示 (`format_int`, `format_int2`): 呼び出し側が `out.reserve(format_len_bound(width, radix))` (`Min{n}` のときは `max(n, その値)`) と `Scratch::reserve_for(width)` を済ませていれば 0 回 (Q-9)。予約しない場合は対象外。
- 補助 (P): `format_len_bound(w, radix)` は、`Auto` と `Min{0}` の任意の値 (幅 1〜1024、S/U、X/Z を含む) の表示の長さ以上。`%d` の符号付きの最小値と全 x を必ず含める。
- **検査器の自己テスト**: 計測区間で意図的に `Vec::with_capacity(1)` を作ると 1 以上を数える。
- 実行: `cargo test -p svsim-logic --test no_alloc_core --test no_alloc_bitwise --test no_alloc_arith --test no_alloc_struct` / 自動。

#### M0-TC55 長時間 proptest (L-2)
- 手順: `PROPTEST_CASES=100000 cargo test -p svsim-logic --release`。
- 期待: 全部通る。失敗したら最小化された入力が `tests/<name>.proptest-regressions` に保存される (コミットはマネージャ)。実装に差し戻す。
- 追加確認: 失敗の再現ファイルは proptest の既定の置き場所 (テストファイルの隣の `tests/<name>.proptest-regressions`、STATUS の決定) にあり、git の管理下にある (plan §5.3)。
- 実行: 受け入れ時に 1 回手動起動、以後は CI の `proptest-long` / 自動。所要時間を test-report に記録する。

#### M0-TC56 proptest の入力分布 (plan §5.3)
- 目的: 戦略が境界を本当に生成しているか (差分テストの質の担保)。
- 手順: `tests/common/mod.rs` の戦略から、固定シードで 10,000 個を生成して集計するテスト。
- 期待:
  - 幅: 境界値の集合 {1,2,3,7,8,31,32,33,63,64,65,127,128,129,191,192,193,255,256,257} の各要素が少なくとも 1 回出る。`201..=1024` の値が全体の 10〜30%。
  - X/Z の密度: 「X/Z なし」「1 ビットだけ」「全部 X/Z」がそれぞれ 1 回以上。x と z の両方が出る。
  - ワードのパターン: 全 0、全 1、符号ビットだけ 1 が出る。
  - シフト量: 0、w、w+1、64 超の値、X を含む値が出る。オフセット: 負、`i64::MIN`、`i64::MAX`、`Unknown` が出る。
- 実行: `cargo test -p svsim-logic --test diff_core` に置く / 自動。

#### M0-TC57 panic の規約 (plan §3.3 規約 1, 3)
- 期待:
  1. API の誤用は **panic** し、`Err` は返さない (Q-1)。release ビルドでも panic する (`cargo test --release` で確認)。対象: 幅の揃っていない文脈決定の演算 (`add` に幅 8 と 9、`eq` に幅 64 と 65、`cond_merge` に幅違い)、`copy_from` の幅違い、`from_planar` の長さ違い、`bit`/`set_bit` の範囲外の i。panic のメッセージに関数名と、期待した値・実際の値 (例: 幅 8 と 9) が含まれる。
  2. 0 除算、x のシフト量、範囲外・`i64` の端の選択、巨大なシフト量、NaN の変換、不正なリテラル文字列は **panic しない** (値または `Err` で返る)。差分テストの中で panic が出たら失敗 (proptest は panic を失敗として扱う)。
- 実行: `--test diff_core` ほか / 自動。

#### M0-TC58 決定性: スレッドに依存しない (plan §3.10、PLAN §1 原則 2)
- 手順: 固定シードの入力 1,000 組で全演算を、(a) 1 スレッドで順に、(b) 8 スレッドで分けて (各スレッドが自分の `Scratch` を持つ) 実行し、結果の bits 文字列の列を比べる。
- 期待: 完全に一致。
- 実行: `--test diff_core` に置く / 自動。

#### M0-TC59 crate ドキュメント (L-8)
- 手順:
  1. TC2・TC3 が通る (`missing_docs` が効いている)。
  2. `cargo doc -p svsim-logic --no-deps` の crate のトップページ (または `src/lib.rs` の `//!`) に次がある (手動で読む + キーワードの grep):
     - 配置の契約: 4 値 `[a_0..a_{nw-1}, b_0..b_{nw-1}]`、2 値 `[a_0..]`、リトルエンディアン、正規化不変条件、**適用範囲は packed 値 1 個で unpacked 配列の要素は対象外**、幅 0 の扱い、`MAX_WIDTH`。
     - §3.6 の幅規則の表の全行 (サイズなし定数、サイズ付き定数、2 項算術・ビット、単項 `+ - ~`、比較・等価 (オペランドを max に拡張)、論理演算 (自己決定)、リダクションと `!` (自己決定)、シフトと `**` (右は自己決定)、`?:` (条件は自己決定)、連結、複製)、代入文脈、符号の規則、評価の手順、`$signed`/`$unsigned`。
     - §3.5.1 の混在規則 1〜5 (選択の例外を含む)。
  3. `svsim-common` の crate ドキュメントに、`Symbol` の番号順を出力の順序に使わない規則 (plan §4.3)。
- 期待: すべてある。
- 実行: 1 は自動、2・3 は手動 (M) + grep。

#### M0-TC60 参照モデルの独立性とゴールデンテスト (L-9)
- 手順:
  1. `docs/STATUS.md` の M0-T9 の担当が、T3a〜T8 と別のエージェント個体であること (現在の記載は「implementer (別個体)」)。マネージャのセッション記録で確認する。
  1b. 参照モデルの `RealToIntStatus` が 4 種 (Q-6) に揃っていること (STATUS の注意。T8 の着手時)。`RefVec::from_bits_str` が Q-22 の規則 (TC10) に従うこと。
  2. `svsim-logic-ref` の `Cargo.toml` に `svsim-logic` への依存が無い (`cargo tree -p svsim-logic-ref | grep -c svsim-logic$` が 0)。
  3. `cargo test -p svsim-logic-ref` が通り、その中に plan §5.4 のゴールデン (TC13〜TC21, TC24〜TC27, TC29, TC31〜TC33, TC36〜TC38, TC40〜TC43, TC45〜TC47 と同じ値) がある。**本計画の期待値表を参照モデルのゴールデンにも使ってよい** (仕様由来なので独立性を損なわない)。
  4. 参照モデルの公開関数が plan §5.1 の一覧 (`not` … `format_int`) を全部持つ。
- 実行: 2, 3 は自動、1, 4 は手動 (M)。

---

### 5.3 `svsim-common` (C 系)

#### M0-TC61 行・列の計算 (C-1)
- `line_col` は 1 始まり、列は UTF-8 の文字数 (不正な UTF-8 はバイト数)。
  | テキスト | offset | 期待 (line, col) |
  |---|---|---|
  | `""` (空) | 0 | (1, 1) |
  | `"a\nb"` | 0 / 1 / 2 / 3 (終端) | (1,1) / (1,2) / (2,1) / (2,2) |
  | `"a\r\nb"` | 1 (`\r`) / 2 (`\n`) / 3 | (1,2) / (1,3) / (2,1) |
  | `"ab\ncd"` (最終行に改行なし) | 4 / 5 (終端) | (2,2) / (2,3) |
  | `"ab\n"` | 3 (終端) | (2,1) |
  | `"aあb"` (あ = 3 バイト) | 4 (`b`) | (1,3) |
  | `"a\xffb"` | 2 (`b`) | (1,3) (不正バイトは 1 バイト = 1 列) |
  | `"\n\n\n"` | 2 | (3,1) |
  | `"x\ry"` (単独の CR) | 2 | (1,3) (CR だけでは改行としない。Q-10) |
  | `"ab\ncd"` (長さ 5) | 6 / 100 / `u32::MAX` (終端を超える) | (2,3) (ファイル末尾として扱う。panic しない。Q-11) |
  | `""` (空) | 7 | (1,1) (Q-11) |
- 実行: `cargo test -p svsim-common` / 自動。

#### M0-TC62 `add_file` / `line_text` / `snippet` (C-1)
- `add_file` を 3 回 → `FileId` は互いに異なり、`file(id)` で元のテキストが返る。
- `line_text` は行末の改行を含まない (Q-12):
  | テキスト | 行 | 期待 |
  |---|---|---|
  | `"ab\r\ncd"` | 1 / 2 | `b"ab"` (`\r` も除く) / `b"cd"` |
  | `"ab\ncd\n"` | 2 / 3 | `b"cd"` / `b""` (最後の改行の後の空の行) |
  | `"ab\r"` (改行なしの最終行が単独の CR で終わる) | 1 | `b"ab\r"` (単独の CR は残す。Q-10, Q-12) |
  | `"a\r\r\nb"` | 1 | `b"a\r"` (除くのは `\n` の直前の `\r` 1 つだけ) |
  | `""` | 1 | `b""` |
  | `"ab"` | 0 / 2 (範囲外の行) | panic |
- `snippet(Span{lo:1, hi:4})` on `"abcdef"` → `b"bcd"`。`lo == hi` → 空。
- 実行: `cargo test -p svsim-common` / 自動。

#### M0-TC63 診断の JSON 表示 (C-2)
- 入力: ファイル `a.sv` = `"module m;\n  wire w\nendmodule\n"`。
  - Diagnostic: severity=Error、code=`E0001`、message=`expected ';'`
  - 主ラベル: span 17..18 (2 行目の `w`)、message `here`、primary=true
  - 副ラベル: span 12..16 (`wire`)、message `declaration`、primary=false
  - 副ラベル (複数行): span 10..28 (2 行目の先頭〜3 行目の `endmodule` の終わり)、message `scope`
  - notes: [`note one`]
  - fixit: span 18..18、replacement `;`、message `insert ';'`
- 期待 (JSON の値として比較。キーの順序は問わない):
  ```json
  {"version":1,"severity":"error","code":"E0001","message":"expected ';'",
   "labels":[
     {"file":"a.sv","line":2,"col":8,"end_line":2,"end_col":9,"message":"here","primary":true},
     {"file":"a.sv","line":2,"col":3,"end_line":2,"end_col":7,"message":"declaration","primary":false},
     {"file":"a.sv","line":2,"col":1,"end_line":3,"end_col":10,"message":"scope","primary":false}],
   "notes":["note one"],
   "fixits":[{"file":"a.sv","line":2,"col":9,"end_line":2,"end_col":9,"replacement":";","message":"insert ';'"}]}
  ```
  - 規則 (決定 Q-13): `end_line`/`end_col` は `hi` (排他) の位置の `line_col`、`severity` は小文字 (`"error" "warning" "note" "help"`)、ラベルと fix-it は発行順。
- `code` が無い診断: `"code": null` (キーは省略しない)。`notes`/`fixits` が無ければ空の配列 `[]`。
- ファイル名は `FileOrigin` のパスの文字列 (`Disk` と `Include` のどちらでも)。別ファイルのラベルはそのファイルの名前と行・列になる。
- 決定性: 同じ入力で 2 回呼んで同じ JSON 文字列 (serde_json の出力)。JSON の文字列にしてから読み戻すと同じ値。
- 既存のゴールデン: `crates/svsim-common/tests/diag_render.rs` の `json_full` (主 + 副 2 + 別ファイル (CRLF の include) + 注記 + fix-it)、`json_multiline_span` (`code: null`、複数行) は上の規則と一致しており、C-2 の JSON 部分のゴールデンとして採用する。上の `a.sv` の例は、同じファイルに追加してよい独立な例。
- 実行: `cargo test -p svsim-common --test diag_render` / 自動。

#### M0-TC64 単一行の人間向け表示 (C-2)
- 書式はマネージャが承認した T10 の rustc 風の書式 (`crates/svsim-common/tests/diag_render.rs` の現在の出力) を仕様とする。そこから読み取れる規則:
  1. 1 行目: `<severity>[<code>]: <message>`。code が無ければ `<severity>: <message>`。色のエスケープシーケンスは含まない。
  2. 主ラベルの位置: `<G 個の空白>--> <file>:<line>:<col>`。G は表示する行番号の最大の桁数 (ガター幅)。
  3. 空のガター行 `<G 個の空白> |`。
  4. ソース行: `<行番号 (G 桁で右詰め)> | <行のテキスト>`。タブは空白 4 個に展開する。不正な UTF-8 は U+FFFD で表示する。
  5. ラベル行: `<G 個の空白> | <(col−1) 個の空白><印> <message>`。印は主ラベルが `^`、副ラベルが `-` で、span の文字数だけ並べる (空の span は 1 個)。message が空なら印の後ろに空白を付けない。同じ行のラベルは列の昇順に 1 行ずつ並べる。列は文字単位 (タブは 4 列)。
  6. 複数行の span は開始行だけを表示し、印は開始位置からその行の終わりまで。
  7. 別ファイルのラベルは `<G 個の空白>::: <file>:<line>:<col>` の行の後に、そのファイルのソース行とラベル行を置く。
  8. 注記があるときは、空のガター行のあとに `<G 個の空白> = note: <text>` を 1 つずつ。fix-it は表示しない。最後は改行で終わる。
- ゴールデン (既存。そのまま C-2 の人間向け表示のゴールデンとして採用): `diag_render.rs` の `human_full` (主 + 同じ行の副 + 別ファイルの副 + 注記)、`human_multiline_span_shows_start_line_only`、`human_tab_and_wide_gutter` (3 桁のガター、タブ、注記 2 個、空のメッセージ)、`human_empty_span_at_eof`、`human_utf8_and_invalid_bytes`。
- 追加のゴールデン (上の規則から導いた。TC63 の `a.sv` を使う。`diag_render.rs` に追加する):
  - 主ラベル `here` (17..18)、副ラベル `declaration` (12..16)、注記 `note one`、code `E0001`:
    ```
    error[E0001]: expected ';'
     --> a.sv:2:8
      |
    2 |   wire w
      |   ---- declaration
      |        ^ here
      |
      = note: note one
    ```
  - 複数行の主ラベル `scope` (10..28) だけ、code なし、message `in scope`:
    ```
    error: in scope
     --> a.sv:2:1
      |
    2 |   wire w
      | ^^^^^^^^ scope
    ```
  - マルチバイト: ファイル `m.sv` = `"  wire あw\n"`、主ラベルが `w` (バイト 10..11、9 文字目。`あ` は 3 バイト) で message 空、`error: e`:
    ```
    error: e
     --> m.sv:1:9
      |
    1 |   wire あw
      |         ^
    ```
- 実行: `cargo test -p svsim-common --test diag_render` / 自動。

#### M0-TC65 `DiagSink::into_sorted` (C-3)
- 順序の規則 (Q-14。plan §4.2 の「発行順」は置き換えられた): **位置の無い (ラベルの無い) 診断が先頭**、その後に位置のある診断。キーは (主 span、重大度、コード、メッセージ、ラベル、注記、fix-it) の辞書順。主 span は最初の `primary` ラベル (無ければ最初のラベル) の span で、(FileId、lo、hi) の順に比べる。発行順は使わない。
- 固定例: ファイル a (id 小) と b に、`b0` (b, 0)、`a9` (a, 9)、`a2` (a, 2)、ラベル無しの `none` の順に発行 → `["none", "a2", "a9", "b0"]`。
- 固定例 (同じ位置): 同じ span で重大度・メッセージだけが違う 2 つ → 発行順を入れ替えても同じ順序。ラベル無しの Error が 2 つ (`"y"`、`"x"`。他の内容は同じ) → メッセージの順 (`"x"` が先) で、発行順に依らない。
- 固定例 (主 span の選び方): ラベルが [副 (a, 1)、主 (a, 50)] の診断と、[主 (a, 10)] の診断 → 後者 (lo 10) が先 (前者のキーは主ラベルの 50)。
- 性質 (P): 任意の 1〜50 個の診断 (位置の有無、同じ位置、同じ内容の重複を含む) を、ランダムな順列で `emit` → `into_sorted` の結果が、順列に依らず同じ。結果の先頭の連続部分がちょうど位置の無い診断。
- `has_errors`: Error を 1 つ含めば true、Warning/Note/Help だけなら false。
- 実行: `cargo test -p svsim-common` (`diag.rs` 内) / 自動。

#### M0-TC66 インターン (C-4)
- `intern("foo")` を 2 回 → 同じ `Symbol`。`intern("bar")` → 違う `Symbol`。`resolve(intern(s)) == s`。
- `get("baz")` (未登録) → `None`、`get("foo")` → `Some(同じ Symbol)`。
- 空文字列、`"あいう"`、`"\0"`、10,000 文字の文字列、大文字小文字だけ違う `"Foo"`/`"foo"` (別の Symbol)。
- 性質 (P): 任意の文字列の列 (重複あり、1〜10,000 個) で、`intern` の結果が等しい ⇔ 文字列が等しい。全部 `resolve` で戻る。
- 実行: `cargo test -p svsim-common --test intern` / 自動。

#### M0-TC67 `IndexVec` と `define_index!` (plan §4.4 (1)、C-5 の補助)
- `define_index!(pub struct NodeId;)` で作った型で、`push` が 0, 1, 2… の順のインデックスを返す。`v[id]` / `v[id] = …` で読み書き。`iter_enumerated` が (id, &T) を順に返す。`len`。
- 実行: `cargo test -p svsim-common` / 自動。

#### M0-TC68 `ArenaLayoutBuilder::alloc` (C-5)
  | 操作 (順に) | 期待 |
  |---|---|
  | `alloc(3, 1)` | `WordOffset(0)` |
  | `alloc(1, 8)` | `WordOffset(8)` |
  | `alloc(2, 2)` | `WordOffset(10)` |
  | `finish()` → `WordArena::new` | 長さ 12 ワード、全部 0 |
  | 新しい builder で `alloc(u32::MAX, 1)` → `alloc(1, 1)` | 1 回目は `Ok(0)`、2 回目は `Err` (溢れ) |
  | 新しい builder で `alloc(u32::MAX − 3, 1)` → `alloc(1, 8)` | 2 回目は `Err` (位置合わせで溢れる) |
  | 新しい builder で `alloc(3, 1)` → `alloc(0, 8)` → `finish()` | 2 回目は `Ok(WordOffset(3))` (整列せず現在位置。2026-11-27 マネージャ決定で変更)。長さは 3 のまま (領域を消費しない。Q-15) |
  | 新しい builder で `alloc(0, 1)` → `alloc(2, 1)` | `Ok(0)` → `Ok(0)` |
  | `alloc(1, 3)` / `alloc(1, 0)` / `alloc(1, 6)` | `Err(ArenaError::BadAlign)` (Q-15) |
  | 上の溢れの 2 行 | `Err(ArenaError::Overflow)` (Q-15) |
- 実行: `cargo test -p svsim-common --test arena` / 自動。

#### M0-TC69 `disjoint_mut` (C-5)
- 長さ 12 のアリーナで:
  | ranges | 期待 |
  |---|---|
  | `[(0,4), (4,4)]` | `Ok`。両方に書いて `words()` で反映を確認 |
  | `[(4,4), (0,4)]` (順序が逆) | `Ok` |
  | `[(0,4), (3,2)]` | `Err` (重なり) |
  | `[(0,4), (0,4)]` | `Err` |
  | `[(10,4)]` | `Err` (範囲外) |
  | `[(0,12)]` | `Ok` |
  | `[(0,1), (1,1), (2,1)]` (N=3) | `Ok` |
  | `[(0,4), (2,0)]` | `Ok` (長さ 0 の区間は重ならない。Q-16) |
  | `[(5,0), (5,0)]` | `Ok` |
  | `[(12,0)]` | `Ok` (`off <= len`) |
  | `[(13,0)]` | `Err` (範囲外) |
- 実行: `--test arena` / 自動。

#### M0-TC70 スナップショット・復元・`content_hash` (C-5、決定性)
- `content_hash` は「各ワードをリトルエンディアンの 8 バイトにして並べた列」の FNV-1a 64 (offset basis `0xcbf29ce484222325`、prime `0x100000001b3`)。固定値 (Python で計算済み):
  | アリーナの内容 | `content_hash` |
  |---|---|
  | 長さ 0 | `0xcbf29ce484222325` |
  | `[0]` | `0xa8c7f832281a39c5` |
  | `[1]` | `0x89cd31291d2aefa4` |
  | `[0, 0x0123456789abcdef]` | `0x2d71366c8ba60175` |
- 手順: 値を書く → `snapshot` → ハッシュ h0 → 何ワードか変更 (h1 ≠ h0 を確認) → `restore` → ハッシュが h0、`words()` も元と一致。
- 別の長さの layout から作ったスナップショットで `restore` → **panic** (`#[should_panic]`。Q-17)。アリーナの内容は変わっていない (`catch_unwind` で確認してもよい)。
- 実行: `--test arena` / 自動。

#### M0-TC71 アリーナとロジック値ビューの結合 (C-5)
- 手順 (`crates/svsim-common/tests/arena_logic.rs`):
  1. layout に 4 値 65 ビット信号 A (4 ワード)、4 値 65 ビット信号 B (4 ワード)、2 値 64 ビット信号 C (1 ワード) を確保。
  2. `LogicMut::from_planar` で A に `65'h1_0000_0000_0000_0001`、B に `65'h0_ffff_ffff_ffff_ffff` を書く。A の bit 3 を x に。
  3. `disjoint_mut` で A と B を同時に取り、`and_assign(A, B)` を in-place で実行。
  4. 期待: A = `65'b0_…_x001` の形 (bit 64 は 1&0 = 0、bit 0 = 1、bit 3 = x&1 = x、他は 0)。B は不変。
  5. C を `BitMut` で書き、`add2` 系で in-place に 1 を足して桁上げを確認。
  6. snapshot → A を `fill(X)` → restore → A が手順 4 の結果の値に戻り、ハッシュも戻る。
- 実行: `cargo test -p svsim-common --test arena_logic` / 自動。

---

### 5.4 コーパスマネージャ (K 系)

#### M0-TC72 マニフェストの解析とエラー (K-4)
- 正常: plan §7.1 の `corpus/manifest.toml` が読めて、`svsim-corpus list` に `sv-tests` が 1 件出る。`rev` = `7274242bcf99a2f6f8282493d0200c92eb87d347`。
- エラーになるべき入力 (各々で `Err`、CLI なら終了コード **1** (マニフェストの誤りは実行時エラー。Q-18)、メッセージにエントリ名かキー名を含む):
  | 入力 | 理由 |
  |---|---|
  | `rev = "main"` | ブランチ名 |
  | `rev = "v1.0"` | タグ |
  | `rev = "7274242b"` | 短縮 SHA |
  | `rev` が 39 桁 / 41 桁 | 桁数 |
  | `rev` に 16 進以外の文字 (`g`) を含む 40 桁 | |
  | 同じ `name` のエントリが 1 つのマニフェストに 2 つ | 重複 |
  | 同じ `name` が `--manifest` で追加した別ファイルにある | 重複 (ファイルをまたぐ) |
  | 未知のキー: トップレベル (`foo = 1`)、`[[entry]]` (`colour = "x"`)、`[entry.source]` (`branch = "main"`)、`[entry.discover]` (`glob = "*"`) | `deny_unknown_fields` |
  | `[entry.source]` が無い、`git` が無い | 必須キー |
  | `kind = "glob"` (M0 で未対応のアダプタ) | 未知の `kind` (Q-19) |
  | `version = 2`、`version` が無い | 版 (Q-19) |
  | `rev = "7274242BCF99A2F6F8282493D0200C92EB87D347"` (大文字の 40 桁) | 小文字の 16 進だけを許す (Q-19) |
  | `git = "file:///tmp/x.git"` (テスト用の許可フラグなし) | file:// の URL は拒否 (STATUS の決定、T13)。同じマニフェストがテスト用の許可フラグ付きなら `Ok` |
- 実行: `cargo test -p svsim-corpus` / 自動。

#### M0-TC73 file:// での取得とキャッシュ (K-1、ネットワーク不要)
- 準備: 一時ディレクトリに作業リポジトリを作り、コミット C1 → C2 → C3 (先端) を作って bare に push。`SVSIM_CORPUS_CACHE` を一時ディレクトリに。マニフェストの `rev` = C2 (**先端でない**コミット)。file:// の URL を使うので、テスト用の許可フラグ (T13 で追加。名前は実装の doc に従う) を付けて実行する (付けなければ TC72 のとおり拒否される)。
- 期待:
  1. `fetch` が成功し、`<cache>/<name>/<C2>/` に C2 の内容 (C3 で足したファイルが無い) がある。`git -C <dir> rev-parse HEAD` = C2。`.svsim-complete` がある。`<cache>/<name>/.tmp-*` が残っていない。`.lock` が残っていない。
  2. 2 回目の `fetch`: bare リポジトリを**削除してから**実行しても成功する (ネットワーク/リモートに出ていない)。`--offline` でも成功。
  3. 空のキャッシュで `--offline` → エラー (終了コード 1)、キャッシュに何も作らない。
  4. リポジトリに存在しない 40 桁の rev → エラー、最終位置のディレクトリも完了印も作らない、`.lock` と `.tmp-*` を残さない。
  5. 最終位置が既にあって完了印がある状態で、別の一時ディレクトリから完了させる経路 (rename の失敗) → 成功扱い (R-18)。再現が難しければ、2 プロセスを同時に起動して両方が成功 (または一方がロックのエラー) し、キャッシュが壊れないことで代える。
- 実行: `cargo test -p svsim-corpus` / 自動。

#### M0-TC74 git の環境固定と安全性 (K-1 の補助、plan §7.2)
- 期待:
  1. 一時的な `HOME` に `~/.gitconfig` を置き、`core.hooksPath` を、`post-checkout` フック (実行されると印のファイルを作る) のあるディレクトリに向ける。`fetch` 後、印のファイルが**作られていない**。
  2. 取得するリポジトリに実行可能なスクリプト (`run.sh`、`conf/generators/x.py`。実行されると印を作る) を入れても、`fetch` / `discover` / `run` の後に印が無い。
  3. 認証の要る URL (存在しない `https://127.0.0.1:1/x.git` 等) でプロンプトを出さずにエラーで終わる (`GIT_TERMINAL_PROMPT=0`。標準入力を閉じて実行し、ハングしない。タイムアウト 30 秒)。
- 実行: `cargo test -p svsim-corpus` / 自動 (3 はローカルのポートを使うのでネットワーク不要)。

#### M0-TC75 ロック (K-5)
- ロックファイルの形式 (Q-20): 2 行のテキスト `pid=<10進>` と `created_unix=<10進の秒>`。テストはこの形式でロックを手で作る。取得中に作られたロックもこの形式であること (取得をロックの直後に止められない場合は、生きているロックのケースで中身を読んで確認する)。
  | 状態 | 期待 |
  |---|---|
  | `<cache>/<name>/.lock` に**存在しない PID** (`/proc/sys/kernel/pid_max` より大きい値) | 古いロックとして取り除き、取得が成功。終了後 `.lock` が無い |
  | `.lock` に**生きているプロセス**の PID (テストが起動した `sleep 60` の PID) | 待たずにエラー (終了コード 1)、メッセージに `--force-unlock` を含む。キャッシュは変更されない |
  | 同じ状態で `--force-unlock` | 成功 |
  | ロックの中身が壊れている (空、`pid=abc`、`created_unix` の行だけ、ゴミのバイト列) | 自動では取り除かずエラー (終了コード 1)、メッセージに `--force-unlock` を含む (Q-20)。`--force-unlock` を付ければ成功 |
- 実行: `cargo test -p svsim-corpus` / 自動 (Linux 前提。`/proc` を使う)。

#### M0-TC76 sv-tests アダプタの単体テスト (K-2)
- 一時ディレクトリに `tests/` を作り、小さな `.sv` を置く。期待:
  | ファイル | 期待 |
  |---|---|
  | `tests/a.sv`: `:name: a` `:tags: 5.6` | `id = "sv-tests/a.sv"` (`<entry>/<root からの相対>`)、`files = [tests/a.sv]`、`incdirs = [tests/]`、`target = Elaborate` (`type` の既定 `"parsing elaboration"`)、`should_fail = false` |
  | `tests/b.SV` (拡張子が大文字) | 列挙される |
  | `tests/c.svh`、`tests/d.v` | 列挙されない |
  | CRLF のファイル: `:type: simulation elaboration parsing\r\n` | `target = Simulate`、値に `\r` が残らない |
  | `:Should_Fail_Because: x` (大文字のキー) | `should_fail = true`、`reason = Some("x")` |
  | `:type: parsing` と後ろに `:type: simulation` (重複) | 後のものが勝って `Simulate` |
  | `:foo_bar: 1` (未知のキー) | 警告を出して無視、解析エラーにはしない |
  | `:runner_verilator_flags: -Wno-x` | `meta` に残る |
  | `:files: sub/x.sv sub/y.sv` `:incdirs: inc` | `tests/sub/x.sv`、`tests/sub/y.sv`、`tests/inc` (`tests/` からの相対) |
  | `:defines: A=1 B` | `defines = ["A=1", "B"]` |
  | `:top_module: top` | `top = Some("top")` |
  | `:type: preprocessing` / `parsing` / `elaboration` / `preprocessing parsing` / `simulation elaboration preprocessing` | Preprocess / Parse / Elaborate / Parse / Simulate (最も後の段階) |
  | 行の途中の `:name: x` (行頭でない) | メタデータとしない (正規表現 `^:`) |
  | `:tags:` の後に値が無い (`:tags:` だけ) | 正規表現 `(.+)` に一致しないので無視 |
  | `tests/sub/z.sv`、`tests/a.sv`、`tests/B.sv` | 結果の順序は `id` のバイト順 (`B.sv` < `a.sv` < `sub/z.sv`) |
- 実行: `cargo test -p svsim-corpus` / 自動。

#### M0-TC77 段階評価の枠組み (K-3)
- テスト用の `StageRunner` を登録した `Pipeline` で:
  - Preprocess=Pass、Parse=Fail のとき、Elaborate 以降を実行しない。
  - `target = Parse` のケースでは Elaborate を実行しない。
  - runner が登録されていない段階は `NotImplemented` で止まる。runner が 0 個なら全ケースが `Preprocess: NotImplemented`。
  - 判定は 3 値の `Verdict { AsExpected, Unexpected, Undetermined }` (Q-21)。生の結果と判定の両方が記録される。
    | should_fail | 段階の結果 (target = Parse) | Verdict |
    |---|---|---|
    | true | Preprocess=Pass、Parse=Fail | AsExpected |
    | true | Preprocess=Fail | AsExpected (`target` までのどこかで Fail) |
    | true | Preprocess=Pass、Parse=Pass | Unexpected |
    | false | Preprocess=Pass、Parse=Pass | AsExpected |
    | false | Preprocess=Pass、Parse=Fail | Unexpected |
    | true / false | Preprocess=NotImplemented | Undetermined |
    | true / false | Preprocess=Pass、Parse=Skipped / Timeout / NotImplemented | Undetermined |
  - runner が 0 個なら全ケースが Undetermined。
- `Stage` の順序: `Preprocess < Parse < Elaborate < Simulate < Compare`。
- 実行: `cargo test -p svsim-corpus` / 自動。

#### M0-TC78 実際の sv-tests の取得 (K-1、E2E)
- 手順:
  ```
  export SVSIM_CORPUS_CACHE=$(mktemp -d)        # 既定の場所を試すときは未設定で ~/.cache/svsim/corpus
  cargo corpus fetch sv-tests                    # 1 回目 (ネットワーク)
  D=$SVSIM_CORPUS_CACHE/sv-tests/7274242bcf99a2f6f8282493d0200c92eb87d347
  git -C $D rev-parse HEAD                       # 期待: 7274242bcf99a2f6f8282493d0200c92eb87d347
  test -f $D/.svsim-complete
  time cargo corpus fetch sv-tests               # 2 回目
  HTTPS_PROXY=http://127.0.0.1:9 https_proxy=http://127.0.0.1:9 cargo corpus fetch sv-tests   # 2 回目 (ネットワークを壊して)
  cargo corpus fetch sv-tests --offline
  ```
- 期待: 1 回目が終了コード 0。`rev-parse` が一致。2 回目は数秒以内 (目安 2 秒。cargo のビルドを除く) で終わり、ネットワークを壊しても `--offline` でも成功。未設定のときは `~/.cache/svsim/corpus/sv-tests/<rev>/` に置かれる。
- 実行: 手順 / 半自動 (X、ネットワーク必須)。

#### M0-TC79 実際の sv-tests の列挙 (K-2、E2E)
- 手順: `cargo corpus discover sv-tests --json > disc.json` と、表示される件数・統計。
- 期待 (固定コミット 7274242b での値。plan §7.3、plan-review R-16 で実ファイルから集計済み):
  | 項目 | 期待 |
  |---|---|
  | ケース数 | **1028** |
  | メタデータの解析エラー | 0 |
  | `should_fail = true` (`should_fail_because` あり) | **79** |
  | `target` の内訳 (`:type:` の内訳から導出) | Preprocess **90** (`preprocessing` 90)、Parse **14** (`parsing` 5 + `preprocessing parsing` 9)、Elaborate **577** (`elaboration` 5 + 指定なし 572)、Simulate **347** (`simulation` 4 + `simulation elaboration` 53 + `simulation elaboration parsing` 289 + `simulation elaboration preprocessing` 1)。合計 1028 |
  | `:type:` の生の値の内訳 (統計に出る場合) | `preprocessing` 90、`parsing` 5、`elaboration` 5、`preprocessing parsing` 9、`simulation` 4、`simulation elaboration` 53、`simulation elaboration parsing` 289、`simulation elaboration preprocessing` 1、指定なし 572 |
  | `tests/testbenches/*.sv` (11 件) と `tests/uvm/uvm_files.sv` | 列挙に含まれる |
  | id の順序 | バイト順に整列 (`LC_ALL=C sort -c` で確認) |
- **独立な検算** (実装と別の手段。差があれば実装を疑う前に検算の手段も確認する):
  ```
  cd $D && find tests -iname '*.sv' | wc -l                                   # 1028
  find tests -iname '*.sv' -print0 | xargs -0 grep -liE '^:should_fail_because:[[:space:]]*[^[:space:]]' | wc -l   # 79
  ```
- 実行: 手順 / 半自動 (X)。

#### M0-TC80 実際の sv-tests の実行と決定性 (K-3、E2E)
- 手順:
  ```
  cargo corpus run sv-tests --out /tmp/r1
  cargo corpus run sv-tests --out /tmp/r2
  ```
- 期待:
  1. 終了コード 0 (ケースの失敗では 0 以外にしない)。
  2. `results.jsonl` が 1028 行、各行が有効な JSON で、id が 1 回ずつ現れる。全行で段階の結果が `Preprocess: NotImplemented` (それ以外の段階の結果を持たない)。
  3. `summary.json` の Preprocess × NotImplemented が 1028、他は 0。全 1028 件の `Verdict` が `Undetermined` (should_fail の 79 件も含む。Q-21)。
  4. 標準出力に件数が出る。
  5. **決定性**: r1 と r2 で、所要時間のフィールドを除いた `results.jsonl` (行の順序を含めて) と `summary.json` がバイト単位で同一。
  6. `--out` を省略すると `target/corpus-results/sv-tests/` に出る。
- 実行: 手順 (jq で所要時間を除いて `diff`) / 半自動 (X/R)。

#### M0-TC81 CLI と終了コード (plan §7.5)
  | 実行 | 期待 |
  |---|---|
  | `svsim-corpus list` | 0、`sv-tests` を表示 |
  | `svsim-corpus` (サブコマンドなし)、`svsim-corpus frobnicate`、`svsim-corpus fetch --bogus` | 2 |
  | `svsim-corpus fetch --offline` (空のキャッシュ) | 1 |
  | `svsim-corpus discover` (NAME なし) | 2 |
  | `svsim-corpus discover no-such-entry`、`fetch no-such-entry`、`run no-such-entry` | **2** (存在しないエントリ名は使い方の誤り。Q-18) |
  | 誤りのあるマニフェストを `--manifest` で渡して `list` | **1** (Q-18) |
  | `cargo corpus list` (エイリアス) | `svsim-corpus list` と同じ |
- 実行: `cargo test -p svsim-corpus` (assert_cmd 等は依存に無いので `std::process::Command` で `CARGO_BIN_EXE_svsim-corpus` を起動) / 自動。

#### M0-TC82 CI の `corpus-fetch` ジョブ (K-6)
- 期待: T13 後の最新コミットで `corpus-fetch` が success。ログに `fetch` と `discover` の実行と 1028 件が出る。`actions/cache` のキーにマニフェストのハッシュが入っている (`hashFiles('corpus/manifest.toml')` 等)。2 回目の実行でキャッシュが当たる。
- 実行: `gh run view` / 手動 (M)。

---

## 6. エッジケースの横断一覧

テスト実行エージェントとコードレビューエージェントは、次の各項目が少なくとも 1 つの TC で固定値として確認されていることを見る。

| 分類 | エッジケース | TC |
|---|---|---|
| 幅 | 0 (連結・複製のみ)、1、2、31、32、33、63、64、65、127、128、129、1000 超 (1025, 4096)、2^20、`MAX_WIDTH` | 9, 32, 52, 53 |
| ワード境界 | 桁上げ・借り・シフト・連結・選択が 64 ビット境界をまたぐ | 24, 29, 32, 33 |
| 符号 | 最小値、−1、最小値 / −1、符号付きと符号なしで結果が変わる比較・除算・`>>>`・拡張・`%d`、base が符号なしの `8'hff ** -1` | 19, 26, 27, 29, 31, 46 |
| X/Z の混在 | 1 ビットだけ x / z、全 x、全 z、x と z が同じ桁に混在、MSB だけ x/z | 13〜20, 45, 46 |
| 0 除算 | 4 値 (x)、2 値 (0 [V-2])、0/0、`0 ** 負` | 26, 27 |
| シフト量 | x/z 入り、`w`、`w+1`、64 ビット超 (2^64, 2^70)、符号付き型の負の値 (符号なしで解釈) | 29, 30 |
| `**` | 0^0、0^負、1^負、(−1)^負 偶奇、\|b\|>1 で負、巨大な指数、base が符号なし | 27 |
| `?:` | x マージ、z/z [V-8]、条件が z | 20 |
| 等価 | `==` の既知不一致優先、`===` の x/z、`==?` の左右非対称 | 16, 17 |
| 選択 | 負のオフセット、一部範囲外、完全に範囲外、`i64` の端、`Unknown`、2 値の例外 | 33 |
| エッジ | 16 通り全部、x↔z、多ビットの LSB | 21 |
| `$clog2` | 0, 1, 2, 2^k±1, 多倍長, x | 36 |
| リテラル | x/z 左拡張、切り詰めフラグ、10 進の値切り詰め、10 進の x/z 1 桁、サイズなしの 32 超 [V-11]、サイズ 0、不正な数字 | 37, 38 |
| 文字列 | 空文字列、0 バイトの除去、8 の倍数でない幅、x/z [V-5] | 40 |
| atoi | 符号・空白・`0x` [V-6]、32 ビット超、`_` | 41 |
| 実数 | 半分の丸め (0 から遠い方)、0.49999999999999994、二重丸め、2^53±1、∞ への溢れ、NaN/∞ [V-4]、−0.0、非正規化数、x/z [V-14] | 42, 43 |
| 表示 | 大文字小文字 (`x/X/z/Z`)、最上位の端数桁、`%d` の自動幅 (符号込み)、`%0h` の x 桁、`%05d`、幅が足りないとき切り詰めない [V-7, V-13] | 45〜47 |
| DPI | 未使用ビットのマスク (aval/bval 両方)、要素数、交互配置 | 50, 51 |
| 正規化 | 上位の未使用ビットを汚しやすい演算 | 12 |
| ソース | CRLF、最終行改行なし、空、不正 UTF-8、マルチバイト | 61, 64 |
| アリーナ | 位置合わせ、u32 溢れ、重なり、範囲外 | 68, 69 |
| コーパス | 先端でない rev、オフライン、古いロック、CRLF、キーの大文字、重複キー | 73, 75, 76 |

---

## 7. 暫定期待値と VCS 確認リスト (plan v2.1 §11、V-1〜V-18) の対応

VCS の結果 (M1 で確認) が暫定と違ったら、該当 TC の期待値を差し替える (挙動を `--compat=vcs` に回すかはその時点でマネージャが判断)。テストのコメントには `[要VCS確認 V-n]` を書く。

| V-ID | 暫定期待値 | TC | 付録 A |
|---|---|---|---|
| V-1 | 符号付き 最小値 / −1 = 最小値、剰余 0 | TC26 | A-1 |
| V-2 | 2 値の 0 除算・剰余 = 0、2 値 `0 ** -1` = 0 | TC26, TC27 | A-2 |
| V-3 | `>>>` で符号ビット z → z で埋める (シフト量 ≥ w でも) | TC29 | A-3 |
| V-4 | NaN / ±∞ → 整数 = 0 (+ `NotFinite`) | TC43 | A-4 |
| V-5 | 整数 → string で x/z は 0 として扱い、0 バイトは捨てる | TC40 | A-5 |
| V-6 | atoi 系は符号・空白・接頭辞を解釈しない、先頭の `_` は読み飛ばす (`"_12"` → 12)、32 ビット超は切り詰め | TC41 | A-6 |
| V-7 | `%d` の自動幅、`%Nd` は空白、`%0Nd` は 0、`%Nh` は 0、`%0h` は x 桁で止まる。追加: `%05d` の負数 → `-0005`、`%5h` of `8'bx` → `000xx`、`%05d` of `8'bx` → `0000x`、N が自動幅より小さい `%1h` of `8'h0a` → `a` | TC46, TC47 | A-7 |
| V-8 | `1'bx ? 1'bz : 1'bz` = x | TC20 | A-8 |
| V-9 | 2 値データ × 4 値の自己決定オペランドは 4 値で評価 (x)、選択だけ 2 値の 0 | TC33, TC34 | A-9 |
| V-10 | `$clog2(0)` = 0、x/z → `None` | TC36 | A-10 |
| V-11 | サイズなしリテラルが 32 ビット超 → 広げて警告フラグ | TC37, TC38 | A-11 |
| V-12 | エッジ表 (x↔z はエッジでない、z→1 は posedge、1→z は negedge)、多ビットは LSB | TC21 | A-12 |
| V-13 | `%h`/`%o` の桁に x と z が混在 → `X` | TC45 | A-13 |
| V-14 | 整数 → 実数で x/z は 0 | TC42 | A-14 |
| V-15 | `==` で既知ビットの不一致があれば x を含んでも 0 (`4'b1x00 == 4'b0x00` → 0、`4'b1x00 == 4'b1000` → x) | TC16 | A-15 |
| V-16 | `==?` は右の x/z だけがワイルドカード、左の x/z は x | TC17 | A-16 |
| V-17 | casez は両側の z を無視、casex は両側の x/z を無視 | TC18 | A-17 |
| V-18 | サイズ付きリテラルの `truncated` は 0 以外 (1/x/z) のビットが落ちたときだけ (`4'o17`、`8'h0ff` → false、`4'hff` → true) | TC37 | A-18 |

規格の条番号自体が未検証のもの (plan 冒頭の注記: 11.4.3/4/6/7/8/9、11.6.1、11.8.2、6.8、6.11.2、9.4.2、12.4、12.5.1、20.8.1、21.2.1.3/4) を根拠にした期待値は、規格本文を参照できるようになった時点で照合する。

---

## 8. 未確定事項の解消 (plan.md v2.1 §12 の回答の反映先)

v1 で挙げた Q-1〜Q-22 は、plan.md v2.1 §12 の回答 (STATUS: マネージャが採用を決定) で**すべて解消**した。v1 の提案と回答が違ったものは「差分」欄に書いた。

| ID | 回答 (要旨) | 反映した TC | 差分 (v1 の提案との違い) |
|---|---|---|---|
| Q-1 | API の誤用は panic (release でも)、メッセージに関数名と期待値・実際の値 | TC11, TC57 | — |
| Q-2 | `count_bits` の空の集合 → 0 | TC14 | — |
| Q-3 | `truncated` は 0 以外のビットが落ちたときだけ [V-18] | TC37, TC38 | 字義どおりの案ではなく後者の案 |
| Q-4 | atoi の `_` は読み飛ばす、数字が無ければ 0 [V-6] | TC41 | — |
| Q-5 | 範囲外の `to_f64`/`to_f32` は ±∞ | TC42 | — |
| Q-6 | `RealToIntStatus { Exact, Rounded, Wrapped, NotFinite }` | TC43, TC44, TC60 | 4 種で確定。参照モデルは T8 着手時に 4 種に合わせる |
| Q-7 | `-0005`、`000xx`、`0000x` [V-7] | TC47 | — |
| Q-8 | 最小表示を元に N 文字まで埋める (`%1h` of `8'h0a` → `a`) [V-7] | TC47 | — |
| Q-9 | `format_len_bound` で予約すれば確保 0 回 | TC54 | `format_len_bound` が追加 |
| Q-10 | 改行は LF だけ | TC61, TC62 | — |
| Q-11 | 終端を超える offset は**ファイル末尾として扱う** (panic しない) | TC61 | **v1 の提案 (panic) と逆** |
| Q-12 | `line_text` は `\n` とその直前の `\r` 1 つを除く。最終行の単独の `\r` は残す | TC62 | — |
| Q-13 | JSON: `hi` (排他) の位置、小文字、`code: null`、発行順 | TC63 | — |
| Q-14 | `into_sorted` は**位置の無い診断を先頭**に、内容 (主 span、重大度、コード、メッセージ、…) の辞書順。発行順は使わない | TC65 | **v1 の提案 (最後に発行順) と逆。同じ位置でも発行順を使わない** |
| Q-15 | `BadAlign` (0 / 2 のべきでない)、`Overflow`、`words = 0` は `Ok` で消費しない | TC68 | — |
| Q-16 | 長さ 0 の区間は重ならない扱い (`off <= len` なら `Ok`) | TC69 | — |
| Q-17 | 長さの違う `restore` は panic | TC70 | — |
| Q-18 | マニフェストの誤りは 1、**存在しないエントリ名だけ 2** | TC72, TC81 | **v1 の提案 (どちらも 1) と違う** |
| Q-19 | `version` ≠ 1、未知の `kind`、大文字の SHA はいずれもエラー (1) | TC72 | — |
| Q-20 | ロックは `pid=<n>` と `created_unix=<n>` の 2 行。壊れたロックはエラー + `--force-unlock` の案内 | TC75 | — |
| Q-21 | `Verdict { AsExpected, Unexpected, Undetermined }`、M0 は全件 Undetermined | TC77, TC80 | — |
| Q-22 | bits 文字列: 空は幅 0 で `Ok`、`0 1 x X z Z` を受ける、`_` を含む他の文字はエラー、出力は小文字 | TC10, TC60 | v1 の TC10 にあった「`_` を無視」の期待値は削除 |

TC64 の人間向け表示は、Q とは別に、マネージャが承認した T10 の書式 (rustc 風) を仕様としてゴールデンを固定した。

新たな未確定事項は無い。ただし次の 2 点は、回答の文言から期待値を導いた (実装時に食い違いがあればマネージャに報告すること)。
- TC43: `from_f64_trunc` で小数部を捨てた場合の状態は `Rounded` とした (Q-6 の「小数部を丸めた」に 0 方向への切り捨ても含めると解釈)。
- TC68: `alloc(0, 8)` は位置合わせしたオフセットを返すが、builder の次の位置は進めない (長さに影響しない) と解釈した (Q-15 の「領域は消費しない」)。

---

## 9. 実行手順のまとめ (テスト実行エージェント向け)

```
# 1. 静的・共通 (TC1〜TC7)
rustc --version                                   # 1.97.0
cargo build --workspace --locked
cargo test  --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo deny check licenses bans sources
grep -nE '^\s*-?\s*uses:' .github/workflows/*.yml   # 全部 @<40桁SHA> # tag
grep -rnw unsafe crates --include='*.rs'            # no_alloc*.rs だけ

# 2. svsim-logic (TC9〜TC58。個別に流すとき)
cargo test -p svsim-logic --test golden_bitwise --test golden_arith --test golden_struct --test golden_literal --test golden_fmt
cargo test -p svsim-logic --test diff_core --test diff_bitwise --test diff_compare --test diff_arith --test diff_struct --test diff_literal_string --test diff_real_fmt
cargo test -p svsim-logic --test no_alloc_core --test no_alloc_bitwise --test no_alloc_arith --test no_alloc_struct
cargo test -p svsim-logic --release                 # TC57 の release での assert
PROPTEST_CASES=100000 cargo test -p svsim-logic --release        # TC55 (L-2)
cargo test -p svsim-logic --release -- --ignored                  # TC53
cargo test -p svsim-logic-ref                                     # TC60

# 3. svsim-common (TC61〜TC71)
cargo test -p svsim-common

# 4. コーパス (TC72〜TC81)
cargo test -p svsim-corpus
cargo corpus fetch sv-tests && cargo corpus fetch sv-tests --offline         # TC78
cargo corpus discover sv-tests --json                                        # TC79
cargo corpus run sv-tests --out /tmp/r1 && cargo corpus run sv-tests --out /tmp/r2   # TC80

# 5. 手動 (TC6, TC8, TC59, TC60, TC82)
gh run list --limit 5
```
テスト実行エージェントは test-report.md に、TC ごとの合否、実際のコマンド出力 (件数・所要時間)、§8 の末尾の「回答の文言から導いた 2 点」について実装がどう振る舞ったかを書く。

---

## 付録 A. M1 の VCS 確認用 SV テスト集の候補 (決定 U-7)

M0 では実行しない。M1 でパーサが入ったら、各断片を `$display` で結果を出す小さな SV テストにし、ユーザが VCS で実行する。期待欄は sv-sim の暫定。

| ID | V-ID | SV 断片 (要旨) | sv-sim の暫定の表示 |
|---|---|---|---|
| A-1 | V-1 | `logic signed [7:0] a=8'sh80, b=-8'sd1; $display("%h %h", a/b, a%b);` | `80 00` |
| A-2 | V-2 | `int a=5, z=0; $display("%0d %0d", a/z, a%z); bit signed [7:0] p = 8'sd0 ** -8'sd1; $display("%0d", p);` | `0 0` / `0` |
| A-3 | V-3 | `logic signed [3:0] s=4'bz000; $display("%b %b", s>>>1, s>>>8);` | `zz00 zzzz` |
| A-4 | V-4 | `real r=0.0/0.0; int i=r; $display("%0d %0d", i, $rtoi(1.0/0.0));` | `0 0` |
| A-5 | V-5 | `string s = string'(16'h41xx); $display("%0d %s", s.len(), s);` | `1 A` |
| A-6 | V-6 | `string s="-12"; $display("%0d %0d %0d %0d", s.atoi(), " 12".atoi(), "0xff".atohex(), "_12".atoi());` | `0 0 0 12` |
| A-7 | V-7 | `logic [7:0] v=8'd42; logic signed [7:0] n=-5; $display("[%d][%5d][%05d][%5h][%0h][%d][%5h][%05d][%1h]", v, v, n, v, 12'bxxxx_0000_0001, 8'bx, 8'bx, 8'bx, 8'h0a);` | `[ 42][   42][-0005][0002a][x01][  x][000xx][0000x][a]` |
| A-8 | V-8 | `logic c=1'bx; $display("%b %b", c?1'bz:1'bz, c?4'b1x0z:4'b1x0z);` | `x 1x0x` |
| A-9 | V-9 | `bit [7:0] b=8'ha5; logic [2:0] s='x; bit [7:0] r; $display("%b %b %b", b<<s, b**s, b[s]); r=b<<s; $display("%b", r);` | `xxxxxxxx xxxxxxxx 0` / `00000000` |
| A-10 | V-10 | `localparam P=$clog2(0); $display("%0d %0d", P, $clog2(x_sig));` | `0 x` (実行時) |
| A-11 | V-11 | `$display("%0d %h", $bits('h1_0000_0000), 64'hx | 'hx);` | `33` / `x` が 16 桁 |
| A-12 | V-12 | `logic s; always @(posedge s) $display("pos %t", $time); initial begin s=1'bx; #1 s=1'bz; #1 s=1'b1; #1 s=1'bz; #1 s=1'bx; end` + negedge 版 | posedge は z→1 の 1 回だけ、negedge は 1→z の 1 回だけ |
| A-13 | V-13 | `$display("%h %o", 4'bxz01, 3'bxz1);` | `X X` |
| A-14 | V-14 | `real r = 8'b1x01; $display("%f", r);` | `9.000000` |
| A-15 | V-15 | `$display("%b %b", 4'b1x00 == 4'b0x00, 4'b1x00 == 4'b1000);` | `0 x` |
| A-16 | V-16 | `$display("%b %b %b", 4'b1x10 ==? 4'b1x1z, 4'b1x10 ==? 4'b1010, 4'b1x1z ==? 4'b1010);` | `1 x x` |
| A-17 | V-17 | `casez (4'b1x01) 4'b1101: $display("hit"); default: $display("miss"); endcase` と casex 版 | casez `miss`、casex `hit` |
| A-18 | V-18 | `logic [3:0] a = 4'o17; logic [7:0] b = 8'h0ff; logic [3:0] c = 4'hff;` を VCS でコンパイルし、切り詰めの警告が出る行を見る | 警告は `c` の行だけ |
