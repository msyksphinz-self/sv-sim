# M0「基盤」詳細計画 (M0-P)

- 作成: 計画エージェント (planner)、2026-11-27
- 対象: `docs/PLAN.md` §1, §3, §10.1, §11 の M0
- 状態: v2 (計画レビュー M0-R の指摘を反映済み。マネージャの確認待ち)

## 改訂履歴
| 版 | 日付 | 内容 |
|---|---|---|
| v1 | 2026-11-27 | 初版 (ドラフト) |
| v2 | 2026-11-27 | レビュー反映 (`plan-review.md` の R-1〜R-27 と、マネージャの決定)。主な変更点は次のとおり。<br>1. lint の方針: workspace は `unsafe_code = "deny"`、ライブラリ/bin のルートは `#![forbid(unsafe_code)]`、`no_alloc` テストだけ `allow` (R-1)<br>2. 2 値のデータに 4 値の自己決定オペランドが来る式の意味論と、混在カーネルのシグネチャ (§3.5.1、R-2)<br>3. 配置の契約は packed 値 1 個に限定。unpacked 配列の要素の配置は M1 以降で決める (§3.2、R-3)<br>4. 参照モデルを独立タスク T9 にし、別の実装エージェントが書く (R-4)<br>5. T3 を T3a/T3b に分割。波を組み直し、worktree 運用の手順を追加 (§8、R-5, R-6)<br>6. 幅規則の表を直した。リダクションと論理演算のオペランドは自己決定 (§3.6、R-7)<br>7. エッジ検出 (§9.4.2) と `$clog2` (§20.8.1) を追加 (R-8)<br>8. atoi 系は符号を解析しない (R-9)。`?:` の z/z は X (R-10)<br>9. VCS 確認リストの節を新設 (§11)<br>10. U-1〜U-7 の決定を反映。CI の action は SHA 固定。コーパスキャッシュの既定を `~/.cache/svsim/corpus` に変更<br>11. 過剰設計として外したもの: コーパスの glob アダプタ、`--jobs`、`summary.md`、`path =` ソース、`corpus/private/`、診断の複数行・色表示。いずれも後回し<br>12. minor の指摘 (R-11〜R-25) を反映。question への回答を追記 (R-26, R-27) |

> **条番号について** (R-15 で修正): 本計画の条番号は 1800-2017 の構成を元にしており、計画作成時もレビュー時も規格本文は参照できなかった。
> 確度は次の 3 段階。
> - **1800-2023 での番号を間接的に確認できたもの** (Verilator のソースのコメントに「IEEE 1800-2023 x.y.z」として引用されている):
>   5.7.1, 5.9, 6.9.1, 6.12.1, 6.16, 6.16.9, 6.24.1, 6.24.3, 11.3.6, 11.4.12.1, 11.4.14, 11.5.1, 11.8.1, 11.10.3, 21.2.1.1, Annex I (`svdpi.h`)。
> - **番号だけを sv-tests のファイル名で確認できたもの** (1800-2017 の番号): 11.4.2, 11.4.5, 11.4.10, 11.4.11, 11.4.12, 11.7, 20.5, 20.9, 6.16.9〜6.16.15。
> - **未検証**: 11.4.3, 11.4.4, 11.4.6, 11.4.7, 11.4.8, 11.4.9, 11.6.1, 11.8.2, 6.8, 6.11.2, 9.4.2, 12.4, 12.5.1, 20.8.1, 21.2.1.3, 21.2.1.4。
>
> 表番号 (Table 11-xx) は未検証なので、**コードやテストのコメントには条番号だけを書き、表番号は書かない**。
> DPI の型 (`svLogicVecVal` 等) は「Annex I (svdpi.h)」を引用する (H.10.x の番号は未検証)。
> 規格が処理系依存としているもの、規格の記述を確認できなかったもの、他実装で挙動が分かれるものは本文で **[要VCS確認]** と書き、
> §11 の VCS 確認リストにまとめた。

---

## 1. 目的とスコープ

### 1.1 目的
M0 では、後続の全マイルストーンが依存する基盤を作る。

特に `svsim-logic` は、定数評価 (M1 sema)、インタプリタ (M1)、JIT (M5)、DPI (M3/M7)、デバッグ API (M4) のすべてが使う値の表現と演算である。
そのため M0 で**完全版**として作り、参照モデルとの差分テストで正しさを固める。

### 1.2 M0 でやること
| 項目 | 内容 |
|---|---|
| Cargo workspace | 4 crate を作る: `crates/svsim-common`、`crates/svsim-logic`、`crates/svsim-logic-ref` (テスト専用の参照モデル)、`crates/svsim-corpus`。ツールチェーンの固定と lint の設定も行う |
| `svsim-logic` | 4 値 / 2 値ビットベクタの表現と全演算 (§3): owned 値型、スライスビュー、≤64 ビットのスカラ高速パス、DPI 形式との変換、リテラル解析、実数・文字列との変換、表示用フォーマット、エッジ検出、`$clog2` |
| `svsim-logic-ref` | 参照モデル (差分テストのオラクル)。規格の真理値表を 1 ビットずつそのまま書く。本体とは別の実装エージェントが書く |
| `svsim-common` | ソースマップ、診断 (JSON と、単一行の人間向け表示)、文字列インターン、型付きインデックス配列、状態アリーナ `WordArena` の基礎 |
| CI | GitHub Actions: fmt、clippy、build、test、doc、ライセンス検査、コーパス取得、定期の長時間 proptest |
| コーパスマネージャ雛形 | マニフェスト (TOML)、git での取得とキャッシュ、テストケースの列挙 (sv-tests アダプタ)、段階評価の枠組み (M0 では全段階が `NotImplemented`)、結果の JSONL |

### 1.3 M0 でやらないこと (後のマイルストーンに回す)
| 項目 | 回す先 | 備考 |
|---|---|---|
| プリプロセッサ、パーサ、sv-tests のパース段階の計測 | M1 | M0 は「取得とファイル列挙」まで。段階評価の枠組みにパース段階を差し込めるようにしておく |
| 強度付き値 (strength) と解決関数 | M2 | `svsim-logic` の 4 値表現とは別の型にする (§3.11) |
| 一部の表示書式と `$display` 本体 (`real` の `%e %f %g`、`%s %c %t %m %v %p %u %z`、`%` 指定の解析、左詰め) | M1 | M0 は整数値の `%b %o %h %d` の数値部分の書式化だけ (§3.9) |
| ストリーミング演算子 (§11.4.14)、`inside` (§11.4.13)、struct/配列の集合演算 | M3 | 型情報が要る。M0 の基本演算 (連結、ワイルドカード等価) を組み合わせて作れる |
| unpacked 配列の要素の配置 (狭い要素を詰める方法) | M1 (配列) / M3 (DPI の open array) | §3.2 の配置の契約は packed 値 1 個に限る (R-3) |
| JIT 向けの `extern "C"` ヘルパと Cranelift のコード生成 | M5 | M0 ではその前提になる「メモリ配置の契約」と「スカラ式の仕様」(§3.2, §3.4) を固める |
| チェックポイント関連 (差分スナップショット、パーティション別アリーナ、キャッシュライン境界での確保) | M4 / M6 | M0 は全体コピーのスナップショットまで |
| 並列インターン (並列パース用) | M1 以降 | M0 は単一スレッド版 (§4.3) |
| 診断の人間向け表示の拡張 (複数行の span の描画、色、fix-it の表示) | M1 | 実際の表示要求はパーサができてから決める。M0 は JSON と単一行の表示だけ |
| コーパスの拡張 (`glob` アダプタ、`path =` のローカルソース、`corpus/private/` の自動読み込み、`--jobs N` の並列実行、`summary.md`、`--filter`) | M1〜M3 | マニフェストの形式 (`kind` で分ける形) は今のままにして、後から足せるようにする |
| Icarus / Verilator / VCS との出力比較 | M1 以降 | M0 の差分テストは参照モデルと num-bigint で行う。VCS 確認用のテスト集は M1 で作る (§11) |
| ベンチマーク | M5 で本格化 | M0 では作らない (正しさ優先) |

### 1.4 完了条件
1. `svsim-logic` の差分テストと proptest がすべて通る。
2. sv-tests を固定コミットで取得し、テストケースを列挙できる。

> **注**: パース段階の計測は M1 に回す (決定 U-2)。`docs/PLAN.md` §11 の M0 行は「sv-tests のパース段階を計測開始」のままなので、マネージャが更新する。

---

## 2. workspace 構成

### 2.1 ディレクトリ
```
Cargo.toml                    workspace (resolver = "3", edition 2024)
rust-toolchain.toml           channel = "1.97.0", components = ["rustfmt", "clippy"]   (決定 U-3)
rustfmt.toml                  edition = "2024" だけ
deny.toml                     cargo-deny (ライセンス許可リスト等)
.gitignore                    /target
.cargo/config.toml            alias: corpus = "run -p svsim-corpus --"
.github/workflows/ci.yml
crates/
  svsim-common/               依存: serde, serde_json, rustc-hash   (dev: svsim-logic)
  svsim-logic/                依存: なし  (dev: svsim-logic-ref, proptest)
  svsim-logic-ref/            依存: num-bigint, num-traits  (publish = false、テスト専用)
  svsim-corpus/               依存: serde, toml, serde_json, clap, walkdir  (dev: tempfile)。bin + lib
corpus/
  manifest.toml               公開コーパスのマニフェスト (最初のエントリは sv-tests)
```
- `svsim-logic` は**依存ゼロ**にする。JIT のランタイムや DPI の共有ライブラリ側からも軽くリンクできるようにするため。
  num-bigint は参照モデル (`svsim-logic-ref`) 側でだけ使う。
- `svsim-common` と `svsim-logic` は互いに依存しない。例外は、アリーナとロジック値ビューの結合テストのために、
  `svsim-common` の dev-dependency に `svsim-logic` を入れること。これは循環にならない。
- **unsafe の方針** (R-1):
  - workspace の lint は `unsafe_code = "deny"` にする。
  - 各 crate の `lib.rs` / `main.rs` の先頭に `#![forbid(unsafe_code)]` を書く。
  - `unsafe` を書くのは `tests/no_alloc*.rs` (確保回数を数える `GlobalAlloc` の実装) だけで、そのファイルの先頭に `#![allow(unsafe_code)]` と `// SAFETY:` を書く。
  - workspace で `forbid` にすると、`[lints]` がテストのターゲットにも効いて `allow` できず、E0453 でコンパイルできない (レビューで rustc 1.97.0 で確認済み)。
- 依存 crate はすべて MIT / Apache-2.0 互換。
  - MIT OR Apache-2.0: serde, serde_json, toml, clap, num-bigint, num-traits, proptest, rustc-hash, tempfile。
  - Unlicense OR MIT: walkdir。
  - globset は glob アダプタを後回しにしたので M0 では入れない。

### 2.2 workspace 設定
```toml
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
edition = "2024"
rust-version = "1.97"
license = "Apache-2.0 OR MIT"

[workspace.lints.rust]
missing_docs = "warn"          # clippy -D warnings と組み合わせて実質エラーにする。公開 API の doc コメントは必須 (CLAUDE.md §3)
unsafe_code = "deny"           # forbid にしない (R-1)。ルートの #![forbid] で締める

[workspace.lints.clippy]
undocumented_unsafe_blocks = "deny"

[workspace.dependencies]
# バージョンは M0-T1 で、その時点の最新安定版を選んで固定する (Cargo.lock をコミットする)
```
- 各 crate の `Cargo.toml` に `[lints] workspace = true` を書く (R-14)。書かないと workspace の lint が効かない。
- **並列実装の前提**: 依存 crate の追加は M0-T1 で全部済ませ、`Cargo.lock` を確定させる。
  以降のタスクは依存を追加しない。必要になったら報告し、マネージャが判断する。並列タスクが `Cargo.lock` を同時に書き換えないためである。

---

## 3. `svsim-logic` 設計

### 3.1 値の表現 (エンコーディング)
1 ビットを aval と bval の 2 ビットで表す。割り当ては VPI の `s_vpi_vecval` と、DPI の `svLogicVecVal` (Annex I, `svdpi.h`) と同じ。

| 値 | aval | bval | `Logic` (u8) |
|---|---|---|---|
| 0 | 0 | 0 | 0 |
| 1 | 1 | 0 | 1 |
| Z | 0 | 1 | 2 |
| X | 1 | 1 | 3 |

- `Logic` は `#[repr(u8)]` で値を `aval | (bval << 1)` にする。これは `svdpi.h` の `sv_0=0, sv_1=1, sv_z=2, sv_x=3` と一致するので、スカラの `svLogic` は変換なしで渡せる (レビューで Annex I の `svdpi.h` と照合済み)。
- 値が未知 (X または Z) かどうかは `bval` だけで判定できる。

### 3.2 メモリ配置 (配置の契約)
**適用範囲** (R-3): この契約が決めるのは **packed 値 1 個**の配置である。対象は、スカラの変数・ネット、packed 配列全体、packed 構造体全体、それに演算の一時値。

unpacked 配列の要素の配置はこの契約の対象外で、M1 (配列) / M3 (DPI の open array) で決める。方針は「狭い要素を密に詰める」で、例えば要素幅が 8/16/32 ビット以下ならプレーンごとに u8/u16/u32 で詰める。理由:
- この契約を unpacked 配列の要素にも適用すると、`logic [7:0] mem [N]` が 1 要素 16 バイト (必要量の 8 倍) になる。
- SoC のメモリモデルやレジスタファイルでは、メモリ量、キャッシュ効率、チェックポイントの量に直接効く。

カーネルは、狭い要素を層 1 (`L64` / `u64` の値渡し、§3.4) にロードして演算すればよいので、M0 の API は変えなくてよい。

以下、幅 `w` (ビット数、`u32`) の値のワード数を `nw = ceil(w / 64)` とする。ワードは `u64` で、ビット順はリトルエンディアン (ビット 0 = LSB = ワード 0 のビット 0)。

| 種別 | 配置 (u64 の並び) | ワード数 |
|---|---|---|
| 4 値 (`logic` 等) | `[a_0, …, a_{nw-1}, b_0, …, b_{nw-1}]` (**プレーン分離**) | `2·nw` |
| 2 値 (`bit` 等) | `[a_0, …, a_{nw-1}]` | `nw` |

- **正規化不変条件**: 最上位ワードの `w` ビット目以上は、両プレーンとも常に 0 にする。全演算は、正規化された入力から正規化された出力を作る。差分テストではこの不変条件も検査する。
- 幅 ≤ 64 も同じ規則に従う (4 値は `[a, b]` の 16 バイト、2 値は 8 バイト)。
- 2 値の配置は、リトルエンディアンの `u32` 配列と同じバイト列になる。そのため `svBitVecVal` は input 引数 (const ポインタ) に限り**ゼロコピー**で渡せる。
  - output / inout 引数は、関数から戻ったときに実引数へ代入する意味論 (代入でイベントが起きる、等) なので、ゼロコピーにできない。一時バッファに受け取り、最上位要素の未使用ビットをマスクしてから代入する (§3.8、R-20)。
  - リトルエンディアンでないターゲットは `abi.rs` で `compile_error!` にする (R-13)。M0 の CI は x86_64 だけ。
- 幅 0 は「空の値」(`nw = 0`) として表現だけを許す。0 回の複製は連結の中でだけ使え、そこで無視されるという規則 (§11.4.12.1) のため。
  連結以外の演算に幅 0 を渡すのは呼び出し側の誤りとし、`debug_assert!` で検出する。
- 最大幅は暫定で `MAX_WIDTH = 1 << 24` ビット。規格は、ベクタ長の上限を設けるなら少なくとも 2^16 ビットにすることを求めている (§6.9.1)。未決 U-6 (§10.1)。

#### 設計判断 D-1 (決定 U-1): 4 値は 64 ビットのプレーン分離にし、`svLogicVecVal` とはゼロコピーにしない
`svLogicVecVal` は `{uint32_t aval; uint32_t bval;}` の配列で、**32 ビットごとに aval と bval が交互に並ぶ**。これを内部表現にすると、次の問題がある。
- 64 ビットの値の aval を 1 回のロードで取れない。
- ワード並列の効率が半分になる。
- 2 値カーネルを aval プレーンにそのまま使えない。

一方、内部表現をプレーン分離にした場合の変換コストは、512 ビットの引数でも数 ns の規模で済む。

そこで**内部は 64 ビットのプレーン分離にし、DPI の境界で `svLogicVecVal` と変換する** (§3.8)。2 値 (`svBitVecVal`) は input に限りゼロコピーにする。
- この方針はレビューで代替案 (32 ビット交互、64 ビット交互、アリーナ全体の SoA) と比べて支持された。
- `LogicRef` はプレーンが隣接していなくてもよい形 (§3.3) にしたので、M5 でアリーナ全体の SoA 配置を試す余地も残る。
- PLAN.md §3 と §6.1 の記述は、「エンコーディングが一致し、配置は境界で変換する」という内容にマネージャが直す。

### 3.3 型と API の層
```
 層 1: スカラ高速パス (幅 ≤ 64)   scalar::L64 { a: u64, b: u64 } / u64       … JIT のコード生成の仕様、インタプリタの高速パス
 層 2: スライスカーネル (任意幅)    LogicRef / LogicMut / BitRef / BitMut を受ける自由関数  … アリーナ上で in-place
 層 3: owned 値型                LogicVec / BitVec (≤64 はインライン)           … 定数評価、インタプリタの一時値、テスト
```
- 層 3 は、層 1 と層 2 を呼ぶだけの薄い包み。
- 層 1 と層 2 は**ヒープ確保をしない**。乗除算、べき乗、書式化の作業領域は、呼び出し側が渡す `Scratch` を使う。
- アリーナ上での in-place 演算と、将来の JIT ヘルパ (`extern "C"`、M5) は層 2 を包む。
- 値は**符号の情報を持たない**。符号付き/符号なしは型の性質で、sema が決める (§11.8.1)。
  符号で結果が変わる演算 (除算、剰余、比較、算術右シフト、拡張、変換、`%d`) は `signed: bool` を引数に取る。
- 2 値と 4 値は**型を分ける** (ジェネリクスにしない)。理由:
  1. JIT から呼ぶ関数は、単相のほうが C ABI にしやすい。
  2. 4 値の算術は「bval に 1 があれば全 X、無ければ aval プレーンに 2 値カーネルを適用」と書けるので、内部の 2 値ワード演算 (`words.rs`) を共有すれば重複は小さい。
  3. 型の混在は §3.5.1 の規則で扱う。

#### 主要な型 (シグネチャ例)
```rust
/// 1 ビットの 4 値。repr は svdpi.h の sv_0/sv_1/sv_z/sv_x と一致。
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Logic { L0 = 0, L1 = 1, Z = 2, X = 3 }
// Display は '0' '1' 'z' 'x'

pub const MAX_WIDTH: u32 = 1 << 24;
pub const fn words_for(width: u32) -> usize;          // ceil(width/64)
pub const fn top_mask(width: u32) -> u64;             // 最上位ワードの有効ビットのマスク (width%64==0 なら !0)

/// 4 値の読み取りビュー。a と b の長さはどちらも words_for(width)。プレーンは隣接していなくてよい。
#[derive(Clone, Copy)]
pub struct LogicRef<'a> { width: u32, a: &'a [u64], b: &'a [u64] }
pub struct LogicMut<'a> { width: u32, a: &'a mut [u64], b: &'a mut [u64] }
#[derive(Clone, Copy)]
pub struct BitRef<'a> { width: u32, a: &'a [u64] }
pub struct BitMut<'a> { width: u32, a: &'a mut [u64] }

impl<'a> LogicRef<'a> {
    /// プレーン分離配置 [a.., b..] のワード列から作る (アリーナ上の信号用)。len == 2*words_for(width)。
    pub fn from_planar(width: u32, words: &'a [u64]) -> Self;
    pub fn width(&self) -> u32;
    pub fn aval(&self) -> &'a [u64];
    pub fn bval(&self) -> &'a [u64];
    pub fn bit(&self, i: u32) -> Logic;
    pub fn has_unknown(&self) -> bool;     // bval に 1 がある
}
impl<'a> LogicMut<'a> {
    pub fn from_planar(width: u32, words: &'a mut [u64]) -> Self;
    pub fn reborrow(&mut self) -> LogicMut<'_>;
    pub fn as_ref(&self) -> LogicRef<'_>;
    pub fn fill(&mut self, v: Logic);
    pub fn set_bit(&mut self, i: u32, v: Logic);
    pub fn copy_from(&mut self, src: LogicRef<'_>);   // 幅は一致していること
}

/// 4 値の owned 値。幅 ≤ 64 はヒープ確保しない。
#[derive(Clone, PartialEq, Eq, Hash)]   // PartialEq は「全ビットが同一 (===)」の意味
pub struct LogicVec { width: u32, store: Store }
enum Store { Inline { a: u64, b: u64 }, Heap(Box<[u64]>) /* プレーン分離、len = 2*nw */ }
pub struct BitVec  { width: u32, store: Store2 }
enum Store2 { Inline(u64), Heap(Box<[u64]>) }

impl LogicVec {
    pub fn zero(width: u32) -> Self;
    pub fn filled(width: u32, v: Logic) -> Self;
    pub fn from_u64(width: u32, v: u64) -> Self;      // 幅に切り詰める
    pub fn from_i64(width: u32, v: i64) -> Self;      // 2 の補数で幅に切り詰める/符号拡張する
    /// "10xz_01" (MSB が先頭、'_' は無視)。テスト・デバッグ用。参照モデルとの受け渡しにも使う (§5.1)
    pub fn from_bits_str(s: &str) -> Result<Self, ParseBitsError>;
    pub fn to_bits_string(&self) -> String;           // MSB が先頭、'_' なし、小文字 x/z。from_bits_str と往復する
    pub fn as_ref(&self) -> LogicRef<'_>;
    pub fn as_mut(&mut self) -> LogicMut<'_>;
    pub fn to_u64(&self) -> Option<u64>;              // X/Z がある、または 64 ビットを超える位置に 1 があれば None
    pub fn to_i64(&self, signed: bool) -> Option<i64>;
    pub fn to_bit_vec(&self) -> BitVec;               // X/Z → 0 (§6.11.2)
}
impl From<&BitVec> for LogicVec { /* bval = 0 */ }

/// 乗除算、べき乗、10 進書式化などの作業領域。呼び出し側が使い回す (ホットループで確保しないため)。
pub struct Scratch { buf: Vec<u64> }
```
- `Debug` は `8'b10xz_0001` の形、`Display` は 2 進 (`%b` 相当) にする。
- 層 3 の便利メソッド (`a.add(&b)` など) は、各演算モジュールの中に `impl LogicVec { … }` ブロックとして書く。同じ crate 内なら impl ブロックを複数のファイルに分けられるので、並列タスクが同じファイルを触らずに済む。

#### スライスカーネルの共通規約
```rust
// 例: 文脈決定 (context-determined) の 2 項演算。dst, x, y の幅はすべて同じであること。
pub fn add(dst: LogicMut<'_>, x: LogicRef<'_>, y: LogicRef<'_>);
pub fn add2(dst: BitMut<'_>, x: BitRef<'_>, y: BitRef<'_>);      // 2 値版は名前の末尾に 2
pub fn mul(dst: LogicMut<'_>, x: LogicRef<'_>, y: LogicRef<'_>, s: &mut Scratch);
pub fn lt(x: LogicRef<'_>, y: LogicRef<'_>, signed: bool) -> Logic;
pub fn lt2(x: BitRef<'_>, y: BitRef<'_>, signed: bool) -> bool;
```
1. **幅の規則は呼び出し側 (sema / MIR) が適用する** (§3.6)。
   - 文脈決定の演算では、呼び出し側がオペランドを先に結果幅へ拡張 (§3.5(e) `resize`) してから、同じ幅のカーネルを呼ぶ。
   - カーネルは幅が揃っていることを `assert!` で検査する。比較 1 回なので release ビルドでも残す。
   - 自己決定 (self-determined) のオペランド (シフト量、べき乗の指数、連結の要素、選択のインデックス) だけは、別の幅を受け付ける。
2. Rust の借用規則により、`dst` は入力と重ならない。
   - 頻繁に in-place で使う演算 (ビット演算、加減算、シフト) には `and_assign(dst_x: LogicMut, y: LogicRef)` のような `_assign` 版を用意する。
   - アリーナの同じバッファから dst と src を同時に取るときは `WordArena::disjoint_mut` (§4.4) を使う。これは `<[u64]>::get_disjoint_mut` で実装する (rustc 1.97.0 で使えることをレビューで確認済み)。
3. カーネルは panic しない (上記の幅の assert を除く)。0 除算などの意味論上の例外は、値 (X) で返す。

### 3.4 スカラ高速パス (幅 ≤ 64) とその式
`scalar` モジュールに、`L64 { a: u64, b: u64 }` (4 値) と `u64` (2 値) に対する演算を、`#[inline]` の自由関数として置く。
- 幅 `w` (1〜64) を引数に取り、結果を `m = top_mask(w)` でマスクする。
- 用途は 2 つある。
  1. インタプリタと `LogicVec` の、幅 ≤ 64 の高速パス。
  2. **M5 の JIT が Cranelift IR で生成する式の仕様**。JIT の出力はこの関数と差分テストする。
- **層 1 は、レジスタ上の値に対する演算の仕様であり、記憶域の配置とは独立している** (R-26)。アリーナからのロード/ストア (プレーン分離、将来の狭い要素の詰め方) は別に定める。これにより、M5 以降で記憶域の配置だけを変える余地が残る。

主な式 (x = (ax, bx), y = (ay, by)、`!` はビット反転。式はレビューで 16 通りの入力すべてを手計算して確認済み):

| 演算 | aval | bval | 根拠 |
|---|---|---|---|
| `~x` | `(!ax \| bx) & m` | `bx` | §11.4.8 |
| `x & y` | `A = (ax\|bx) & (ay\|by)` | `A & (bx\|by)` | §11.4.8 (0 が勝つ) |
| `x \| y` | `ax\|bx\|ay\|by` | `(bx\|by) & !((ax&!bx)\|(ay&!by))` | §11.4.8 (1 が勝つ) |
| `x ^ y` | `((ax^ay) \| B) & m` | `B = bx\|by` | §11.4.8 |
| `x ~^ y` | `(!(ax^ay) \| B) & m` | `B = bx\|by` | §11.4.8 |
| `c ? t : f` (c が曖昧) | `((at & S) \| !S) & m` | `!S & m` | §11.4.11 (S = `!(at^af) & !(bt^bf) & !bt`) |
| 算術 `+ - * / % **` | 未知 (`bx\|by != 0`) なら `m`、そうでなければ 2 値の演算結果 `& m` | 未知なら `m`、そうでなければ `0` | §11.4.3 |

- 1 ビットの結果を返す演算 (比較、リダクション、論理演算) は `Logic` を返す。
- Z は、入力としては X と同じに扱う (aval を見ずに bval で判定する)。そのため、ビット演算の結果に Z は出ない (§11.4.8 の表で、z の入力は x と同じ行)。
  ただし、**データを移すだけの演算** (シフト、連結、複製、選択、拡張、および条件が確定している `?:`) は Z を Z のまま運ぶ。

### 3.5 演算一覧 (規格の意味論)
凡例:
- **CD** = 文脈決定。全オペランドと結果が同じ幅 `w` で、呼び出し側で拡張済み。
- **SD** = 自己決定。

「X 規則」の列は 4 値版の規則。2 値版は入力に X/Z が無いので該当しない。ただし、規格上の結果が X になる場合 (0 除算など) の 2 値版の扱いは各表に明記した。自己決定のオペランドだけが 4 値の場合は §3.5.1 に従う。

#### (a) ビット演算・リダクション・論理演算 (タスク M0-T4)
| 演算 | 関数 | 幅 | X 規則 | 条 |
|---|---|---|---|---|
| `~` | `not` | CD | 表どおり (§3.4) | §11.4.8 |
| `& \| ^ ~^ ^~` | `and or xor xnor` | CD | 表どおり | §11.4.8 |
| `& ~& \| ~\| ^ ~^` (単項) | `reduce_and/nand/or/nor/xor/xnor` → `Logic` | 入力は SD、結果は 1 ビット | AND: 既知の 0 が 1 つでもあれば 0、無くて未知があれば X。OR: 既知の 1 があれば 1、無くて未知があれば X。XOR: 未知があれば X、無ければパリティ | §11.4.9 |
| 真理値 (`if (e)` 等) | `truth(x) -> Logic` | SD | 既知の 1 があれば 1、全ビットが既知の 0 なら 0、それ以外は X | §11.4.7, §12.4 |
| `! && \|\| -> <->` | `Logic::{not, and, or, implies, equiv}` (`truth` の結果に適用する)。多ビットの `!` は `logical_not(x) = truth(x).not()` | 各オペランドは SD、結果は 1 ビット | `&&` は AND の表、`\|\|` は OR の表。`a -> b` = `!a \|\| b`、`a <-> b` = `(a -> b) && (b -> a)` | §11.4.7 |
| `$countones` `$countbits` `$onehot` `$onehot0` `$isunknown` | `count_ones`, `count_bits(x, set: LogicSet)`, `onehot`, `onehot0`, `is_unknown` | SD | `$countbits(e, ...)` は、指定した値 (0/1/X/Z の集合) を持つビットの数。`$onehot(e)` = `$countbits(e,'1)==1`、`$onehot0` = `$countbits(e,'1)<=1`、`$isunknown` = `$countbits(e,'x,'z)!=0` | §20.9 |

- 短絡評価 (§11.3.5) は評価器の責務。ライブラリは両オペランドの真理値を受け取るだけ。

#### (b) 比較・等価 (タスク M0-T4)
| 演算 | 関数 | 幅 | X 規則 | 条 |
|---|---|---|---|---|
| `==` `!=` | `eq`, `ne` → `Logic` | 入力は CD (呼び出し側で max 幅に拡張)、結果は 1 ビット | 既知ビットに不一致があれば 0 (`!=` は 1)。不一致が無く未知ビットがあれば X。全ビット既知で一致すれば 1 | §11.4.5 |
| `===` `!==` | `case_eq`, `case_ne` → `bool` | 同上 | aval・bval とも完全に一致すれば真 (X は X とだけ、Z は Z とだけ一致) | §11.4.5 |
| `==?` `!=?` | `wildcard_eq`, `wildcard_ne` → `Logic` | 同上 | **右**オペランドの X/Z ビットはワイルドカードで、比較しない。残りのビットで判定し、既知の不一致があれば 0、左に未知があれば X、それ以外は 1 | §11.4.6 |
| `casez` / `casex` の照合 | `casez_match`, `casex_match` → `bool` | 同上 | casez: どちらかが Z のビット (と `?`) は無視し、残りを `===` で比べる。casex: どちらかが X/Z のビットは無視する | §12.5.1 |
| `< <= > >=` | `lt le gt ge (x, y, signed)` → `Logic` | 同上 | どちらかに未知ビットがあれば X。符号付きで比較するのは**両方が符号付きのときだけ** (`signed` は呼び出し側が決める) | §11.4.4, §11.8.1 |

- `case` 文の照合は `case_eq` を使う (§12.5)。`inside` (§11.4.13) は、`wildcard_eq` と比較を組み合わせて M3 で作る。

#### (c) 算術 (タスク M0-T5)
| 演算 | 関数 | 幅 | X 規則 / 特殊値 | 条 |
|---|---|---|---|---|
| `+ -` (2 項) | `add`, `sub` | CD | どこかに未知ビットがあれば全 X。結果は幅 `w` で 2 の補数として切り詰める (オーバフローは捨てる) | §11.4.3 |
| `-` `+` (単項) | `neg` (`0 - x`) / 恒等 | CD | 同上 | §11.4.3 |
| `++ --` | `inc`, `dec` (`_assign` 版) | CD | 同上 | §11.4.2 |
| `*` | `mul(…, &mut Scratch)` | CD | 同上。下位 `w` ビットだけを計算する (符号付きでも符号なしでも下位ビットは同じなので `signed` は不要) | §11.4.3 |
| `/` | `div(…, signed, &mut Scratch)` | CD | 同上に加えて、**除数が 0 なら全 X**。0 方向へ切り捨てる。符号付きの最小値 / -1 は 2 の補数で折り返す (結果は最小値) [要VCS確認 V-1] | §11.4.3 |
| `%` | `rem(…, signed, &mut Scratch)` | CD | 同上に加えて、除数が 0 なら全 X。結果の符号は第 1 オペランドの符号 | §11.4.3 |
| `**` | `pow(dst, base, base_signed, exp: LogicRef, exp_signed, &mut Scratch)` | 結果は base の幅 (CD)、指数は SD | どちらかに未知があれば全 X。指数が負 (指数が符号付きで負) のとき: base=0 なら全 X、base=1 なら 1、base=-1 (base が符号付きのとき) なら指数が偶数で 1・奇数で -1、\|base\|>1 なら 0。指数が 0 なら 1 (base=0 でも 1) | §11.4.3 (べき乗の規則表) |

- 2 値版 (`div2`, `rem2`) で除数が 0 の場合: 規格上の結果は X で、2 値の型に格納すると 0 になる (§6.11.2)。そのため **0 を返す** [要VCS確認 V-2]。`pow2` で base=0・指数が負の場合も同じく 0。
- べき乗は、`w` ビットの剰余環の上で二乗法で計算する。指数が `w` 以上で base が偶数なら、0 を早期に返してよい。
- 多倍長の除算は Knuth の Algorithm D (`u64` ワード、`u128` の中間値) で行う。符号付きは絶対値で計算してから符号を戻す。最小値の絶対値は `w` ビットに収まらないので、符号なしとして扱う。
- `Scratch` に必要なワード数の目安は、`div` が `2·nw + 1`、`mul` が `nw`。`Scratch::reserve_for(width)` で事前に確保できる。
- べき乗の特殊ケース、除算、剰余の規則は、Verilator と slang の実装と一致することをレビューで確認済み。

#### (d) シフト (タスク M0-T6)
| 演算 | 関数 | 幅 | 規則 | 条 |
|---|---|---|---|---|
| `<<` `<<<` | `shl(dst, x, amt: LogicRef)`, `shl_const(dst, x, n: u64)` | x と結果は CD。シフト量は SD で、常に符号なし | シフト量に未知ビットがあれば全 X。シフト量が `w` 以上なら全 0。空いたビットは 0。x の X/Z はそのまま移動する | §11.4.10 |
| `>>` | `shr` / `shr_const` | 同上 | 空いたビットは 0 | §11.4.10 |
| `>>>` | `ashr(dst, x, amt, signed)` / `ashr_const` | 同上 | 結果の型が符号付きなら、符号ビットの値 (X/Z を含めてそのまま) で埋める。符号なしなら `>>` と同じ。シフト量が `w` 以上なら、全ビットが符号ビットの値になる | §11.4.10 |
- シフト量が 64 ビットより広い場合も、上位ワードに 1 があれば「`w` 以上」として扱う (切り詰めない)。
- 符号ビットが Z のとき `>>>` で Z を埋めるのは、「符号ビットの値で埋める」の字義どおりの解釈 (slang と一致) [要VCS確認 V-3]。

#### (e) 幅の変更・連結・複製・選択 (タスク M0-T6)
| 演算 | 関数 | 規則 | 条 |
|---|---|---|---|
| 拡張・切り詰め | `resize(dst, x, signed)` (目標幅は dst の幅) | 切り詰めは下位ビットを残す。拡張は、符号なしなら 0 で埋め、符号付きなら符号ビットの値 (aval・bval とも) を複製する。符号ビットが X なら X、Z なら Z で埋まる | §11.6.1, §11.8.2, §11.8.4, §6.24.1 (サイズキャスト) |
| 4 値 → 2 値 | `to_two_state(dst: BitMut, x)` | X/Z → 0 | §6.11.2 |
| 2 値 → 4 値 | `from_two_state(dst: LogicMut, x: BitRef)` | bval = 0 | §6.11.2 |
| 連結 `{a, b, …}` | `concat(dst, parts: &[LogicRef])` | 先頭の要素が MSB 側。dst の幅は各要素の幅の和。幅 0 の要素は無視する | §11.4.12 |
| 複製 `{n{a}}` | `replicate(dst, x, n: u32)` | n = 0 は幅 0 の値になる (連結の中でだけ許される。その検査は sema が行う) | §11.4.12.1 |
| ビット選択・部分選択 (読み) | `get_slice(dst, src, idx: Index)`、`get_bit(src, idx: Index) -> Logic` (dst の幅 = 選択幅) | `Index::Value(offset)` の `offset` は、LSB を 0 とする物理オフセット (`i64`)。宣言範囲 `[msb:lsb]` や `+:` `-:` からの変換は sema/MIR の責務。範囲外のビットは **4 値なら X、2 値なら 0**。完全に範囲外でも同じ。`Index::Unknown` なら全ビットが範囲外と同じ扱い | §11.5.1 |
| 同 (書き) | `set_slice(dst, idx: Index, src)`, `set_bit` | 範囲内のビットだけを書き、範囲外は無視する。`Index::Unknown` なら何も書かない | §11.5.1 |
| インデックスの解釈 | `index_value(idx: LogicRef, signed) -> Index` (`enum Index { Unknown, Value(i64) }`) | インデックスに X/Z があれば `Unknown`。`i64` に収まらない値は飽和させる (どのみち範囲外になる) | §11.5.1 |
| 実数の選択 | (なし) | 実数へのビット選択は不正 (§11.5.1)。sema で弾く | — |
| `$clog2` | `clog2(x: LogicRef) -> Option<u32>` (R-8) | 引数は符号なしとして扱い、`ceil(log2(x))` を返す。`$clog2(0)` = 0、`$clog2(1)` = 0。X/Z を含めば `None` で、結果をどうするか (定数式なら診断、実行時なら X) は呼び出し側が決める [要VCS確認 V-10]。幅は任意 (多倍長でも正確に) | §20.8.1 |
- `$clog2` は `ops/math.rs` に置く (タスク M0-T6)。

#### (f) 条件演算子 (タスク M0-T4)
| 演算 | 関数 | 規則 | 条 |
|---|---|---|---|
| `c ? t : f` で c が X/Z | `cond_merge(dst, t, f)` | ビットごとに、両方 0 なら 0、両方 1 なら 1、それ以外 (Z と Z も含む) は X | §11.4.11 |
| 同 (t, f が 2 値) | `cond_merge_bits(dst: LogicMut, t: BitRef, f: BitRef)` (§3.5.1) | 値が違うビットは X。結果は 4 値のまま返し、2 値の変数に代入するときに 0 になる | §11.4.11, §3.5.1 |
- 条件の真理値は `truth(c)` で求める。真なら `t`、偽なら `f` を評価器が選び、X のときだけ `cond_merge` を呼ぶ。t と f の幅は呼び出し側で揃える。
- 整数型でないオペランド (実数など) の扱い (§11.4.11) は評価器の責務。
- **z/z → X** (R-10) は規格の表を優先した挙動。他実装の挙動は分かれる。Icarus は同じ値のビットをそのまま残すので z?z → z、slang は両方の値全体が一致すればそのまま残す。**Icarus との既知の差**として記録し、ゴールデンテストに `1'bx ? 1'bz : 1'bz` を入れて [要VCS確認 V-8] の印を付ける。

#### (g) リテラル・整数変換 (タスク M0-T7)
| 機能 | 関数 | 規則 | 条 |
|---|---|---|---|
| 基数付きリテラル (2/8/16 進) | `parse_based(size: Option<u32>, signed: bool, radix: Radix, digits: &str) -> Result<Literal, LiteralError>` | `_` は無視し、`?` は Z として扱う。**桁から得たビット数**が size より多ければ左を切り詰め (警告フラグを立てる)、少なければ左を 0 で埋める。ただし**最左ビットが X/Z なら、その値で左を埋める**。size が 0 ならエラー | §5.7.1 |
| 基数付きリテラル (10 進) | 同 (`Radix::Dec`) | 数字列の**値**がサイズに収まらなければ上位を切り詰める (`4'd20` → 4、警告フラグ)。X/Z は、`x`/`z`/`?` の 1 桁 (後ろに `_` が続いてもよい: `8'dx_`、`8'd?`) のときだけ許し、全ビットがその値になる (R-11) | §5.7.1 |
| 符号付きのサイズ付きリテラル | 同 (`signed = true`) | リテラルの段階でも 0 で左を埋める (`4'sb1` → `0001`)。符号拡張は文脈で行う (R-11) | §5.7.1 |
| サイズなしリテラル | 同 (`size = None`) | 幅は 32 ビット以上。32 ビットを超える値は必要な幅まで広げ、警告フラグを立てる (Verilator と同じ挙動) [要VCS確認 V-11]。最左が X/Z のものは `Literal::xz_extend = true` を返し、文脈幅への拡張は sema が行う | §5.7.1 |
| 10 進のサイズなし整数 | `parse_unsized_decimal(digits)` | 符号付きで 32 ビット以上 | §5.7.1 |
| 基数なしのサイズなし `'0 '1 'x 'z` | `LogicVec::filled(w, v)` | 文脈幅の全ビットをその値で埋める (幅は sema が決める)。サイズを付けるのは構文エラー (パーサの仕事) | §5.7.1 |
```rust
pub enum Radix { Bin, Oct, Dec, Hex }
pub struct Literal { pub value: LogicVec, pub signed: bool, pub sized: bool,
                     pub xz_extend: bool, pub truncated: bool, pub widened_unsized: bool }
```

#### (h) 文字列との変換 (タスク M0-T7、itoa 系だけ M0-T8)
| 機能 | 関数 | 規則 | 条 |
|---|---|---|---|
| 文字列リテラル → 整数 | `from_string_bytes(dst, bytes: &[u8])` | 1 文字を 8 ビットにし、最後の文字を LSB 側に置く (右詰め)。dst が広ければ左を 0 で埋め、狭ければ左 (先頭側の文字) を切り詰める。空の文字列 `""` は 1 文字の `"\0"` (8 ビットの 0) として扱う。dst の幅を決めるのは sema (R-11) | §5.9, §11.10, §11.10.3 |
| 整数 → `string` 型 | `to_string_bytes(x) -> Vec<u8>` | MSB 側から 8 ビットずつ文字にする。幅が 8 の倍数でなければ、左を 0 で埋める。値が 0 のバイト (`"\0"`) は捨てる。X/Z を含むバイトの扱いは暫定で、X/Z を 0 として扱う [要VCS確認 V-5] | §6.16, §6.24.1 |
| `atoi atohex atooct atobin` | `atoi(s: &[u8], radix: Radix) -> BitVec` (幅 32。§3.5.1 により `integer` 型には 4 値として格納されるが、X/Z は出ない) | 先頭から、その基数の数字と `_` を読み、他の文字か終端で止まる。数字が無ければ 0。**符号 (`-` `+`)、先頭の空白、`0x` などの接頭辞は解釈しない** (規格の文言、Icarus と同じ。R-9)。32 ビットを超える分は黙って切り詰める [要VCS確認 V-6] | §6.16.9 |
| `itoa hextoa octtoa bintoa` | `fmt::itoa(x, signed, radix) -> Vec<u8>` | `%0d %0h %0o %0b` と同じ書式 (§3.9) で生成する | §6.16.11〜§6.16.14 |
- `string` 型そのもの (メソッド群、比較、連結) は M1/M3 のランタイムの仕事。M0 で作るのは、整数値との変換部分だけ。
- `atoreal` と `realtoa` (§6.16.10, §6.16.15) は、実数の書式化と一緒に M1 で作る。

#### (i) 実数との変換 (タスク M0-T8)
| 機能 | 関数 | 規則 | 条 |
|---|---|---|---|
| 整数 → 実数 (暗黙の変換、`$itor`) | `to_f64(x: LogicRef, signed) -> f64`, `to_f32(x, signed) -> f32` | X/Z のビットは 0 として扱う。仮数に収まらない値は、IEEE 754 の最近接偶数丸めで**多倍長の値から直接 1 回だけ**丸める。`to_f32` も f64 を経由しない (二重丸めを避ける。R-12) | §6.12.1, §20.5 |
| 実数 → 整数 (暗黙の変換、キャスト) | `from_f64_round(dst, v: f64) -> RealToIntStatus` | **最も近い整数に丸める。ちょうど半分のときは 0 から遠い方へ** (例: 35.5 → 36、-1.5 → -2)。結果は dst の幅で 2 の補数として切り詰める。dst の幅は任意で、64 ビットを超えても正確に変換する | §6.12.1 |
| `$rtoi` | `from_f64_trunc(dst, v)` | 0 方向へ切り捨てる | §20.5 |
| NaN / ±∞ → 整数 | 同上 (`RealToIntStatus::NotFinite`) | 規格の記述が見当たらない。暫定で 0 を返し、状態を返す [要VCS確認 V-4] | — |
| `$realtobits` / `$bitstoreal` | `f64::to_bits` / `from_bits` の包み (64 ビット) | ビットパターンをそのまま使う | §20.5 |
| `$shortrealtobits` / `$bitstoshortreal` | 同 (32 ビット) | 同 | §20.5 |

#### (j) 表示用フォーマット (タスク M0-T8)
§3.9 で詳述する。

#### (k) エッジ検出 (タスク M0-T4、R-8)
```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Edge { None, Pos, Neg }
pub fn edge(old: Logic, new: Logic) -> Edge;               // 1 ビットの遷移
pub fn edge_lsb(old: LogicRef<'_>, new: LogicRef<'_>) -> Edge;  // 式の値の LSB で判定
pub fn edge2(old: bool, new: bool) -> Edge;                // 2 値
```
| old \ new | 0 | 1 | X | Z |
|---|---|---|---|---|
| 0 | None | **Pos** | **Pos** | **Pos** |
| 1 | **Neg** | None | **Neg** | **Neg** |
| X | **Neg** | **Pos** | None | None |
| Z | **Neg** | **Pos** | None | None |
- §9.4.2 の遷移表どおりにする。posedge は 0→1/x/z と x/z→1、negedge は 1→0/x/z と x/z→0。x↔z の遷移はエッジではない。
- `edge` イベント (§9.4.2) は「Pos または Neg」で、評価器が判定する。
- 多ビットの式では LSB の遷移だけを見る (§9.4.2)。
- 層 1 (`scalar`) にも同じ関数を置き、JIT の仕様にする。
- [要VCS確認 V-12]

### 3.5.1 型の混在: 2 値のデータに 4 値の自己決定オペランドが来る式 (R-2)
**問題**: 文脈決定のオペランドなら、sema が 4 値に揃えればよい (2 値 → 4 値の変換は情報を失わない)。
しかし**自己決定のオペランド** (シフト量、べき乗の指数、選択のインデックス、`?:` の条件) は、データ側が 2 値でも X/Z を持ちうる。そして、その X/Z が結果を決める。
例: `bit [7:0] b; logic [2:0] s = 3'bx;` のとき、`b << s` は規格上 X になる。ところが sema がシフト量を 2 値に変換すると `s = 0` になり、結果は `b` になってしまう。

**規則** (マネージャ決定):
1. **評価の値域**: オペランドのどれか (自己決定のものを含む) が 4 値型なら、その演算は **4 値カーネルで評価**し、結果の値も 4 値で持つ。
2. **結果の型**: 幅と符号は規格どおりに決める。例えば、シフトの結果の幅と符号は左オペランドから決まる (§11.6.1, §11.8.1)。この場合、結果の値は 4 値の一時値で持つ。
3. **2 値化の時期**: X/Z → 0 の変換 (§6.11.2) は、**2 値型の変数への代入時** (および 2 値型の引数・ポートへの受け渡し時) にだけ行う。式の途中で 4 値の値が別の演算に渡れば、X はそのまま伝わる。例: `(b << s) + 1` は全 X になり、`int` に代入すると 0 になる。
4. **sema はオペランドを勝手に 2 値化しない**。自己決定の 4 値オペランドに 2 値への変換を挿入してはならない。M1 の sema の計画に引き継ぐ。
5. **例外: 選択**。2 値のベクタを X/Z のインデックスで読んだ値は、§11.5.1 が「2 値なら 0」と明記している。そのため、結果は 2 値の 0 になり、4 値にしない。範囲外も同じ。

**混在カーネル** (変換用の一時値を作らずに直接計算するためのもの。4 値のデータに対する同名のカーネルと結果は同じ):
```rust
// シフト: 2 値データ × 4 値シフト量 → 4 値結果 (シフト量に X/Z があれば全 X、無ければ bval = 0 のシフト結果)
pub fn shl_bits(dst: LogicMut<'_>, x: BitRef<'_>, amt: LogicRef<'_>);
pub fn shr_bits(dst: LogicMut<'_>, x: BitRef<'_>, amt: LogicRef<'_>);
pub fn ashr_bits(dst: LogicMut<'_>, x: BitRef<'_>, amt: LogicRef<'_>, signed: bool);
// べき乗: 2 値の base × 4 値の指数 → 4 値結果
pub fn pow_bits(dst: LogicMut<'_>, base: BitRef<'_>, base_signed: bool,
                exp: LogicRef<'_>, exp_signed: bool, s: &mut Scratch);
// 4 値 base × 2 値の指数は、指数を from_two_state するだけで済むので混在カーネルは作らない
// ?: で条件が X: 2 値の t, f → 4 値結果 (違うビットは X)
pub fn cond_merge_bits(dst: LogicMut<'_>, t: BitRef<'_>, f: BitRef<'_>);
// 選択 (例外): 2 値 × Index (Unknown は 0) → 2 値結果
pub fn get_slice2(dst: BitMut<'_>, src: BitRef<'_>, idx: Index);
pub fn set_slice2(dst: BitMut<'_>, idx: Index, src: BitRef<'_>);   // Unknown なら書かない
```
- 全オペランドが 2 値のときは、2 値のカーネル (`shl2`, `pow2`, …) を使う。シフト量・指数・インデックスは `BitRef` で受ける。
- 混在カーネルは、`from_two_state` してから 4 値カーネルを呼んだ結果と一致することを差分テストで確認する。
- この規則で VCS と結果が合うかは [要VCS確認 V-9]。特に `$display("%b", b << s)` で x が表示されるか (中間値が 4 値か) を確認する。

### 3.6 符号と幅の規則 (呼び出し側の責務。R-7 で修正)
ライブラリは幅と符号を決めない。呼び出し側 (M1 の sema、定数評価、MIR 生成) が守るべき規則を、`svsim-logic` の crate ドキュメントに次の表として書く (規則の実装は M1)。

**式のビット長** (§11.6.1):
| 式 | 結果のビット長 | オペランドの扱い |
|---|---|---|
| サイズなしの定数 | 32 以上 (§5.7.1) | — |
| サイズ付きの定数 | 指定のサイズ | — |
| `i op j` (`op`: `+ - * / % & \| ^ ^~ ~^`) | max(L(i), L(j)) | i, j とも文脈決定 |
| `op i` (`op`: `+ - ~`) | L(i) | i は文脈決定 |
| `i op j` (`op`: `=== !== == != ==? !=? > >= < <=`) | 1 | i, j を max(L(i), L(j)) に拡張する。この拡張は比較の中だけで、外の文脈は入らない |
| `i op j` (`op`: `&& \|\| -> <->`) | 1 | i, j とも**自己決定** |
| `op i` (`op`: `& ~& \| ~\| ^ ~^ ^~ !`) | 1 | i は**自己決定** |
| `i op j` (`op`: `>> << >>> <<< **`) | L(i) | i は文脈決定、j は**自己決定** |
| `i ? j : k` | max(L(j), L(k)) | i は**自己決定**、j と k は文脈決定 |
| `{i, …, j}` | L(i) + … + L(j) | すべて**自己決定** |
| `{n{i, …, j}}` | n × (L(i) + … + L(j)) | すべて**自己決定** |

- **代入文脈**: 代入では、左辺の幅も文脈に入る。右辺の文脈決定のオペランドは、max(右辺の幅, 左辺の幅) に拡張してから評価し、最後に左辺の幅に切り詰める (§11.6.1, §11.8.2)。
- **符号** (§11.8.1):
  - 結果が符号付きになるのは、文脈決定のオペランドがすべて符号付きのときだけ。
  - 比較・等価・論理演算・リダクションの結果は符号なし (1 ビット)。比較では、両オペランドが符号付きのときだけ符号付きで比べる。
  - 連結、複製、ビット選択、部分選択の結果は、常に符号なし。
  - シフトとべき乗の結果の符号は、左オペランド (i) で決まる。j は自己決定なので影響しない。
  - `?:` の結果は、j と k の両方が符号付きのときだけ符号付き。
  - 自己決定のオペランドは、自分の型のまま評価する。
- **評価の手順** (§11.8.2): 式全体の型と幅を先に決め、文脈決定のオペランドをその幅に拡張してから評価する。拡張は、拡張する**前のオペランドの**符号で行う。
- `$signed` と `$unsigned` (§11.7) は値を変えない。ライブラリでは何もしない。
- 2 値と 4 値の混在は §3.5.1 に従う。
- L-8 (受け入れ基準) で、この表が crate ドキュメントにあることを確認する。

### 3.7 モジュール構成とファイル (並列実装のための分割)
```
crates/svsim-logic/src/
  lib.rs            モジュール宣言と再エクスポート (T3a が全モジュールを宣言しておく)
  logic.rs          Logic (スカラ)                                   T3a
  layout.rs         words_for, top_mask, MAX_WIDTH, 正規化              T3a
  view.rs           LogicRef/LogicMut/BitRef/BitMut                  T3a
  vec.rs            LogicVec/BitVec (構築、変換、Debug/Display、bits 文字列)  T3a
  scratch.rs        Scratch                                         T3a
  scalar.rs         L64 と共通の補助関数 (マスク、符号拡張)。各演算のスカラ版は各演算ファイル内に置く  T3a
  words.rs          2 値のワード列プリミティブ (add/sub/neg/shift/mul_small_add/divrem_small/cmp/is_zero)  T3b
  abi.rs            svLogicVecVal / svBitVecVal との変換               T3b
  ops/mod.rs        (宣言のみ)                                        T3a
  ops/bitwise.rs    ビット演算                                        T4
  ops/reduce.rs     リダクション、$countbits 系                       T4
  ops/logical.rs    truth, 論理演算                                   T4
  ops/compare.rs    等価、比較、casez/casex                           T4
  ops/cond.rs       cond_merge, cond_merge_bits                      T4
  ops/edge.rs       エッジ検出                                        T4
  ops/arith.rs      算術、pow_bits                                    T5
  ops/shift.rs      シフト、shl_bits 等                               T6
  ops/resize.rs     拡張、切り詰め、2 値/4 値の変換                    T6
  ops/concat.rs     連結、複製                                        T6
  ops/select.rs     選択、インデックス、get_slice2 等                 T6
  ops/math.rs       $clog2                                          T6
  literal.rs        リテラル解析                                       T7
  string.rs         文字列変換、atoi 系                               T7
  real.rs           実数変換                                          T8
  fmt.rs            表示用フォーマット、itoa 系                        T8
crates/svsim-logic/tests/
  common/mod.rs     proptest の戦略、RefVec との変換 (bits 文字列経由)、不変条件の検査   T3a
  diff_core.rs, no_alloc_core.rs                                    T3b
  diff_bitwise.rs, diff_compare.rs, golden_bitwise.rs, no_alloc_bitwise.rs   T4
  diff_arith.rs, golden_arith.rs, no_alloc_arith.rs                 T5
  diff_struct.rs, golden_struct.rs, no_alloc_struct.rs              T6
  diff_literal_string.rs, golden_literal.rs                         T7
  diff_real_fmt.rs, golden_fmt.rs                                   T8
crates/svsim-logic-ref/src/          (全ファイルが T9。T1 が公開 API の最小限のスタブを置く)
  lib.rs, value.rs, bitwise.rs, compare.rs, arith.rs, structure.rs, literal.rs, string.rs, real.rs, fmt.rs, edge.rs
```
- **スタブ**: T3a は、後続タスクが触る `svsim-logic` のファイルを**すべて空のスタブとして作り**、`lib.rs` と `ops/mod.rs` のモジュール宣言を済ませる。後続タスクは自分の担当ファイルだけを編集すればよい。
  - 各スタブには `//!` の 1 行説明を付ける (R-14)。これが無いと `missing_docs` と `clippy -D warnings` の組み合わせで通らない。
- **`words.rs`**: 2 値ワード列のプリミティブ (特に `mul_small_add` と `divrem_small`) は T5 (算術)、T7 (10 進リテラル)、T8 (10 進表示) が共有するので、T3b で作る。T7 と T8 は T5 の多倍長除算に依存しない (10^19 で繰り返し割れば足りる)。
  - `words.rs` は `pub(crate)`。
- **`tests/common/mod.rs`**: T4〜T8 は**読むだけ**にする。追加の戦略は各テストファイルの中に書く。
- **参照モデルとの受け渡し**: 値は bits 文字列 (`LogicVec::to_bits_string` ⇔ `RefVec::from_bits_str`) で受け渡す。こうすれば、参照モデルの crate は本体の型に依存しない。

### 3.8 DPI 形式との変換 (タスク M0-T3b、`abi.rs`)
```rust
/// LogicRef → svLogicVecVal 配列 (u32 の aval/bval が交互に並ぶ。長さは 2*ceil(w/32))。
pub fn write_sv_logic_vec_val(src: LogicRef<'_>, dst: &mut [u32]);
/// svLogicVecVal 配列 → LogicMut。最上位要素の w ビット目以上は読み捨ててマスクする。
pub fn read_sv_logic_vec_val(src: &[u32], dst: LogicMut<'_>);
pub fn write_sv_bit_vec_val(src: BitRef<'_>, dst: &mut [u32]);
pub fn read_sv_bit_vec_val(src: &[u32], dst: BitMut<'_>);
```
- 要素数は `(w + 31) >> 5` (Annex I の `svdpi.h` で確認済み)。
- **ゼロコピーにできるのは、2 値の input 引数 (const ポインタ) だけ**。
  - 4 値の引数は、`write_sv_logic_vec_val` で一時バッファに詰め替える。
  - output / inout 引数は、2 値でも 4 値でも次の手順を踏む: 一時バッファに受け取る → `read_*` で最上位要素の未使用ビットをマスクする → 実引数へ代入する (R-20)。
- C 側が未使用ビットに何を書いても、読み込み時にマスクして正規化不変条件を守る。
- M3 で 2 値の `svBitVecVal` をアリーナから直接ポインタで渡す場合も、境界をはみ出さない。`u32` 単位の要素数 `ceil(w/32)` は、`u64` のワード数の 2 倍以下だからである。
- `#[cfg(not(target_endian = "little"))] compile_error!(…)` を置く (R-13)。

### 3.9 表示用フォーマット (タスク M0-T8、`fmt.rs`)
```rust
pub enum FmtRadix { Bin, Oct, Dec, Hex }
/// Auto = 既定の自動幅。Min { width: 0, .. } = %0d 等。zero_pad = %05d のような 0 埋めの指定 (R-19)。
pub enum FieldWidth { Auto, Min { width: u32, zero_pad: bool } }
pub fn format_int(out: &mut String, x: LogicRef<'_>, radix: FmtRadix, signed: bool,
                  width: FieldWidth, s: &mut Scratch);
pub fn format_int2(out: &mut String, x: BitRef<'_>, radix: FmtRadix, signed: bool,
                   width: FieldWidth, s: &mut Scratch);
pub fn itoa(x: LogicRef<'_>, signed: bool, radix: FmtRadix, s: &mut Scratch) -> Vec<u8>;   // §3.5(h)
```
規則 (§21.2.1.2 書式指定、§21.2.1.3 表示データのサイズ、§21.2.1.4 不定値・ハイインピーダンス値。21.2.1.3/4 の番号は未検証):
1. **`%b %o %h`** (`%x` は `%h` と同じ): LSB 側から 1/3/4 ビットずつを 1 桁にする。最上位の桁は端数で、存在するビットだけで判定する。各桁は次のように表示する。
   - 全ビットが X なら `x`、全ビットが Z なら `z`。
   - 一部が X なら `X`。X が無く一部が Z なら `Z`。
   - 全ビットが既知なら数字 (16 進は小文字 `a-f`)。
2. **`%d`**: 値全体で判定する。全ビットが X なら `x`、全ビットが Z なら `z`、一部が X なら `X`、X が無く一部が Z なら `Z`。全ビットが既知なら 10 進で表示する (符号付きで負なら `-` を付ける)。
3. **自動幅 (`Auto`)**: その幅の値が取りうる最大の表示に必要な文字数にする。
   - `%b %o %h` は、先頭を 0 で埋める。
   - `%d` は、先頭を空白で埋める (右詰め)。文字数は、符号なしなら `2^w - 1` の桁数、符号付きなら `-2^(w-1)` の文字数 (符号を含む)。
   - X/Z の 1 文字表示も、同じ幅で右詰めにする。
4. **`%0d` 等** (`Min { width: 0, .. }`): 先頭の 0 や空白を付けない最小の表示。`%0h` で上位の桁が X/Z の場合は、そこで止まる (X/Z の桁は 0 ではないため)。
5. **`Min { width: n, zero_pad }`** (n > 0): 埋め文字は次のとおり。値が n 文字より長くても切り詰めない。[要VCS確認 V-7]
   - `%d` は空白 (`zero_pad` なら 0)。
   - `%b %o %h` は 0。
6. 2 値の値は、X/Z が無いだけで同じ規則に従う。
7. 左詰め (`%-5d`) が規格にあるかは未確認。`%` 指定の解析と一緒に M1 で確認する。

- 規則 1〜3 と 5 (埋め文字) は、Icarus と Verilator の実装と一致することをレビューで確認済み。
- 例 (ゴールデンテストに入れる):

  | 書式 | 値 | 表示 |
  |---|---|---|
  | `%b` | `4'b10xz` | `10xz` |
  | `%h` | `8'bxxxx_01z0` | `xZ` |
  | `%h` | `8'bzzzz_xxxx` | `zx` |
  | `%h` | `8'b0x01_0000` | `X0` |
  | `%d` | `8'bx` | `  x` (3 文字) |
  | `%d` | `8'sb1111_1111` | `  -1` (4 文字) |
  | `%d` | 32 ビット符号付きの `1` | `          1` (11 文字) |
  | `%0d` | `-5` | `-5` |

### 3.10 JIT・アリーナ・並列化・チェックポイントとの関係の確認
| 後続 | 本設計で守っていること |
|---|---|
| アリーナ (M1〜) | packed 値は §3.2 の配置で `WordArena` (§4.4) の連続したワードに置く。`LogicRef::from_planar` / `LogicMut::from_planar` でビューを作り、層 2 のカーネルを in-place で呼ぶので、値型の一時オブジェクトは要らない。unpacked 配列の要素の詰め方は M1 で別に決める (§3.2) |
| JIT (M5) | 幅 ≤ 64 は、§3.4 の式を Cranelift IR で直接生成する (ヘルパ呼び出しは無い)。幅 > 64 は、層 2 のカーネルを `extern "C" fn(dst: *mut u64, x: *const u64, y: *const u64, width: u32)` で包んで呼ぶ。この包みは M5 で svsim-jit 側に作り、unsafe はそこに閉じ込める。カーネルはヒープ確保をせず、panic するのは幅の assert だけ。層 1 は記憶域の配置から独立している (§3.4) |
| 2 値推論 (M5) | 2 値の値は aval プレーンだけで持ち、その配置は 4 値の aval プレーンと同じ。2 値と証明できた信号は、bval を持たない配置に切り替えられる。混在の規則 (§3.5.1) によって、2 値推論の可否は「自己決定のオペランドを含めて、式の木に 4 値のオペランドが無いこと」になる |
| チェックポイント (M4) | 状態はワード列だけなので、memcpy でスナップショットを取れる。値の内部にポインタを持たない (`LogicVec` のヒープは一時値だけに使い、アリーナには入れない) |
| 並列 (M6) | カーネルはグローバルな状態を持たない (`Scratch` は呼び出し側が持つ) ので、どのスレッドで実行しても同じ結果になる |
| DPI (M3) | §3.8 の変換を使う。スカラの `svLogic` は `Logic as u8` のまま渡せる。4 値の open array の扱いは未決 R-21 (§10.1) |
| デバッグ (M4) | `fmt.rs` を、波形や問い合わせ API の値の表示にも使う |

### 3.11 強度 (strength) との関係
強度付き値 (PLAN §4、M2。IEEE 1800 §28.11 以降) は、値 (0/1/X/Z) と、0 側・1 側の強度を持つ別の表現にする。`svsim-logic` の 4 値表現は、強度を解決した後の値として使う。M0 では強度を扱わない。M2 で別モジュールとして追加する想定で、現在の型を変更する必要は無い。

---

## 4. `svsim-common` 設計

### 4.1 ソースマップ (`source.rs`、タスク M0-T10)
```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct FileId(u32);
/// バイトオフセットの半開区間 [lo, hi)。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Span { pub file: FileId, pub lo: u32, pub hi: u32 }

pub enum FileOrigin {
    Disk(std::path::PathBuf),
    Virtual { name: String },                          // コマンドラインの define 等
    Include { from: Span },                            // M1: `include された位置
    MacroExpansion { call_site: Span, def_site: Span } // M1: マクロ展開を仮想ファイルとして持つ
}
pub struct SourceFile { origin: FileOrigin, text: Box<[u8]>, line_starts: Vec<u32> }
pub struct SourceMap { files: Vec<SourceFile> }
impl SourceMap {
    pub fn add_file(&mut self, origin: FileOrigin, text: Vec<u8>) -> FileId;
    pub fn file(&self, id: FileId) -> &SourceFile;
    pub fn line_col(&self, file: FileId, offset: u32) -> LineCol;  // 1 始まり。列は UTF-8 の文字数 (不正な UTF-8 はバイト数)
    pub fn line_text(&self, file: FileId, line: u32) -> &[u8];
    pub fn snippet(&self, span: Span) -> &[u8];
}
```
- テキストは**バイト列**で持つ。SV のソースが UTF-8 とは限らず、字句解析もバイト単位で行うため。表示するときだけ、損失のある UTF-8 変換をする。
- 1 ファイルの上限は 4 GiB (`u32` のオフセット)。`Span` は 12 バイト。
- マクロ展開の追跡 (PLAN §2) は、`FileOrigin::MacroExpansion` の仮想ファイルで表す想定。具体化は M1 で行い、M0 では列挙子を用意するだけにする。

### 4.2 診断 (`diag.rs`, `diag/render_human.rs`, `diag/render_json.rs`、タスク M0-T10)
```rust
pub enum Severity { Error, Warning, Note, Help }
pub struct DiagCode(pub &'static str);           // 例: "E0001"。番号体系の割り当ては M1 で決める
pub struct Label { pub span: Span, pub message: String, pub primary: bool }
pub struct FixIt { pub span: Span, pub replacement: String, pub message: String }
pub struct Diagnostic {
    pub severity: Severity, pub code: Option<DiagCode>, pub message: String,
    pub labels: Vec<Label>, pub notes: Vec<String>, pub fixits: Vec<FixIt>,
}
pub struct DiagSink { diags: Vec<Diagnostic>, errors: usize, warnings: usize }
impl DiagSink {
    pub fn emit(&mut self, d: Diagnostic);
    pub fn has_errors(&self) -> bool;
    /// 決定的な順序 (ファイル ID、開始位置、発行順) に並べて返す。並列パースでも出力順を一定にするため。
    pub fn into_sorted(self) -> Vec<Diagnostic>;
}
pub fn render_human(sm: &SourceMap, d: &Diagnostic) -> String;
pub fn render_json(sm: &SourceMap, d: &Diagnostic) -> serde_json::Value;
```
- **JSON** は AI 向けの構造化診断 (PLAN §8.4) で、M0 でスキーマまで作る。`version` フィールドを付けて版を管理する。
  ```
  {"version":1, "severity", "code", "message",
   "labels":[{"file","line","col","end_line","end_col","message","primary"}],
   "notes", "fixits":[{"file","line","col","end_line","end_col","replacement","message"}]}
  ```
- **人間向けの表示**は、M0 では単一行の span だけに対応する。表示する内容は、位置 (`file:line:col`)、該当行、`^^^` の下線、ラベルのメッセージ、注記。
  - 複数行にまたがる span は、開始位置と開始行だけを表示する。
  - 色付け、複数行の描画、fix-it の表示は M1 に回す (過剰設計の指摘への対応)。
- 表示部は自前で実装する (外部の診断表示 crate は使わない)。

### 4.3 文字列インターン (`intern.rs`、タスク M0-T11)
```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Symbol(u32);
pub struct Interner { map: rustc_hash::FxHashMap<Box<str>, Symbol>, strings: Vec<Box<str>> }  // 実装は自由 (重複して保持しない工夫も可)
impl Interner {
    pub fn intern(&mut self, s: &str) -> Symbol;
    pub fn get(&self, s: &str) -> Option<Symbol>;
    pub fn resolve(&self, sym: Symbol) -> &str;
}
```
- **規則**: `Symbol` の番号順を出力の順序に使わない。並列化したときに、番号が実行ごとに変わってもよいようにするため。この規則は crate ドキュメントに書く。
- 並列パース用のスレッド安全版は、M1 以降で必要になった時点で決める (候補: スレッドごとのローカル表を後で統合する、または `lasso` 等の crate)。M0 の API は `&mut self` を取る単一スレッド版。

### 4.4 アリーナ (`index.rs`, `arena.rs`、タスク M0-T11)
アリーナは 2 種類ある。

**(1) 型付きインデックス配列** (コンパイラのデータ構造用: AST、型表、階層など)
```rust
pub trait Idx: Copy + Eq + Ord + std::hash::Hash { fn new(i: usize) -> Self; fn index(self) -> usize; }
#[macro_export] macro_rules! define_index { ($vis:vis struct $name:ident;) => { /* u32 の newtype + Idx 実装 */ } }
pub struct IndexVec<I: Idx, T> { raw: Vec<T>, _m: PhantomData<fn(&I)> }
impl<I: Idx, T> IndexVec<I, T> {
    pub fn push(&mut self, v: T) -> I;  pub fn len(&self) -> usize;
    pub fn iter_enumerated(&self) -> impl Iterator<Item = (I, &T)>;
}
impl<I: Idx, T> Index<I> for IndexVec<I, T> { … }   // IndexMut も
```

**(2) 状態アリーナ `WordArena`** (シミュレーション状態用。PLAN §1 原則 1 の基礎)
```rust
/// ワード (u64) 単位のオフセット。表せるのは 2^32 ワード = 32 GiB まで (R-23)。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct WordOffset(pub u32);
pub struct ArenaLayoutBuilder { next: u32 }
impl ArenaLayoutBuilder {
    /// align_words は 2 のべき (1, 2, 8 …)。u32 を溢れたら Err (R-23)。
    pub fn alloc(&mut self, words: u32, align_words: u32) -> Result<WordOffset, ArenaError>;
    pub fn finish(self) -> ArenaLayout;
}
pub struct ArenaLayout { len_words: u32 }
pub struct WordArena { words: Vec<u64> }          // 0 で初期化
impl WordArena {
    pub fn new(layout: &ArenaLayout) -> Self;
    pub fn words(&self) -> &[u64];
    pub fn words_mut(&mut self) -> &mut [u64];
    pub fn slice(&self, off: WordOffset, len: u32) -> &[u64];
    pub fn slice_mut(&mut self, off: WordOffset, len: u32) -> &mut [u64];
    /// 互いに重ならない複数の可変スライス (dst と src を同時に取るため)。重なりや範囲外は Err。
    pub fn disjoint_mut<const N: usize>(&mut self, ranges: [(WordOffset, u32); N]) -> Result<[&mut [u64]; N], ArenaError>;
    pub fn snapshot(&self) -> ArenaSnapshot;          // 全体のコピー (memcpy)
    pub fn restore(&mut self, s: &ArenaSnapshot);     // 同じ layout から作ったものに限る (長さを検査する)
    /// 決定性テスト用の内容ハッシュ。各ワードをリトルエンディアンの 8 バイトとして並べた列に FNV-1a 64 をかける (R-23)。
    pub fn content_hash(&self) -> u64;
}
```
- アリーナは「何がどこにあるか」(信号名 → オフセット、幅、2 値/4 値の別) を知らない。その対応表は M1 のエラボ/MIR が持つ。
- M0 では、`svsim-common` の結合テスト (`tests/arena_logic.rs`。`svsim-logic` を dev-dependency にする) で次を示す。
  1. アリーナ上に 4 値と 2 値の信号を置く。
  2. `LogicRef::from_planar` / `LogicMut::from_planar` でビューを作り、in-place で演算する。
  3. スナップショット → 変更 → 復元で、元の状態に戻る。
- 0 で初期化した値は、4 値でも「0」を意味する。`logic` の初期値 X (§6.8: 4 値型の既定の初期値は X) は、M1 のエラボ側が明示的に `fill(X)` する (アリーナは型を知らないため)。
- ベースアドレスのキャッシュライン境界 (64 バイト) への位置合わせと、ページ単位の差分スナップショットは M4/M6 で行う。M0 の `align_words` は、ベースからの相対的な位置合わせだけ。

---

## 5. 差分テスト戦略 (`svsim-logic`)

### 5.1 参照モデル `svsim-logic-ref` (タスク M0-T9、R-4)
- **独立性**:
  - 本体 (`svsim-logic`) を書かない別の実装エージェントが、この計画と規格の知識**だけ**を見て書く。本体のコードは見ない。
  - `svsim-logic` には依存しない。
  - T4〜T8 で参照モデルの誤りらしきものに気付いても、T4〜T8 の担当は参照モデルを**直さない**。マネージャに報告し、マネージャが規格に照らして判断してから、T9 の担当に戻して直させる。
- **値**: `RefVec(Vec<RefLogic>)`。LSB が先頭で、1 要素が 1 ビット。`enum RefLogic { Zero, One, X, Z }`。
- **公開 API** (固定。T1 が `unimplemented!()` の本体でスタブを置き、T9 が実装する):
  ```rust
  impl RefVec {
      pub fn from_bits_str(s: &str) -> Result<RefVec, String>;   // MSB が先頭、'_' は無視 (LogicVec と同じ形式)
      pub fn to_bits_string(&self) -> String;
      pub fn width(&self) -> usize;
  }
  ```
  T1 がスタブを置くのはこの 3 つだけ。T3a の `tests/common/mod.rs` はこの 3 つだけを使う。
  - 演算の関数は T9 が次の名前で作る (自由関数、引数は `&RefVec`、2 値版は無い。2 値の挙動は「X/Z を含まない入力 + 結果に `to_two_state`」で確かめる):
  `not and or xor xnor reduce_{and,nand,or,nor,xor,xnor} truth logical_{not,and,or,implies,equiv} count_bits onehot onehot0 is_unknown eq ne case_eq wildcard_eq casez_match casex_match lt le gt ge cond_merge add sub neg mul div rem pow shl shr ashr resize to_two_state concat replicate get_slice set_slice clog2 edge parse_based parse_unsized_decimal from_string_bytes to_string_bytes atoi to_f64 to_f32 from_f64_round from_f64_trunc format_int`。
  `signed` 引数やオフセットなどは §3.5 の本体の関数と同じ順に取る。
- **実装方針**:
  - ビット演算、リダクション、等価、ワイルドカード、casez/casex、`?:` のマージ、エッジ: **規格の真理値表を 4×4 の表としてそのまま書き**、ビットごとに適用する。§3.4 の式は使わない。
  - シフト、連結、選択、拡張: `Vec` の要素の並べ替えとして素朴に書く。
  - 算術、比較、べき乗、`$clog2`:
    - X/Z があれば全 X (`$clog2` は `None`)。
    - X/Z が無ければ、`num-bigint` の `BigInt` に変換して計算し、`2^w` の剰余を取って戻す。
    - 除算と剰余は `BigInt` の `/` と `%` (0 方向への切り捨て、剰余は被除数の符号) を使う。
    - べき乗は、指数が負の場合を §3.5(c) の規則で場合分けし、非負なら `modpow(exp, 2^w)` で計算する。
  - 10 進表示は `BigInt::to_str_radix`。2/8/16 進表示は、ビットを桁ごとに集めて §3.9 の規則を素朴に適用する。
  - 実数:
    - 整数 → 実数は、`BigInt` から正しく 1 回だけ丸める自前の実装。f64 用と f32 用を別々に作る。
    - 実数 → 整数は、`f64` を符号・指数・仮数に分解し、`BigInt` へ正確に変換してから丸める。
  - リテラル: 1 文字ずつ桁を `RefVec` に積む素朴な実装。
- 参照モデル自身の単体テストとして、規格の表と例のゴールデンテストを T9 が書く。§5.4 のゴールデンテストと同じ値でよい。

### 5.2 比較の方法
- テストごとに、`LogicVec` (層 3)、スライスカーネル (層 2)、スカラ (層 1。幅 ≤ 64 のとき) の 3 つを、それぞれ参照モデルと比べる。比較は「全ビットが同一 (aval・bval とも)」で行う。
- 毎回、出力の**正規化不変条件** (§3.2) も検査する。
- `_assign` 版も、同じ入力で比べる。
- 混在カーネル (§3.5.1) は、「`from_two_state` してから 4 値カーネル」の結果、および参照モデルと比べる。
- 2 値版は、X/Z を含まない入力で、「参照モデルの結果 → `to_two_state`」と比べる。

### 5.3 入力の生成 (proptest)
- **幅**:
  - 40%: 境界値 `{1, 2, 3, 7, 8, 31, 32, 33, 63, 64, 65, 127, 128, 129, 191, 192, 193, 255, 256, 257}` から選ぶ。
  - 40%: `1..=200` から選ぶ。
  - 20%: `201..=1024` から選ぶ。
- **値**:
  - ワードごとに、全 0 / 全 1 / 乱数 / 1 ビットだけ 1 / 符号ビットだけ 1 / 符号付きの最大・最小を混ぜる。
  - X/Z の密度は `{0, 1 ビットだけ, 1%, 50%, 全部}` から選び、X と Z の比も変える。
  - 算術では X/Z を含まない入力の比率を高くする (意味のある計算を多く試すため)。
- **シフト量**: `0..=w+2` の小さい値、64 の倍数の前後、64 ビットを超える巨大な値、X/Z を含む値。
- **選択のオフセット**: 負、0、範囲の境界、完全に範囲外、`i64` の端、`Index::Unknown`。
- **除数**: 0、1、-1、2 のべき、乱数。
- **べき乗の指数**: 負、0、1、大きい値。
- **件数**: `PROPTEST_CASES` は既定で 256 (CI の通常ジョブ)。定期ジョブ (§6) では 100,000。失敗の再現ファイル (`proptest-regressions/`) はコミットする。

### 5.4 網羅テスト・ゴールデンテスト
- **網羅テスト**:
  - 幅 1〜3 の 4 値の全値 (4^3 = 64 値) の全組み合わせで、ビット演算、比較、等価、`?:` のマージ、ワイルドカード、casez/casex を試す。
  - エッジ検出は 4×4 の全遷移を試す。
  - 幅 1〜4 の全値 × シフト量 `0..=w+1` と X で、シフトを試す。
  - 幅 1〜6 の 2 値の全組み合わせで、算術 (符号付き/符号なし) を試す。
- **ゴールデンテスト** (規格の表と例を固定値で書く。テスト名やコメントに条番号を書き、表番号は書かない):
  - §11.4.8 の各表、§11.4.9 のリダクションの例
  - §11.4.3 のべき乗の規則と剰余の例
  - §11.4.10 のシフトの例、§11.4.11 の表と `1'bx ? 1'bz : 1'bz` ([要VCS確認 V-8])
  - §11.4.12.1 の複製の例、§11.5.1 の範囲外の選択
  - §5.7.1 のリテラルの例 (R-11 の各ケース)、§6.12.1 の丸めの例
  - §9.4.2 のエッジ表、§20.8.1 の `$clog2(0)`・`$clog2(1)`・2 のべきの前後
  - §3.9 の表示の例、§3.5.1 の混在の例
- **上限近くの幅のスモークテスト** (R-27): 幅 2^20 で、加算、シフト、連結、`%h` 表示を 1 回ずつ行う。`#[ignore]` を付け、定期ジョブでだけ実行する。

### 5.5 ヒープ確保の検査
- `tests/no_alloc_<area>.rs` で、確保の回数を数える `GlobalAlloc` を設定する。このファイルだけ、先頭で `#![allow(unsafe_code)]` とし、`// SAFETY:` を付ける。
- 次の 2 つで確保が 0 回であることを検査する。
  1. 幅 ≤ 64 の `LogicVec` の全演算。
  2. 事前確保した `Scratch` を渡した、任意幅のスライスカーネル。
- **スレッドへの注意** (R-24): テストハーネスは `#[test]` を並列のスレッドで走らせる。そのため、カウンタはスレッドローカルにする (`const` 初期化の `thread_local!` は確保しない)。
- 枠と T3a/T3b の範囲の検査 (`no_alloc_core.rs`) は T3b が作る。各タスクは自分の範囲の別ファイルを作る。

---

## 6. CI (GitHub Actions、タスク M0-T2。`corpus-fetch` ジョブは M0-T13 で追加)
`.github/workflows/ci.yml`:
| ジョブ | トリガ | 内容 |
|---|---|---|
| `fmt` | push / PR | `cargo fmt --all -- --check` |
| `clippy` | push / PR | `cargo clippy --workspace --all-targets -- -D warnings` |
| `test` | push / PR | `cargo build --workspace --locked`、`cargo test --workspace --locked` (debug)。proptest は既定の件数 |
| `doc` | push / PR | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` |
| `deny` | push / PR | `cargo deny check licenses bans sources`。許可するライセンスは、T1 で `Cargo.lock` を確定した時点の `cargo deny list` の結果から、必要なものだけにする。想定は MIT, Apache-2.0, Unicode-3.0 程度で、Unlicense は単独では許可しない (R-25) |
| `corpus-fetch` | push / PR (T13 で追加) | `cargo corpus fetch sv-tests` と `cargo corpus discover sv-tests`。`SVSIM_CORPUS_CACHE` を `~/.cache/svsim/corpus` に置き、`actions/cache` でキャッシュする (キーはマニフェストのハッシュ) |
| `proptest-long` | 毎日 (schedule) + 手動 | `PROPTEST_CASES=100000 cargo test -p svsim-logic --release`、および `--ignored` の上限近くの幅のスモークテスト |
- ツールチェーンは `rust-toolchain.toml` に従う (`rustup show` で導入する)。ビルドキャッシュには `Swatinem/rust-cache` を使う。
- **サードパーティの action はコミット SHA で固定し** (決定 U-5)、コメントにバージョンのタグを書く。例: `uses: actions/checkout@<40桁SHA> # v4.x.y`。SHA は T2 の実装時に各 action のリリースから取る。
- M0 は Linux (ubuntu-latest) だけで回す。macOS と Windows は DPI の時期 (M3) に検討する。

---

## 7. コーパスマネージャ雛形 (`svsim-corpus`、タスク M0-T12, M0-T13)

### 7.1 マニフェスト (`corpus/manifest.toml`)
```toml
version = 1

[[entry]]
name = "sv-tests"
description = "CHIPS Alliance sv-tests: IEEE 1800 の章別テスト"
license = "ISC"
tags = ["standard"]
[entry.source]
git = "https://github.com/chipsalliance/sv-tests"
rev = "7274242bcf99a2f6f8282493d0200c92eb87d347"   # 2026-11-27 時点の HEAD。必ず 40 桁のコミット ID
submodules = []                                   # M0 では取得しない (third_party のコアや UVM は後で)
sparse = []                                       # 空なら全体を取得する。必要なら ["tests/", "LICENSE"]
[entry.discover]
kind = "sv-tests"                                 # アダプタ: sv-tests のメタデータを解釈する
root = "tests"
```
- M0 のアダプタは `sv-tests` だけ。`kind` で分ける形なので、`glob` アダプタ (一般のリポジトリ用) を後から足せる (M3 で CPU コアなどを入れるとき)。
- M0 のソースは `git` だけ。ローカルの非公開コード用の `path = …` と、`corpus/private/` の自動読み込みは後回しにする。
- マニフェストは、既定の `corpus/manifest.toml` に加えて、`--manifest <path>` で読み込める (複数指定可)。エントリ名が重複したらエラーにする。
- `rev` にはコミット ID だけを許す。ブランチ名やタグは不可 (再現性のため)。
- 未知のキーはエラーにする (`serde(deny_unknown_fields)`)。

### 7.2 取得とキャッシュ
- **キャッシュの場所**: `$SVSIM_CORPUS_CACHE`。未設定なら `$HOME/.cache/svsim/corpus/` (マネージャ決定)。配置は `<cache>/<name>/<rev>/`。
  - リポジトリの外に置くので、worktree が複数あってもキャッシュを共有できる。
- **取得の手順**: 外部コマンドの `git` を使い、libgit2 等の依存は入れない。
  1. `git init` → `git fetch --depth 1 <url> <rev>` → `git checkout --detach FETCH_HEAD`
     (先端でないコミットを浅く取得できることは、レビューで file:// で確認済み)
  2. `git rev-parse HEAD` が `rev` と一致することを検証する。
  3. sparse / submodules の指定があれば適用する。
  4. 一時ディレクトリ (`<cache>/<name>/.tmp-<rev>-<pid>`) から最終位置へ `rename` して、原子的に完了させる。
  5. 完了印の `.svsim-complete` を置く。
- **git の環境の固定** (R-18): 次の設定で実行する。
  - `GIT_TERMINAL_PROMPT=0`
  - `GIT_CONFIG_NOSYSTEM=1`
  - `-c core.hooksPath=/dev/null`
  - `-c advice.detachedHead=false`
  - `-c protocol.file.allow=always` は、テスト (file://) のときだけ付ける。
- **冪等性**:
  - 完了印があれば何もしない。
  - `--offline` ではネットワークに出ず、キャッシュに無ければエラーにする。
  - `rename` が失敗したとき (別のプロセスが先に完了した場合) は、最終位置の完了印を確かめて成功扱いにする (R-18)。
- **排他** (R-18): `<cache>/<name>/.lock` を `create_new` で作って排他する。中身には PID と作成時刻を書く。
  - ロックが既にあり、そこに書かれた PID のプロセスが存在しなければ (Linux では `/proc/<pid>` で確かめる)、古いロックとして取り除いて取り直す。
  - それ以外の場合は、待たずにエラーにする。メッセージで `--force-unlock` を案内する。
- **安全**:
  - 取得したファイルは信頼しない入力として扱う。コーパスマネージャはファイルを読むだけで、取得したリポジトリ内のスクリプト (sv-tests の Python の生成器など) は実行しない。
  - git のフックは、新規に init したリポジトリなので存在せず、上の `core.hooksPath` でも無効にしてある。
- sv-tests の生成テスト (`conf/generators`。サブモジュールのコアや UVM を使うもの) は M0 の対象外。

### 7.3 テストケースの列挙
```rust
pub struct TestCase {
    pub id: String,                 // "<entry>/<root からの相対パス>"
    pub files: Vec<PathBuf>, pub incdirs: Vec<PathBuf>, pub defines: Vec<String>,
    pub top: Option<String>,
    pub expect: Expectation,
    pub meta: BTreeMap<String, String>,   // アダプタ固有の情報 (sv-tests の :tags: 等をそのまま入れる)
}
pub struct Expectation { pub should_fail: bool, pub reason: Option<String>, pub target: Stage }
```
**sv-tests アダプタ**: sv-tests 本体の `tools/runner` の解釈に合わせる。固定コミットの実ファイルで確認済み (R-16)。
- **対象**: `tests/` 以下の `*.sv` (大文字小文字を区別しない。sv-tests の Makefile と同じ `find -iname '*.sv'`)。固定コミットでは **1028 件**。
  - この中には、ライブラリ用のファイル `tests/testbenches/*.sv` (11 件) と `tests/uvm/uvm_files.sv` も含まれる。sv-tests の Makefile と同じ数え方なので、これらもテストケースとして数える。
- **メタデータの解析**:
  - ファイルの**全行**を `^:([a-zA-Z_-]+):\s*(.+)` で照合する。
  - キー名は**小文字に変換**してから照合する。
  - 値は、前後の空白 (`\r` を含む) を除いてから使う。固定コミットには CRLF のファイルが 8 件あり、値の末尾に `\r` が付くため。
  - 同じキーが複数あれば、後のものが勝つ。
  - 対応するキー: `name tags description files incdirs top_module timeout type should_fail should_fail_because defines compatible-runners unsynthesizable results_group simvariants`、および `runner_<tool>_flags`。それ以外のキーは警告して無視する。
- **既定値**:
  - `files` = そのファイル。`incdirs` = そのファイルのディレクトリ。
  - `type` = `"parsing elaboration"`。
  - `should_fail` = `should_fail_because` があれば 1。
  - `files`、`incdirs`、`defines` は空白区切り。明示された `:files:` と `:incdirs:` は **`tests/` からの相対パス**として解釈する。
- **期待する段階**: `:type:` の語 (`preprocessing parsing elaboration simulation`) のうち、最も後の段階を `Expectation::target` にする。
- **ライブラリの追加ファイル**: `tags` に UVM 等のライブラリ名がある場合の追加ファイル (sv-tests の `libs` 設定) は M0 では扱わず、`meta` に残すだけにする。
- **固定コミットでの件数**:
  - 実際に使われているキー: `name tags description type unsynthesizable timeout should_fail_because top_module defines runner_verilator_flags`。
  - `should_fail_because` を持つファイル: 79 件。
  - `:type:` の内訳: `preprocessing` 90、`parsing` 5、`elaboration` 5、`preprocessing parsing` 9、`simulation` 4、`simulation elaboration` 53、`simulation elaboration parsing` 289、`simulation elaboration preprocessing` 1、指定なし 572 (レビューで集計)。
- 列挙の結果は `id` のバイト順に並べる (決定性のため)。

### 7.4 段階評価の枠組み
```rust
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize)]
pub enum Stage { Preprocess, Parse, Elaborate, Simulate, Compare }
pub enum StageOutcome { Pass, Fail { category: String, message: String }, NotImplemented, Skipped { reason: String }, Timeout }
pub trait StageRunner: Send + Sync {
    fn stage(&self) -> Stage;
    fn run(&self, case: &TestCase, env: &RunEnv) -> StageOutcome;
}
pub struct Pipeline { runners: Vec<Box<dyn StageRunner>> }   // M1 で Preprocess と Parse の runner を登録する
```
- ケースごとに、`Preprocess` から `target` まで順に実行し、`Pass` 以外が出たらそこで止める。
- 期待失敗 (`should_fail`) のケースでは、生の結果と「期待どおりか」の判定を両方記録する。判定規則は「`target` までのどこかで `Fail` なら期待どおり」とし、M1 で runner が入ったときに見直す。
- M0 には runner が 1 つも無いので、全ケースが `Preprocess: NotImplemented` になる。
- **出力** (既定の `<out>` は `target/corpus-results/<entry>/`):
  - `<out>/results.jsonl`: 1 行に 1 ケース。id、段階ごとの結果、所要時間。
  - `<out>/summary.json`: 段階 × 結果の件数。
  - 所要時間以外は決定的にする (比較や差分表示に使えるように)。
  - 人間向けの `summary.md` は後回し (M1)。件数は標準出力にも表示する。
- ケースは逐次に実行する。`--jobs N` の並列実行は、パースが入る M1 で追加する。

### 7.5 CLI
```
svsim-corpus list                          マニフェストのエントリの一覧
svsim-corpus fetch [NAME...] [--offline] [--force-unlock]   取得 (名前を省略すると全部)
svsim-corpus discover NAME [--json]        ケースを列挙し、件数とメタデータの統計を表示する
svsim-corpus run NAME [--out DIR]
```
- `cargo corpus <subcmd>` で動かす (`.cargo/config.toml` のエイリアス)。
- 終了コード: 0 = 成功、1 = 実行時のエラー (取得の失敗等)、2 = 使い方の誤り。`run` は、ケースが失敗しても 0 以外にしない (計測用のため)。

---

## 8. タスク分解

### 8.1 一覧
| ID | 内容 | 変更範囲 | 依存 | 規模 |
|---|---|---|---|---|
| **M0-T1** | workspace の骨組み。(1) ルートの `Cargo.toml` (workspace の設定、lint、`[workspace.dependencies]` に M0 で使う全依存)、`rust-toolchain.toml`、`rustfmt.toml`、`.gitignore`、`.cargo/config.toml`。(2) 4 crate の `Cargo.toml` (`[lints] workspace = true`) と、`//!` 付きの空の `lib.rs`/`main.rs` (`#![forbid(unsafe_code)]`)。(3) `svsim-common` の全モジュールの `//!` 付きスタブと、`lib.rs` の宣言。(4) `svsim-logic-ref` の公開 API の最小スタブ (§5.1 の `RefLogic`、`RefVec`、`from_bits_str`、`to_bits_string`、`width`)。(5) `Cargo.lock` の確定 | ルート直下の設定ファイル、`crates/*/Cargo.toml`、`crates/svsim-common/src/**` (スタブのみ)、`crates/svsim-logic/src/lib.rs`、`crates/svsim-logic-ref/src/{lib,value}.rs` (スタブのみ)、`crates/svsim-corpus/src/{lib,main}.rs` | なし | S |
| **M0-T2** | CI。`.github/workflows/ci.yml` (§6 の `corpus-fetch` 以外。action は SHA で固定) と `deny.toml` (`cargo deny list` に基づく許可リスト) | `.github/`、`deny.toml` | T1 | S |
| **M0-T3a** | logic の中核の型と API の形。`Logic`、layout、ビュー、`LogicVec`/`BitVec` (構築、変換、Debug/Display、bits 文字列)、`Scratch`、`scalar.rs` の `L64` と共通補助。§3.7 の**全ファイルの `//!` 付きスタブとモジュール宣言**。`tests/common/mod.rs` (戦略、RefVec との変換、不変条件の検査)。型の単体テスト | `crates/svsim-logic/src/{lib,logic,layout,view,vec,scratch,scalar}.rs`、`src/ops/mod.rs`、§3.7 の全スタブ (空の中身のみ)、`tests/common/mod.rs` | T1 | M |
| **M0-T3b** | `words.rs` (2 値のワード列プリミティブ)、`abi.rs` (§3.8)、`diff_core.rs` (構築・変換・bit の get/set を参照モデルと比べる)、`no_alloc_core.rs` (枠と T3a/T3b の範囲の検査) | `crates/svsim-logic/src/{words,abi}.rs`、`tests/{diff_core,no_alloc_core}.rs` | T3a, T9 | M |
| **M0-T4** | ビット演算、リダクション、`$countbits` 系、論理演算、等価/比較/ワイルドカード/casez/casex、`cond_merge`/`cond_merge_bits`、エッジ検出 (§3.5 (a)(b)(f)(k))。それぞれスカラ版、スライス版、`_assign` 版、`LogicVec` のメソッド、2 値版。差分テスト、網羅テスト、ゴールデンテスト | `svsim-logic/src/ops/{bitwise,reduce,logical,compare,cond,edge}.rs`、`svsim-logic/tests/{diff_bitwise,diff_compare,golden_bitwise,no_alloc_bitwise}.rs` | T3a, T9 | M |
| **M0-T5** | 算術 (§3.5 (c)): add/sub/neg/inc/dec/mul/div/rem/pow、`pow_bits`、符号付き/符号なし、Knuth D。テスト | `svsim-logic/src/ops/arith.rs`、`svsim-logic/tests/{diff_arith,golden_arith,no_alloc_arith}.rs` | T3b, T9 | L |
| **M0-T6** | シフト (`*_bits` を含む)、拡張/切り詰め、2 値/4 値の変換、連結、複製、選択 (`get_slice2` 等)、インデックス、`$clog2` (§3.5 (d)(e)、§3.5.1) | `svsim-logic/src/ops/{shift,resize,concat,select,math}.rs`、`svsim-logic/tests/{diff_struct,golden_struct,no_alloc_struct}.rs` | T3a, T9 | M |
| **M0-T7** | リテラル解析、文字列変換、atoi 系 (§3.5 (g)(h)。itoa 系は T8) | `svsim-logic/src/{literal,string}.rs`、`svsim-logic/tests/{diff_literal_string,golden_literal}.rs` | T3b, T9 | M |
| **M0-T8** | 実数変換 (§3.5 (i))、表示用フォーマット (§3.9)、itoa/hextoa/octtoa/bintoa (`fmt.rs` に置く) | `svsim-logic/src/{real,fmt}.rs`、`svsim-logic/tests/{diff_real_fmt,golden_fmt}.rs` | T3b, T9 | L |
| **M0-T9** | **参照モデル `svsim-logic-ref` の全体** (§5.1)。本体を書かない別の実装エージェントが、計画と規格の知識だけで書く。参照モデル自身のゴールデンテストも書く | `crates/svsim-logic-ref/**` (T1 のスタブを置き換える) | T1 | M |
| **M0-T10** | ソースマップ、診断、JSON 表示、単一行の人間向け表示 (§4.1, §4.2)。表示のゴールデンテスト | `svsim-common/src/{source,diag}.rs`、`svsim-common/src/diag/**`、`svsim-common/tests/diag_*.rs` | T1 | M |
| **M0-T11** | インターン、`IndexVec`/`define_index!`、`WordArena` (§4.3, §4.4)。アリーナとロジック値ビューの結合テスト | `svsim-common/src/{intern,index,arena}.rs`、`svsim-common/tests/{arena,arena_logic,intern}.rs` | T1, T3a, T4 (結合テストで演算を使う) | M |
| **M0-T12** | コーパス: マニフェストの解析、取得・キャッシュ・ロック・検証、CLI の `list`/`fetch`、`corpus/manifest.toml` (sv-tests)。単体テスト (一時ディレクトリに作った bare リポジトリを `file://` で取得する) | `crates/svsim-corpus/src/{lib,main,manifest,fetch,cache}.rs`、`crates/svsim-corpus/tests/**`、`corpus/manifest.toml` | T1 | M |
| **M0-T13** | コーパス: 列挙 (sv-tests アダプタ)、段階評価の枠組み、結果の出力 (JSONL と summary.json)、CLI の `discover`/`run`。CI に `corpus-fetch` ジョブを追加する | `crates/svsim-corpus/src/{discover,stage,report}.rs`、`main.rs` (サブコマンドの追加)、`crates/svsim-corpus/tests/**` (新しいファイル)、`.github/workflows/ci.yml` (ジョブの追加のみ) | T12, T2 | S〜M |

### 8.2 実行の順序 (波) と worktree の運用 (R-5, R-6)
```
波 1: T1
波 2: T3a  T9  T10  T12          (4 本)
波 3: T3b  T4  T6  T2            (4 本。T3a, T9 のコミット後)
波 4: T5  T7  T8  T13            (4 本。T3b のコミット後。T13 は T12, T2 のコミット後)
波 5: T11                        (T4 のコミット後)
```
- クリティカルパスは T1 → T3a → T3b → T5/T8。T9 は波 2 で T3a と並列に走るので、律速にならない。
- **worktree の運用** (マネージャ決定):
  1. 波 n の全タスクの成果を、マネージャがレビューしてコミットする。そのあとで、波 n+1 のエージェントを起動する。worktree はコミット済みの HEAD から作られるため。
  2. 波 n+1 の各エージェントは `isolation: worktree` で起動する。
  3. マネージャは、各 worktree の差分を作業ブランチに順に取り込む。担当ファイルの範囲外の変更があれば差し戻す。全部を取り込んだら全体のビルドとテストを確認し、タスク単位でコミットする。
  4. 同時実行は**最大 4 本**。
  5. cargo の `target/` は **worktree ごとに分ける** (共有しない)。`CARGO_TARGET_DIR` は設定しない。
  6. コーパスのキャッシュは `~/.cache/svsim/corpus` (`SVSIM_CORPUS_CACHE` で変更できる) にあり、worktree 間で共有される。
- `lib.rs` と `ops/mod.rs` のモジュール宣言は T3a が済ませるので、波 3〜4 のタスクはこれらを触らない。担当ファイルが重ならないので、取り込みで衝突は起きない。
- どのタスクも依存 crate を追加しない (§2.2)。必要になったら報告する。
- **各タスクの完了条件**:
  - 共通: `cargo build --workspace`、`cargo test --workspace`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings` が通る (CLAUDE.md / implementer.md)。
  - T4〜T8: 担当する演算のすべてについて、§5 の差分テスト (3 層 × 参照モデル)、網羅テスト、ゴールデンテストがあること。
  - T9: §5.1 の全関数と、参照モデルのゴールデンテストがあること。

### 8.3 各タスクの補足
- **T1**: `svsim-logic-ref` のスタブは、§5.1 の 3 関数を `unimplemented!()` で置くだけにする。演算の関数は T9 が足す。`svsim-corpus` の dev-dependency に `tempfile` を入れる (R-17)。
- **T3a**:
  - `LogicVec` のインライン版とヒープ版の切り替えは、幅だけで決める (幅 ≤ 64 ならインライン)。同じ幅なら常に同じ表現にして、`PartialEq` と `Hash` が表現の違いに左右されないようにする。
  - `tests/common/mod.rs` は参照モデルの 3 関数だけを使う。この時点の参照モデルは T1 のスタブなので、このファイルを使うテストは T3b 以降で走る。T3a 自身のテストは参照モデルを呼ばない。
  - T3a の成果 (API の形) をマネージャが確認してからコミットする。
- **T3b**: T9 の参照モデルの `from_bits_str`/`to_bits_string` が動くことを前提にする。
- **T5**: 4 値の算術は、「未知があれば全 X」の判定を先に行い、そのあと 2 値カーネルに委ねる。`div` の符号付き処理では、最小値の絶対値が `w` ビットに収まらない点に注意する。
- **T6**: `Index::Value(offset)` の `offset + width` のオーバフローに注意する。混在カーネルは §3.5.1 のとおり。
- **T8**: 10 進表示は `words::divrem_small` で 10^19 ずつ割る。自動幅の文字数は幅だけから計算できるので、`BigInt` を使わずに計算する (本体は依存ゼロ)。
- **T9**: 本体のファイル (`crates/svsim-logic/**`) を読まない。仕様の曖昧な点は推測で決めず、マネージャに質問する。
- **T12**: 取得のテストはネットワークに出ない (`tempfile` の一時ディレクトリに `git init --bare` したリポジトリを `file://` で使う)。実際の sv-tests の取得は、CI の `corpus-fetch` ジョブと受け入れの確認で行う。
- **T13**: sv-tests アダプタの単体テストは、メタデータ付きの小さな `.sv` を一時ディレクトリに作って行う。sv-tests のファイルはリポジトリに取り込まない (CLAUDE.md §3)。CRLF のケース、キーの大文字、重複したキー、未知のキーのケースを入れる。

---

## 9. 受け入れ基準 (M0 完了の判定)
テスト計画エージェントは、ここから test-plan.md を作る。L-1 の「すべての演算」は、テスト計画で演算ごとのチェックリストに展開すること。

### 9.1 共通
- A-1 rustc 1.97.0 で、次のコマンドがすべて通る。
  - `cargo build --workspace --locked`
  - `cargo test --workspace --locked`
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`
  - `cargo deny check licenses bans sources`
- A-2 GitHub Actions の CI (§6 の push/PR ジョブ全部) が緑。action はコミット SHA で固定されている。
- A-3 unsafe の方針が守られている。
  - workspace の lint は `unsafe_code = "deny"`。
  - 全 crate のライブラリと bin のルートに `#![forbid(unsafe_code)]` がある。
  - `unsafe` があるのは `tests/no_alloc*.rs` だけで、`// SAFETY:` が付いている。

### 9.2 `svsim-logic`
- L-1 §3.5 と §3.5.1 の表のすべての演算 (4 値版、2 値版、混在カーネル) が実装されている。層 1 (幅 ≤ 64)、層 2、層 3 のそれぞれが、参照モデルとの proptest (既定で 1 性質あたり 256 件) で一致する。`_assign` 版も同様。
- L-2 `PROPTEST_CASES=100000` で `svsim-logic` のテストが通る (受け入れ時に 1 回。以後は定期ジョブで回す)。
- L-3 §5.4 の網羅テストとゴールデンテスト (条番号付き) がすべて通る。
- L-4 全演算の出力が正規化不変条件 (§3.2) を満たす (差分テストの中で検査する)。
- L-5 幅の境界 (1, 63, 64, 65, 127, 128, 129) と、幅 > 1000 を含むテストがある。幅 0 の連結と複製が動く。
- L-6 幅 ≤ 64 の `LogicVec` の演算と、事前確保した `Scratch` を渡したスライスカーネルが、ヒープ確保をしない (§5.5)。
- L-7 `abi.rs` の変換が往復で元に戻る。出力が `svLogicVecVal` の配置 (u32 の aval/bval が交互) と一致する (手で作った期待値と比べる)。未使用ビットに 1 を入れた入力が、正しくマスクされる。
- L-8 公開 API に doc コメントがある。crate ドキュメントに、§3.2 の配置の契約 (適用範囲を含む)、§3.6 の幅・符号の規則の表、§3.5.1 の混在の規則がある。
- L-9 参照モデル (T9) は本体と別の実装エージェントが書いた (STATUS の担当欄で確認する)。参照モデル自身のゴールデンテストが通る。

### 9.3 `svsim-common`
- C-1 ソースマップの行・列の計算のテストがある。CRLF、最終行に改行が無い場合、空のファイル、不正な UTF-8 を含む。
- C-2 診断の JSON 表示のゴールデンテストがある (主ラベル + 副ラベル + 注記 + fix-it、複数行にまたがる span)。単一行の人間向け表示のゴールデンテストもある。
- C-3 `DiagSink::into_sorted` の順序が、発行順の入れ替えに依存しない。
- C-4 インターン: 同じ文字列は同じ `Symbol` になり、`resolve` で元の文字列に戻る。
- C-5 `WordArena` について、次を確認する。
  - 位置合わせ付きの確保ができ、溢れが検出される。
  - `disjoint_mut` が重なりを検出する。
  - スナップショット → 変更 → 復元で、`content_hash` が元に戻る。
  - アリーナ上の 4 値 / 2 値の信号に、`svsim-logic` のカーネルを in-place で適用できる (結合テスト)。

### 9.4 コーパスマネージャ
- K-1 `cargo corpus fetch sv-tests` が、固定コミット `7274242b…` を `~/.cache/svsim/corpus/sv-tests/<rev>/` (または `$SVSIM_CORPUS_CACHE`) に取得し、`git rev-parse HEAD` の検証に通る。2 回目はネットワークに出ずにすぐ終わる (`--offline` でも成功する)。
- K-2 `cargo corpus discover sv-tests` が **1028 件**を列挙し、メタデータの解析エラーが 0 件。`should_fail_because` を持つケースが 79 件、`:type:` の分布が §7.3 の内訳と一致する。
- K-3 `cargo corpus run sv-tests` が `results.jsonl` と `summary.json` を出し、全ケースが `Preprocess: NotImplemented` になる。2 回実行して、所要時間以外が同一 (決定性)。
- K-4 マニフェストの誤り (rev がコミット ID でない、名前の重複、未知のキー) が、エラーとして報告される。
- K-5 古いロック (存在しない PID) が自動で取り除かれ、生きているプロセスのロックではエラーになる。
- K-6 CI の `corpus-fetch` ジョブが緑。

---

## 10. リスクと未決事項

### 10.1 ユーザ・マネージャの判断事項
| ID | 事項 | 状態 |
|---|---|---|
| U-1 | 4 値の内部配置 (§3.2 D-1) | **決定**: 64 ビットのプレーン分離を採用。4 値は DPI の境界で変換し、2 値は input に限りゼロコピー。PLAN.md §3/§6.1 を修正する (マネージャ) |
| U-2 | M0 の完了条件 | **決定**: sv-tests を取得できる状態まで。パースの計測は M1。PLAN.md §11 を更新する (マネージャ) |
| U-3 | ツールチェーン | **決定**: 1.97.0 に固定 (`rust-toolchain.toml`) |
| U-4 | CI | **決定**: GitHub Actions を使う |
| U-5 | action の固定 | **決定**: コミット SHA で固定 (§6) |
| U-6 | 処理系依存の上限: `MAX_WIDTH`、32 ビットを超えるサイズなしリテラル | **決定: 2^24 (マネージャ)**。暫定は `MAX_WIDTH = 2^24`、サイズなしリテラルは広げて警告。判断材料 (R-27): Verilator の既定の上限は 65536 ビットで、それを超えるとエラーにする。2^24 でも内部の型には問題が無い。上限近くの動作は §5.4 のスモークテスト (2^20) で確認する。VCS の上限は V-11 で確認する |
| U-7 | 規格で決まっていない挙動 | **決定**: M1 で確認用の SV テスト集を用意し、ユーザが業務環境の VCS で結果を確認する。それまでは暫定の挙動で実装し、テストに [要VCS確認] の印を付ける (§11) |
| **R-21** | **業務の DPI コードが、4 値の open array に `svGetArrayPtr`/`svGetArrElemPtr` を使い、得たポインタを `svLogicVecVal*` として読み書きしているか** | **要ユーザ確認** (マネージャが確認する)。使っている場合、D-1 の配置ではその C コードが動かない (得られるのは実装の表現へのポインタになるため)。そのときは、open array の要素だけを正規の表現 (`svLogicVecVal`) で持つ選択肢を M3 で検討する。M3 の計画の入力にする |

### 10.2 question への回答
- **R-26** (幅 ≤ 32 の 4 値を u64 1 個に詰める案の余地): 残す。§3.4 に「層 1 は記憶域の配置から独立している」と明記したので、M5 で記憶域の配置だけを変えられる。M0 では採らない。
- **R-27** (`MAX_WIDTH`): §10.1 の U-6 に判断材料を記録した。上限近くのスモークテストを §5.4 に追加した。

### 10.3 その他のリスク
- **条番号の照合**: 規格本文は、計画作成時もレビュー時も参照できなかった。冒頭の注記の「未検証」の番号は、規格を参照できるようになった時点で照合する。コードとテストには表番号を書かない。
- **参照モデルが同じ誤解を共有する危険**: 参照モデルを別のエージェントに書かせることで軽減した (T9)。それでも規格の読み違いは両方に入りうるので、ゴールデンテスト (規格の例) と、M1 以降の VCS/Icarus との出力比較 (§11) で補う。
- **多倍長の除算と表示の性能**: Knuth D と、10^19 での繰り返し除算は O(n^2)。M0 の用途 (幅 ≤ 数千) では十分。巨大な幅の `%d` 表示が遅いと分かったら、分割統治に替える (後で)。
- **sv-tests の固定コミット**: 上流でファイル構成が変わると、K-2 の件数が変わる。固定コミットを更新するときは、期待値も一緒に更新する。
- **worktree の並列ビルドの負荷**: `target/` を共有しないので、4 本の並列ビルドでディスクと CPU を使う。マシンの負荷が問題になったら、同時実行の本数を減らす。

---

## 11. VCS 確認リスト (M1 で確認用テスト集にする)
次の項目について、M1 で小さな SV テストを作る。各テストは `$display` で結果を出し、ユーザが業務環境の VCS で実行して結果を確認する (決定 U-7)。

それまでは「暫定」の列の挙動で実装し、該当するテストに `[要VCS確認 V-n]` の印を付ける。VCS の結果が暫定と違えば、`--compat=vcs` の既定の挙動として取り込むかを、その時点で判断する。

| ID | 確認する挙動 | 暫定の挙動 (sv-sim) | 他実装の情報 | 計画の箇所 |
|---|---|---|---|---|
| V-1 | 符号付き除算の最小値 / -1 (`8'sh80 / -8'sd1` 等)、同じ組み合わせの剰余 | 2 の補数で折り返す (結果は最小値)、剰余は 0 | slang の手順では最小値 | §3.5(c) |
| V-2 | 2 値の除算・剰余の 0 除算 (`int a = b / 0;`)、2 値で `0 ** -1` | 0 | Verilator は 0 | §3.5(c) |
| V-3 | `>>>` で符号ビットが Z (`4'sbz000 >>> 1`)、シフト量が幅以上の場合 | Z で埋める | slang は Z で埋める | §3.5(d) |
| V-4 | 実数の NaN、±∞ を整数に代入、キャスト、`$rtoi` | 0 | — | §3.5(i) |
| V-5 | X/Z を含む整数を `string` に変換 (`string'(16'h41xx)` 等)。0 のバイトを含む場合 | X/Z は 0 として扱い、0 のバイトは捨てる | — | §3.5(h) |
| V-6 | `atoi` 系: 先頭の `-`/`+`、先頭の空白、`atohex` の `0x` 接頭辞、32 ビットを超える値 | 符号、空白、接頭辞は解釈しない (そこで止まる)。超えた分は切り詰める | Icarus は符号を解釈しない。Verilator は `strtol` 相当 (符号、空白、`0x` を受ける) | §3.5(h) |
| V-7 | 表示の幅: `%d` の自動幅 (符号付き/符号なし、X/Z の場合)、`%5d`、`%05d`、`%5h`、`%0h` で上位の桁が X/Z の場合、`%-5d` が使えるか | §3.9 の規則 3〜5 | Icarus と Verilator は暫定と一致 | §3.9 |
| V-8 | `?:` で条件が X、両方のビットが z (`1'bx ? 1'bz : 1'bz`)、片方だけ z の場合 | X (規格の表を優先) | **Icarus は z** (既知の差として記録)。slang は値全体が一致すれば z | §3.5(f) |
| V-9 | 2 値のデータに 4 値の自己決定オペランドが来る式: `bit [7:0] b; logic [2:0] s = 'x;` のときの `b << s`、`b >>> s`、`b ** s`、`x ? b1 : b2`、`b[s]` (X のインデックス)、`b[s+:2]`。それぞれを `$display("%b")` で直接表示した結果 (中間値が 4 値か) と、`bit`/`logic` の変数に代入した結果 | §3.5.1: 4 値で評価する (表示は x)。2 値の変数への代入で 0 になる。選択だけは 2 値の 0 | — | §3.5.1 |
| V-10 | `$clog2` に X/Z を含む値を渡した場合 (定数式と実行時の両方)、`$clog2(0)` | `$clog2(0)` = 0。X/Z は呼び出し側の判断 (定数式なら診断、実行時なら X を予定) | — | §3.5(e) |
| V-11 | サイズなしリテラルが 32 ビットを超える場合 (`'h1_0000_0000`、`4294967296`)、`'hx` を 64 ビットの文脈で使った場合の拡張、VCS のベクタ幅の上限 | 必要な幅まで広げて警告する。`'hx` は文脈幅まで x で拡張する | Verilator は広げる。上限は 65536 ビット | §3.5(g)、U-6 |
| V-12 | エッジ検出: x→z、z→x、z→1、1→z の遷移での `posedge`/`negedge`、多ビットの式 (LSB だけを見るか) | §3.5(k) の表 | — | §3.5(k) |
| V-13 | `%h`/`%o` の桁に X と Z が混在する場合 (`4'bxz01` → `X`)、最上位の端数桁 | X を優先 (§3.9 の規則 1) | Icarus は暫定と一致 | §3.9 |
| V-14 | X/Z を含む整数を `real` に変換 (`real r = 8'b1x01;`) | X/Z のビットを 0 として扱う | — | §3.5(i) |
