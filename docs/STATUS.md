# 進捗 (マネージャが更新)

状態: todo / doing / review / done / blocked

## M0 基盤
| タスク | 内容 | 担当 | 状態 |
|---|---|---|---|
| M0-P | M0 詳細計画 | planner | done |
| M0-R | 計画レビュー (条件付き承認 → v2 で反映) | plan-reviewer | done |
| M0-TP | テスト計画 | test-planner | doing |
| M0-T1 | workspace 骨組み (波1) | implementer | done |
| M0-T3a | logic 中核の型・API・全スタブ (波2) | implementer | doing |
| M0-T9 | 参照モデル svsim-logic-ref (波2、独立実装) | implementer (別個体) | doing |
| M0-T10 | ソースマップ・診断 (波2) | implementer | done |
| M0-T12 | コーパスマネージャ雛形 (波2) | implementer | done |
| M0-T3b/T4/T6/T2 | 波3 | implementer | todo |
| M0-T5/T7/T8/T13 | 波4 | implementer | todo |
| M0-T11 | 波5 | implementer | todo |
| M0-TR | テスト実行 | test-runner | todo |

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
