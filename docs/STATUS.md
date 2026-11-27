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
| 2026-11-27 | M0 Q-22 bits 文字列 | 空 = 幅0 で Ok、0/1/x/X/z/Z を受理、'_' 等はエラー、出力は小文字 (T3a/T9 に通知) | マネージャ |
| 2026-11-27 | M0 Q-13 診断 JSON | T10 実装を採用: end_* は hi (排他) の位置、severity 小文字、code 無しは null、ラベルは発行順 | マネージャ |
| 2026-11-27 | M0 Q-3 リテラル切り詰め | truncated は 0 以外 (1/x/z) のビットが落ちたときだけ立てる (Verilator と同じ)。VCS 確認リストに追加 | マネージャ |
| 2026-11-27 | M0 Q-7/Q-8, A-15〜17 | VCS 確認リストに追加 (%05d の負数、%Nh で N < 自動幅、条番号未検証の期待値) | マネージャ |
| 2026-11-27 | M0 Q-1〜Q-21 残り | planner が plan.md v2.1 で回答し、test-plan.md の期待値を確定する | マネージャ |
| 2026-11-27 | M0 ops 命名 | ops/mod.rs で glob 再エクスポートしない。カーネルは `ops::<分野>::<名前>`。同じファイル内の層1スカラ版は `_l64` 接尾辞 | マネージャ |
| 2026-11-27 | proptest 回帰ファイル | proptest 既定 (テストファイル隣の `tests/<name>.proptest-regressions`) を採用しコミットする | マネージャ |
