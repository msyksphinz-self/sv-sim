# 進捗 (マネージャが更新)

状態: todo / doing / review / done / blocked

## M0 基盤
| タスク | 内容 | 担当 | 状態 |
|---|---|---|---|
| M0-P | M0 詳細計画 | planner | done |
| M0-R | 計画レビュー (条件付き承認 → v2 で反映) | plan-reviewer | done |
| M0-TP | テスト計画 (82 項目、Q 残りは plan v2.1 で解消) | test-planner | done |
| M0-T1 | workspace 骨組み (波1) | implementer | done |
| M0-T3a | logic 中核の型・API・全スタブ (波2) | implementer | done |
| M0-T9 | 参照モデル svsim-logic-ref (波2、独立実装) | implementer (別個体) | done |
| M0-T10 | ソースマップ・診断 (波2) | implementer | done |
| M0-T12 | コーパスマネージャ雛形 (波2) | implementer | done |
| M0-T3b | words/abi/diff_core/no_alloc_core + レビュー minor (波3) | implementer | done |
| M0-T4 | ビット演算・リダクション・論理・比較・?:・エッジ (波3) | implementer | done |
| M0-T6 | シフト・幅変更・連結・選択・$clog2 (波3) | implementer | done |
| M0-T2 + T12fix | CI・deny.toml + コーパスのレビュー修正 (波3) | implementer | done |
| M0-T5 | 算術 (+ - * / % **、混在 pow_bits) (波4) | implementer | done |
| M0-T7 | リテラル・整数変換・文字列 (波4) | implementer | done |
| M0-T8 | 実数変換・表示フォーマット (波4) | implementer | done |
| M0-T13 | コーパス段階評価 + CI corpus-fetch + 波3 nit (波4) | implementer | done |
| M0-T11 | インターン・アリーナ (波5) | implementer | done |
| M0-T14 | 整理 (改名・補助統合・正規化修正) (波5) | implementer | done |
| M0-CR5 | 波5 コードレビュー (T11/T13b/T14) | code-reviewer | done |
| M0-T13b | コーパスのパス安全性修正 (波4レビュー) | implementer | done |
| M0-T9b | 参照モデル RealToIntStatus 4種化 (本体を読まない別個体) | implementer | done |
| M0-TR | テスト実行 (合格 23/26、L-2 不合格、A-2/K-6 未実施) | test-runner | done |
| M0-T15 | テスト不足・L-2 修正、CI トリガ修正 | implementer | doing |

## 決定事項ログ
| 日付 | 項目 | 決定 | 決定者 |
|---|---|---|---|
| 2026-11-27 | M0 U-1 4値メモリ配置 | 64bit プレーン分離 (D-1) を採用。4値は DPI 境界で変換、2値はゼロコピー。PLAN.md §3/§6.1 を修正する | マネージャ (レビュー結果で再確認) |
| 2026-11-27 | M0 U-2 完了条件 | sv-tests 取得可能まで。パース計測は M1 | マネージャ |
| 2026-11-27 | M0 U-3 ツールチェーン | 1.97.0 に固定 (rust-toolchain.toml) | マネージャ |
| 2026-11-27 | M0 U-4 CI | GitHub Actions を使用 | ユーザ |
| 2026-11-27 | M0 U-5 action 固定 | コミット SHA で固定 | マネージャ |
| 2026-11-27 | M0 U-7 未規定挙動 | M1 で確認用 SV テスト集を用意し、ユーザが業務環境の VCS で結果を確認する。それまでは暫定挙動 | ユーザ |
| 2026-11-27 | M0 R-2 例外 | 2値ベクタを X インデックスで読むと 0 (§11.5.1 明文どおり) を採用 | マネージャ |
| 2026-11-27 | M0 U-6 MAX_WIDTH | 2^24 ビットで確定 | マネージャ |
| 2026-11-27 | M0 R-9/R-10 | atoi は符号を解析しない / ?: の z/z は X (LRM 優先)。VCS 確認リスト V-6/V-8 | マネージャ |
| 2026-11-27 | T13 範囲 | T13 の変更範囲に crates/svsim-corpus/src/lib.rs (モジュール宣言・Error 追加) を含める | マネージャ |
| 2026-11-27 | corpus file:// | マニフェスト検証で file:// URL を拒否し、テスト用フラグ時のみ許可する (T13 で対応) | マネージャ |
| 2026-11-27 | M0 Q-22 bits 文字列 | 空 = 幅0 で Ok、0/1/x/X/z/Z を受理、'_' 等はエラー、出力は小文字 (T3a/T9 に通知) | マネージャ |
| 2026-11-27 | M0 Q-13 診断 JSON | T10 実装を採用: end_* は hi (排他) の位置、severity 小文字、code 無しは null、ラベルは発行順 | マネージャ |
| 2026-11-27 | M0 Q-3 リテラル切り詰め | truncated は 0 以外 (1/x/z) のビットが落ちたときだけ立てる (Verilator と同じ)。VCS 確認リストに追加 | マネージャ |
| 2026-11-27 | M0 Q-7/Q-8, A-15〜17 | VCS 確認リストに追加 (%05d の負数、%Nh で N < 自動幅、条番号未検証の期待値) | マネージャ |
| 2026-11-27 | M0 Q-1〜Q-21 残り | planner が plan.md v2.1 で回答し、test-plan.md の期待値を確定する | マネージャ |
| 2026-11-27 | M0 ops 命名 | ops/mod.rs で glob 再エクスポートしない。カーネルは `ops::<分野>::<名前>`。同じファイル内の層1スカラ版は `_l64` 接尾辞 | マネージャ |
| 2026-11-27 | proptest 回帰ファイル | proptest 既定 (テストファイル隣の `tests/<name>.proptest-regressions`) を採用しコミットする | マネージャ |
| 2026-11-27 | M0 Q-1〜Q-22 | plan.md v2.1 §12 の回答を採用 (要ユーザ確認なし)。注意: Q-6 の RealToIntStatus は4種 (Exact/Rounded/Wrapped/NotFinite) だが参照モデルは2種 → T8 着手時に参照モデルも合わせる | マネージャ |
| 2026-11-27 | test-plan v2 解釈 | from_f64_trunc で小数部を捨てたら Rounded / alloc(0, align) は整列オフセットを返し次位置は進めない、を採用 | マネージャ |
| 2026-11-27 | 波2レビュー | T3a/T9/T10 承認、T12 修正後承認 (major: ロック形式を created_unix= に)。T12 修正は波3で T2 と同じエージェントが実施 | マネージャ |
| 2026-11-27 | T12 追加修正 | ロックに File::try_lock (flock) を併用、git に protocol 許可リスト (https のみ、テスト時 file)、マニフェスト未検出は終了コード 1 | マネージャ |
| 2026-11-27 | T3b 範囲拡大 | tests/common に RefLogic/bool/Edge 用の比較補助を追加、L64::bit に debug_assert、Scratch は「公開カーネルは Scratch、内部はスライス」方針を doc 化 + split_words 相当を追加。scalar.rs/scratch.rs/tests/common/mod.rs を範囲に含める | マネージャ |
| 2026-11-27 | サイズなし10進 2^31〜2^32-1 | 参照モデルの現挙動 (32bit 符号付き、負値になる) を暫定採用、V-11 で VCS 確認 | マネージャ |
| 2026-11-27 | リテラル先頭の `_` | 文法どおりエラー (T7 も同じにする) | マネージャ |
| 2026-11-27 | コーパス URL | マニフェストは https:// のみ許可 (テスト時 file://)。業務の社内リポジトリで ssh が必要になったら見直す | マネージャ |
| 2026-11-27 | 運用メモ | T2 担当エージェントが worktree ガードを base64 で回避しようとして権限チェックで拒否された (以後は試行せず crates.io から導入)。実装エージェント定義に「ガードや権限チェックを回避しない」を追記 | マネージャ |
| 2026-11-27 | T3b 残件 | view.rs の panic メッセージに関数名を入れる (TC57) は波4で小修正タスクとして実施。words.rs の allow(dead_code) は M0 完了時に外す。公開カーネル同士の Scratch 入れ子は避け、内部はスライスを渡す (T5 へ伝達) | マネージャ |
| 2026-11-27 | 命名規約 (2値) | 2値版は名前の末尾に `2` (`shl2`, `shl_const_assign2`, `clog2_2`)、層1は `<name>2_l64`、2値×4値の混在は `_bits` (カーネル) / `_logic` (BitVec メソッド)。T6 の命名を規約として採用 | マネージャ |
| 2026-11-27 | ビット補助の統合 | shift.rs の pub(crate) ビット補助 (read_bits 等) は波4の小修正タスクで words.rs に移す | マネージャ |
| 2026-11-27 | 層3 の eq/ne/not | PartialEq::eq を隠す固有メソッドは紛らわしいので、LogicVec/BitVec の固有 eq/ne は `logic_eq`/`logic_ne` に改名、not は std::ops::Not を実装。波4の整理タスクで実施 | マネージャ |
| 2026-11-27 | 整理タスク M0-T14 (波4) | 上記改名、ビット補助の words.rs への移動、view.rs の panic メッセージに関数名、テストのローカル比較補助を tests/common に寄せる | マネージャ |
| 2026-11-27 | 波3レビュー | 全タスク承認 (blocker/major 0)。レビュアが規格から独立モデルを書いて照合し不一致 0 | マネージャ |
| 2026-11-27 | T14 追加項目 | concat/replicate/get_slice の dst 上位ビット正規化 + 上位を汚した dst のテスト、shift の 1 パス化は任意 | マネージャ |
| 2026-11-27 | T13 追加項目 | fetch.rs: 禁止プロトコルに ftp/ftps、GIT_SSL_NO_VERIFY 等を env_remove、manifest の https は小文字に限定、LockGuard::drop で remove 直前に same_file 再確認。ci.yml: push は main のみ + pull_request、main では cancel-in-progress しない | マネージャ |
| 2026-11-27 | 波構成の変更 | T14 は words.rs/vec.rs を触るため T5/T7/T8 と衝突 → 波5 (T11 と並行) に移す。参照モデルの RealToIntStatus 4種化は T9 担当エージェント (本体を読まない) に別途依頼 | マネージャ |
| 2026-11-27 | T7 | リテラル幅 > MAX_WIDTH は Err(TooWide) (参照モデルは上限なし、差分テスト範囲外)。sema での警告/エラーは M1 で決める。文字列変換の層1版は作らない (JIT では層2を呼ぶ) | マネージャ |
| 2026-11-27 | T13 | discover の解析エラーは当面 終了コード 0 + 件数表示。CI で件数 (1028) を検査する厳格モードは M1 で追加。GIT_ASKPASS 除去は社内 https 認証が必要になったら見直し | マネージャ |
| 2026-11-27 | T8 | 幅0の表示は panic せず空/`0` (呼び出し側の誤り扱い)。T9b 完了後に diff_real_fmt へ状態の直接照合を追加 (T14 で実施)。10進表示 O(nw²) は M5 で必要なら改善 | マネージャ |
| 2026-11-27 | T9b | Wrapped と Rounded が同時なら Wrapped 優先。幅0 dst は「n=0 のときだけ収まる」(本体と揃える、T14 で確認)。TC44 の状態照合の保留を解除 (T14 で diff_real_fmt に追加) | マネージャ |
| 2026-11-27 | alloc(0, align) 変更 | 先の決定を変更: BadAlign 検査のみ行い、整列せず現在位置を返す (次位置も進めない)。オフセットが常にレイアウト長以下になり、幅0ビューが範囲外にならないため。test-plan TC68 の期待値を更新すること | マネージャ |
| 2026-11-27 | svsim-common のテスト | proptest は dev-dependency に追加せず、決定的 xorshift で代用 | マネージャ |
| 2026-11-27 | 波4レビュー | T5/T7/T8/T9b 承認、T13 修正後承認 (minor: 明示 :files: の外部パス、--json の絶対パス)。レビュアの独立モデル照合: 算術 6万件・実数 80万件・log10 定数 全件で不一致 0。修正は M0-T13b | マネージャ |
| 2026-11-27 | M1 への持ち越し | コーパス runner はファイルを開く時にシンボリックリンクでチェックアウト外に出ないか確認する (T13b の検査は字句のみ) | マネージャ |
| 2026-11-27 | T14 | Logic::not (1bit, const fn) は固有メソッドのまま。BitVec の bool 版は bit_eq/bit_ne。fmt 幅0 dst の状態は本体・参照・Q-6 の3者一致 | マネージャ |
| 2026-11-27 | 波5レビュー | T11/T14 承認、T13b 修正後承認 (minor のみ)。汚れた dst で層2全カーネル 5246 件合格、T14 前後で出力ハッシュ 7952 件完全一致 | マネージャ |
| 2026-11-27 | CI トリガ | T13 で push を main 限定にした結果、作業ブランチで CI が走らなくなっていた → 全ブランチに戻す (T15)。corpus-fetch は workflow_dispatch でも実行 | マネージャ |
| 2026-11-27 | L64::bit の範囲外 | 層1 (JIT 仕様・ホットパス) は debug_assert のみで可。Q-1 の「release でも panic」は層2/層3 の API に適用 | マネージャ |

## M1 に持ち越す作業 (バックログ)
| 項目 | 由来 |
|---|---|
| コーパスのパス検査を OS 非依存に (`\` と `:` を常に拒否、Windows 形式のテスト 4 件)。空白だけの :files: に警告。結合パスの正規化 | 波5レビュー (T13b) |
| コーパス runner でファイルを開くときにシンボリックリンクで外に出ないか確認 | T13b |
| discover の厳格モード (解析エラーで終了コード 1、CI で 1028 件を検査) | T13 |
| should_fail の判定規則 (target 前段の Fail を AsExpected にする暫定規則) の見直し | T13 / plan §7.4 |
| Interner の文字列二重保持の解消、u32::MAX 予約の方針を index と揃える | 波5レビュー (T11) |
| アリーナのスナップショットに layout 識別子 (M4/M6 候補) | 波5レビュー (T11) |
| shift の one_pass テストに 2 値版を追加 | 波5レビュー (T14) |
| VCS 確認用 SV テスト集 (plan.md §11 の V-1〜V-18、test-plan 付録 A) を作成し、ユーザに業務環境の VCS で実行してもらう | U-7 |
| FileOrigin::Include の設計見直し、全角文字の表示幅 | T10 |
| リテラル幅 > MAX_WIDTH の診断 (警告/エラー) | T7 |
