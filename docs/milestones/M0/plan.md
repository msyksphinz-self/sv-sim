# M0「基盤」詳細計画 (M0-P)

- 作成: 計画エージェント (planner)、2026-11-27
- 対象: `docs/PLAN.md` §1, §3, §10.1, §11 の M0
- 状態: ドラフト (計画レビュー M0-R 待ち)

> **条番号について**: 本計画の IEEE 1800 の条番号は IEEE 1800-2017 の構成に基づいている
> (1800-2023 では第 5〜21 章の該当節の番号は変わっていない想定)。計画作成時に規格本文を参照できなかったため、
> 番号と表番号の照合を計画レビュー (M0-R) で行うこと。sv-tests のファイル名 (`tests/chapter-*/<条番号>--*.sv`) で
> 番号を確認できたもの (6.16.9〜6.16.15、11.4.x、11.5.1、20.5、20.9 など) は確度が高い。
> 規格が処理系依存としているもの、または規格の記述を確認できなかったものは、本文で **[要確認]** と書き、§10 にまとめた。

---

## 1. 目的とスコープ

### 1.1 目的
後続の全マイルストーンが依存する基盤を作る。特に `svsim-logic` は、定数評価 (M1 sema)、インタプリタ (M1)、
JIT (M5)、DPI (M3/M7)、デバッグ API (M4) のすべてが使う値の表現と演算なので、M0 で**完全版**として作り、
参照モデルとの差分テストで正しさを固める。

### 1.2 M0 でやること
| 項目 | 内容 |
|---|---|
| Cargo workspace | `crates/svsim-common`、`crates/svsim-logic`、`crates/svsim-logic-ref` (テスト専用の参照モデル)、`crates/svsim-corpus`。ツールチェーン固定、lint 設定 |
| `svsim-logic` | 4 値 / 2 値ビットベクタの表現と全演算 (§3)。owned 値型、スライスビュー、≤64 ビットのスカラ高速パス、DPI 形式との変換、リテラル解析、実数・文字列変換、表示用フォーマット |
| `svsim-logic-ref` | 1 ビットずつ規格の真理値表をそのまま書いた参照モデル (差分テストのオラクル) |
| `svsim-common` | ソースマップ、診断 (人間向け表示 + JSON)、文字列インターン、型付きインデックス配列、状態アリーナ (`WordArena`) の基礎 |
| CI | GitHub Actions: fmt / clippy / build / test / doc / ライセンス検査 / コーパス取得 / 定期の長時間 proptest |
| コーパスマネージャ雛形 | マニフェスト (TOML)、git 取得とキャッシュ、テストケースの列挙 (sv-tests アダプタ)、段階評価の枠組み (M0 では全段階 `NotImplemented`)、結果の JSONL と集計 |

### 1.3 M0 でやらないこと (後のマイルストーンに回す)
| 項目 | 回す先 | 備考 |
|---|---|---|
| プリプロセッサ、パーサ、sv-tests のパース段階の計測 | M1 | M0 は「取得とファイル列挙」まで。段階評価の枠組みにパース段階を差し込めるようにしておく |
| 強度付き値 (strength) と解決関数 | M2 | `svsim-logic` の 4 値表現とは別の型にする (§3.11) |
| `real` の表示 (`%e %f %g`)、`%s %c %t %m %v %p %u %z`、`$display` 本体 | M1 | M0 は整数値の `%b %o %h %d` の数値部分の書式化だけ (§3.9) |
| ストリーミング演算子 (§11.4.14)、`inside` (§11.4.13)、struct/配列の集合演算 | M3 | 型情報が必要。M0 の基本演算 (連結、ワイルドカード等価) の組合せで実装できる |
| JIT 向け `extern "C"` ヘルパ、Cranelift コード生成 | M5 | M0 はその前提になる「メモリ配置の契約」と「スカラ式の仕様」(§3.3, §3.4) を固める |
| チェックポイントの差分スナップショット、パーティション別アリーナ、キャッシュライン境界の確保 | M4 / M6 | M0 は全体コピーのスナップショットまで |
| 並列インターン (並列パース用) | M1 以降 | M0 は単一スレッド版。§4.3 参照 |
| Icarus / Verilator / VCS との出力比較 | M1 以降 | M0 の差分テストは参照モデルと num-bigint |
| ベンチマーク | M5 で本格化 | M0 では作らない (正しさ優先) |

### 1.4 完了条件 (マネージャ指示)
1. `svsim-logic` の差分テスト・proptest が全通過。
2. sv-tests を固定コミットで取得し、テストケースを列挙できる。

> **注**: `docs/PLAN.md` §11 の M0 完了条件は「sv-tests のパース段階を計測開始」となっているが、
> M0 にはパーサが無いので、マネージャ指示の「sv-tests を取得できる状態」を完了条件とする。
> PLAN.md の表の更新が必要 (マネージャ作業。§10 U-2)。

---

## 2. workspace 構成

### 2.1 ディレクトリ
```
Cargo.toml                    workspace (resolver = "3", edition 2024)
rust-toolchain.toml           channel = "1.97.0", components = ["rustfmt", "clippy"]
rustfmt.toml                  (既定のまま。edition = "2024" だけ)
deny.toml                     cargo-deny (ライセンス許可リスト等)
.gitignore                    /target, /.corpus-cache, /corpus/private/
.github/workflows/ci.yml
crates/
  svsim-common/               依存: serde, serde_json, rustc-hash
  svsim-logic/                依存: なし (dev: svsim-logic-ref, proptest, num-bigint, num-traits)
  svsim-logic-ref/            依存: num-bigint, num-traits  (publish = false、テスト専用)
  svsim-corpus/               依存: serde, toml, serde_json, clap, walkdir, globset (bin + lib)
corpus/
  manifest.toml               公開コーパスのマニフェスト (sv-tests を最初のエントリに)
  private/                    ローカル専用マニフェスト置き場 (gitignore)
```
- `svsim-logic` は**依存ゼロ**にする。JIT のランタイムや DPI の共有ライブラリ側からも軽くリンクできるようにするため。
- `svsim-common` は `svsim-logic` に依存しない (逆も依存しない)。アリーナとロジック値ビューの結合テストだけ
  `svsim-common` の dev-dependency に `svsim-logic` を入れる (循環にならない)。
- 全 crate に `#![forbid(unsafe_code)]` を付ける。M0 の範囲には unsafe が要る箇所が無い
  (アリーナは `Vec<u64>` で足りる。キャッシュライン境界の確保は M6 で、そのときに unsafe と `// SAFETY:` を入れる)。
- 依存 crate はすべて MIT / Apache-2.0 互換 (serde, serde_json, toml, clap, num-bigint, num-traits, proptest,
  rustc-hash: MIT OR Apache-2.0。walkdir, globset: Unlicense OR MIT)。

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
missing_docs = "warn"            # clippy -D warnings で実質エラー。公開 API の doc コメント必須 (CLAUDE.md §3)
unsafe_code = "forbid"

[workspace.lints.clippy]
# 既定 + 以下
undocumented_unsafe_blocks = "deny"   # 将来 unsafe を入れたとき用

[workspace.dependencies]
# バージョンは M0-T1 で、その時点の最新安定版を選んで固定する (Cargo.lock をコミット)
```
- **並列実装の前提**: 依存 crate の追加は M0-T1 で全部済ませ、`Cargo.lock` を確定させる。
  以降のタスクは依存を追加しない (必要になったら報告してマネージャが判断)。並列タスクが `Cargo.lock` を同時に書き換えないため。

---

## 3. `svsim-logic` 設計

### 3.1 値の表現 (エンコーディング)
1 ビットを aval/bval の 2 ビットで表す。VPI `s_vpi_vecval`、DPI `svLogicVecVal` (§H.10.1.2, `svdpi.h`) と同じ割り当て。

| 値 | aval | bval | `Logic` (u8) |
|---|---|---|---|
| 0 | 0 | 0 | 0 |
| 1 | 1 | 0 | 1 |
| Z | 0 | 1 | 2 |
| X | 1 | 1 | 3 |

- `Logic` を `#[repr(u8)]` で `aval | (bval << 1)` にすると、`svdpi.h` の `sv_0=0, sv_1=1, sv_z=2, sv_x=3` と一致する
  (スカラ `svLogic` はそのまま渡せる)。
- 「値が未知 (X または Z)」の判定は `bval` だけで済む。

### 3.2 メモリ配置 (配置の契約)
幅 `w` (ビット、`u32`) の値のワード数 `nw = ceil(w / 64)`。ワードは `u64`、リトルエンディアンのビット順
(ビット 0 = LSB = ワード 0 のビット 0)。

| 種別 | 配置 (u64 の並び) | ワード数 |
|---|---|---|
| 4 値 (`logic` 等) | `[a_0, …, a_{nw-1}, b_0, …, b_{nw-1}]` (**プレーン分離**) | `2·nw` |
| 2 値 (`bit` 等) | `[a_0, …, a_{nw-1}]` | `nw` |

- **正規化不変条件**: 最上位ワードの `w` 以上のビットは、両プレーンとも常に 0。全演算は正規化された入力から
  正規化された出力を作る。差分テストでこの不変条件も検査する。
- 幅 ≤ 64 も同じ規則 (4 値は `[a, b]` の 16 バイト、2 値は 8 バイト)。アリーナ上の信号・JIT・owned 値型で規則を揃える。
- 2 値の配置はリトルエンディアンの `u32` 配列と同じバイト列になるので、`svBitVecVal` (§H.10.1.1) とは
  **ゼロコピー**で受け渡せる (最上位ワードの上位 32 ビットの扱いだけ注意。§3.10)。
- 幅 0 は「空の値」として表現だけ許す (`nw = 0`)。0 回の複製 `{0{x}}` が連結の中で無視される規則 (§11.4.12.1) のため。
  連結以外の演算に幅 0 を渡すのは呼び出し側の誤りとし、`debug_assert!` で検出する。
- 最大幅 `MAX_WIDTH = 1 << 24` ビット (規格はベクタ長の上限を少なくとも 2^16 ビットとすることを要求: §6.9.1)。[要確認 U-6]

#### 設計判断 D-1: 4 値の配置は 64 ビットのプレーン分離にし、`svLogicVecVal` とはゼロコピーにしない
`svLogicVecVal` は `{uint32_t aval; uint32_t bval;}` の配列で、**32 ビットごとに aval と bval が交互に並ぶ**。
これをそのまま内部表現にすると、

- 64 ビット信号 (`logic [63:0]` はよくある) の aval を 1 回のロードで取り出せない (32 ビット × 2 + 結合)。
- 加算の桁上げやシフトが 32 ビット単位になり、ワード並列の効率が半分になる。
- 2 値のコードパス (aval だけの連続配列) を 4 値の aval プレーンに再利用できない。

一方、変換コストは 64 ビットあたりシャッフル数命令で、DPI で多い 32〜512 ビットの引数なら数〜十数 ns 程度。
そこで、**内部は 64 ビットのプレーン分離、DPI 境界で `svLogicVecVal` との変換** (§3.10) とする。
2 値 (`svBitVecVal`) はゼロコピー。PLAN.md §3「メモリ表現が一致」・§6.1「引数の変換なし」の記述と食い違うので
**要ユーザ確認 (U-1)**。代替案 (32 ビット交互配置を内部表現にする) を選ぶ場合、§3.3〜3.4 の API 形は変わらないが、
カーネルの実装と §3.4 のスカラ式が全部変わる。

### 3.3 型と API の層
```
 層 1: スカラ高速パス (幅 ≤ 64)   scalar::L64 { a: u64, b: u64 } / u64       … JIT のコード生成の仕様、インタプリタの高速パス
 層 2: スライスカーネル (任意幅)    LogicRef / LogicMut / BitRef / BitMut を受ける自由関数  … アリーナ上で in-place
 層 3: owned 値型                LogicVec / BitVec (≤64 はインライン)           … 定数評価、インタプリタの一時値、テスト
```
- 層 3 は層 1・層 2 を呼ぶだけの薄い包み。層 1 と層 2 は**ヒープ確保をしない** (乗除算・べき乗・書式化は
  呼び出し側が渡す `Scratch` を使う)。アリーナ上での in-place 演算と、将来の JIT ヘルパ (`extern "C"`、M5) は層 2 を包む。
- 値は**符号の情報を持たない**。符号付き/符号なしは型の性質で、sema が決める (§11.8.1)。
  符号で結果が変わる演算 (除算、剰余、比較、算術右シフト、拡張、変換、`%d`) は `signed: bool` 引数を取る。
- 2 値と 4 値は**型を分ける** (ジェネリクスにしない)。理由: (1) JIT から呼ぶ関数は単相で C ABI にしやすい、
  (2) 4 値の算術は「bval に 1 があれば全 X、無ければ aval プレーンに 2 値カーネルを適用」で書けるので、
  内部の 2 値ワード演算 (`words` モジュール) を共有すれば重複は小さい、(3) 2 値と 4 値の混在は sema が型変換を
  挿入するので、ライブラリ側で混在演算を持つ必要がない。

#### 主要な型 (シグネチャ例)
```rust
/// 1 ビットの 4 値。repr は svdpi.h の sv_0/sv_1/sv_z/sv_x と一致。
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Logic { L0 = 0, L1 = 1, Z = 2, X = 3 }

pub const MAX_WIDTH: u32 = 1 << 24;
pub const fn words_for(width: u32) -> usize;          // ceil(width/64)
pub const fn top_mask(width: u32) -> u64;             // 最上位ワードの有効ビットマスク (width%64==0 なら !0)

/// 4 値の読み取りビュー。a/b は同じ長さ words_for(width)。プレーンは隣接していなくてよい。
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
    pub fn copy_from(&mut self, src: LogicRef<'_>);   // 幅は一致必須
}

/// 4 値の owned 値。幅 ≤ 64 はヒープ確保しない。
#[derive(Clone, PartialEq, Eq, Hash)]   // PartialEq は「全ビットが同一 (===)」の意味
pub struct LogicVec { width: u32, store: Store }
enum Store { Inline { a: u64, b: u64 }, Heap(Box<[u64]>) /* planar, len = 2*nw */ }
pub struct BitVec  { width: u32, store: Store2 }
enum Store2 { Inline(u64), Heap(Box<[u64]>) }

impl LogicVec {
    pub fn zero(width: u32) -> Self;
    pub fn filled(width: u32, v: Logic) -> Self;
    pub fn from_u64(width: u32, v: u64) -> Self;      // 幅に切り詰め
    pub fn from_i64(width: u32, v: i64) -> Self;      // 2 の補数で幅に切り詰め/符号拡張
    pub fn from_bits_str(s: &str) -> Result<Self, ParseBitsError>; // "10xz_01" (MSB 先頭)。テスト・デバッグ用
    pub fn as_ref(&self) -> LogicRef<'_>;
    pub fn as_mut(&mut self) -> LogicMut<'_>;
    pub fn to_u64(&self) -> Option<u64>;              // X/Z があるか 64 ビットを超える 1 があれば None
    pub fn to_i64(&self, signed: bool) -> Option<i64>;
    pub fn to_bit_vec(&self) -> BitVec;               // X/Z → 0 (§6.11.2)
}
impl From<&BitVec> for LogicVec { /* bval = 0 */ }

/// 乗除算・べき乗・10 進書式化などの作業領域。呼び出し側が使い回す (ホットループで確保しない)。
pub struct Scratch { buf: Vec<u64> }
```
- `Debug` は `8'b10xz_0001` 形式、`Display` は 2 進 (`%b` 相当) にする。
- 層 3 の便利メソッド (`a.add(&b)` など) は、各演算モジュールの中に `impl LogicVec { … }` ブロックとして書く
  (Rust は同じ crate 内なら impl ブロックを複数ファイルに分けられる)。並列タスクが同じファイルを触らないため。

#### スライスカーネルの共通規約
```rust
// 例: 文脈決定 (context-determined) の 2 項演算。dst, x, y の幅は全部同じでなければならない。
pub fn add(dst: LogicMut<'_>, x: LogicRef<'_>, y: LogicRef<'_>);
pub fn add2(dst: BitMut<'_>, x: BitRef<'_>, y: BitRef<'_>);      // 2 値版は末尾 2
pub fn mul(dst: LogicMut<'_>, x: LogicRef<'_>, y: LogicRef<'_>, s: &mut Scratch);
pub fn lt(x: LogicRef<'_>, y: LogicRef<'_>, signed: bool) -> Logic;
pub fn lt2(x: BitRef<'_>, y: BitRef<'_>, signed: bool) -> bool;
```
1. **幅の規則は呼び出し側 (sema / MIR) が適用する**。§11.6 の文脈決定の演算は、オペランドを先に結果幅へ拡張
   (§3.5 `resize`) してから同じ幅のカーネルを呼ぶ。カーネルは `assert!(x.width == y.width && …)` で検査する
   (比較 1 回なので release でも残す。スライスの境界検査があるので違反しても UB にはならない)。
   自己決定 (self-determined) のオペランド (シフト量、べき乗の指数、連結の要素、選択のインデックス) だけは別の幅を受ける。
2. Rust の借用規則により `dst` は入力と重ならない。in-place が多い演算 (ビット演算、加減算、シフト) には
   `and_assign(dst_x: LogicMut, y: LogicRef)` のような `_assign` 版を用意する。アリーナの同じバッファから dst と src を
   同時に取るときは `<[u64]>::get_disjoint_mut` (Rust 1.86 で安定化) を使う (§4.4)。
3. カーネルは panic しない (上記の幅 assert を除く)。0 除算などの意味論上の例外は値 (X) で返す。

### 3.4 スカラ高速パス (幅 ≤ 64) とその式
`scalar` モジュールに、`L64 { a: u64, b: u64 }` (4 値) と `u64` (2 値) に対する演算を `#[inline]` の自由関数で置く。
幅 `w` (1〜64) を引数に取り、結果を `m = top_mask(w)` でマスクする。これは

- インタプリタと `LogicVec` の幅 ≤ 64 の高速パス、
- **M5 の JIT が Cranelift IR で生成する式の仕様** (JIT 出力をこの関数と差分テストする)

を兼ねる。主な式 (x = (ax, bx), y = (ay, by)、`!` はビット反転):

| 演算 | aval | bval | 根拠 |
|---|---|---|---|
| `~x` | `(!ax \| bx) & m` | `bx` | §11.4.8 (否定の表) |
| `x & y` | `A = (ax\|bx) & (ay\|by)` | `A & (bx\|by)` | §11.4.8 (AND の表: 0 が勝つ) |
| `x \| y` | `ax\|bx\|ay\|by` | `(bx\|by) & !((ax&!bx)\|(ay&!by))` | §11.4.8 (OR の表: 1 が勝つ) |
| `x ^ y` | `((ax^ay) \| B) & m` | `B = bx\|by` | §11.4.8 |
| `x ~^ y` | `(!(ax^ay) \| B) & m` | `B = bx\|by` | §11.4.8 |
| `c ? t : f` (c が曖昧) | `((at & S) \| !S) & m` | `!S & m` | §11.4.11 表 (S = `!(at^af) & !(bt^bf) & !bt`) |
| 算術 `+ - * / % **` | `bx\|by != 0` なら `m`、そうでなければ 2 値演算 `& m` | 同 `m` / `0` | §11.4.3 |

- 1 ビットの結果を返す演算 (比較・リダクション・論理演算) は `Logic` を返す。
- Z は入力としては X と同じに扱い (aval を見ずに bval で判定)、ビット演算の結果に Z は出ない (§11.4.8 の表で z 入力は x と同じ行)。
  ただし**データを移すだけの演算** (シフト、連結、複製、選択、拡張、`?:` で条件が確定している場合) は Z を Z のまま運ぶ。

### 3.5 演算一覧 (規格の意味論)
凡例: **CD** = 文脈決定 (全オペランドと結果が同じ幅 `w`、呼び出し側で拡張済み)、**SD** = 自己決定。
「X 規則」は 4 値版の規則。2 値版は X/Z が入力に無いので該当しない (X が出る場合は §6.11.2 により 0、下表に明記)。

#### (a) ビット演算・リダクション・論理演算 (タスク M0-T4)
| 演算 | 関数 | 幅 | X 規則 | 条 |
|---|---|---|---|---|
| `~` | `not` | CD | 表どおり (§3.4) | §11.4.8 |
| `& \| ^ ~^ ^~` | `and or xor xnor` | CD | 表どおり | §11.4.8 |
| `& ~& \| ~\| ^ ~^` (単項) | `reduce_and/nand/or/nor/xor/xnor` → `Logic` | SD 入力、1 ビット結果 | AND: 既知の 0 が 1 つでもあれば 0、無くて未知があれば X。OR: 既知の 1 があれば 1、無くて未知があれば X。XOR: 未知があれば X、無ければパリティ | §11.4.9 |
| 真理値 (`if (e)` 等) | `truth(x) -> Logic` | SD | 既知の 1 があれば 1、全ビット既知の 0 なら 0、それ以外 X | §11.4.7, §12.4 |
| `! && \|\| -> <->` | `Logic::{not, and, or, implies, equiv}` (`truth` の結果に適用) | 1 ビット | `&&` は AND の表、`\|\|` は OR の表。`a -> b` = `!a \|\| b`、`a <-> b` = `(a -> b) && (b -> a)` | §11.4.7 |
| `$countones` `$countbits` `$onehot` `$onehot0` `$isunknown` | `count_ones`, `count_bits(x, mask: LogicSet)`, `onehot`, `onehot0`, `is_unknown` | SD | `$countbits(e, ...)` は指定した値 (0/1/X/Z の集合) を持つビット数。`$onehot(e)` = `$countbits(e,'1)==1`、`$onehot0` = `<=1`、`$isunknown` = `$countbits(e,'x,'z)!=0` | §20.9 |

- 短絡評価 (§11.3.5) は評価器の責務。ライブラリは両オペランドの真理値を受け取るだけ。

#### (b) 比較・等価 (タスク M0-T4)
| 演算 | 関数 | 幅 | X 規則 | 条 |
|---|---|---|---|---|
| `==` `!=` | `eq`, `ne` → `Logic` | CD 入力 (呼び出し側で最大幅へ拡張)、1 ビット結果 | 既知ビットで不一致があれば 0 (`!=` は 1)。無くて未知ビットがあれば X。全既知で一致なら 1 | §11.4.5 |
| `===` `!==` | `case_eq`, `case_ne` → `bool` | 同上 | aval・bval とも完全一致 (X は X とだけ、Z は Z とだけ一致) | §11.4.5 |
| `==?` `!=?` | `wildcard_eq`, `wildcard_ne` → `Logic` | 同上 | **右**オペランドの X/Z ビットはワイルドカード (比較しない)。残りで: 既知の不一致 → 0、左の未知 → X、他は 1 | §11.4.6 |
| `casez` / `casex` の照合 | `casez_match`, `casex_match` → `bool` | 同上 | casez: どちらかが Z のビット (と `?`) は無視、残りは `===`。casex: どちらかが X/Z のビットは無視 | §12.5.1 |
| `< <= > >=` | `lt le gt ge (x, y, signed)` → `Logic` | 同上 | どちらかに未知ビットがあれば X。符号付き比較は**両方が符号付きのときだけ** (呼び出し側が `signed` を決める) | §11.4.4, §11.8.1 |

- `case` 文の照合は `case_eq` (§12.5)。`inside` (§11.4.13) は `wildcard_eq` と比較を組み合わせて M3 で作る。

#### (c) 算術 (タスク M0-T5)
| 演算 | 関数 | 幅 | X 規則 / 特殊値 | 条 |
|---|---|---|---|---|
| `+ -` (2 項) | `add`, `sub` | CD | どこかに未知ビット → 全 X。結果は幅 `w` で 2 の補数の切り詰め (オーバフローは捨てる) | §11.4.3 |
| `-` `+` (単項) | `neg` (`0 - x`) / 恒等 | CD | 同上 | §11.4.3 |
| `++ --` | `inc`, `dec` (`_assign` 版) | CD | 同上 | §11.4.2 |
| `*` | `mul(…, &mut Scratch)` | CD | 同上。下位 `w` ビットだけ計算 (符号付き/符号なしで下位ビットは同じなので `signed` 不要) | §11.4.3 |
| `/` | `div(…, signed, &mut Scratch)` | CD | 同上 + **除数 0 → 全 X**。0 方向への切り捨て。符号付きの最小値 / -1 は 2 の補数で折り返す (結果 = 最小値) [要確認 U-7] | §11.4.3 |
| `%` | `rem(…, signed, &mut Scratch)` | CD | 同上 + 除数 0 → 全 X。結果の符号は第 1 オペランドの符号 | §11.4.3 |
| `**` | `pow(dst, base, base_signed, exp: LogicRef(SD), exp_signed, &mut Scratch)` | 結果 = base の幅 (CD)、指数は SD | どちらかに未知 → 全 X。指数が負 (指数が符号付きで負) のとき: base=0 → 全 X、base=1 → 1、base=-1 → 指数が偶数なら 1・奇数なら -1、\|base\|>1 → 0。指数 0 → 1 (base=0 でも 1) | §11.4.3 (べき乗の規則表。表番号は要照合) |

- 2 値版 (`div2`, `rem2`) の除数 0: 規格上の結果は X で、2 値の型へは 0 として格納される (§6.11.2) ので **0 を返す**。[要確認 U-7]
- べき乗は二乗法で `w` ビットの剰余環上で計算する。指数が `w` 以上で base が偶数なら 0 を早期に返してよい。
- 多倍長の除算は Knuth の Algorithm D (`u64` ワード、`u128` 中間値)。符号付きは絶対値で計算して符号を戻す。
- `Scratch` に必要なワード数は `div`: `2·nw + 1` 程度、`mul`: `nw` 程度。`Scratch::reserve_for(width)` で事前確保できる。

#### (d) シフト (タスク M0-T6)
| 演算 | 関数 | 幅 | 規則 | 条 |
|---|---|---|---|---|
| `<<` `<<<` | `shl(dst, x, amt: LogicRef)`, `shl_const(dst, x, n: u64)` | x と結果は CD、シフト量は SD・常に符号なし | シフト量に未知ビット → 全 X。シフト量 ≥ `w` → 全 0。空いたビットは 0。x の X/Z はそのまま移動 | §11.4.10 |
| `>>` | `shr` / `shr_const` | 同上 | 空いたビットは 0 | §11.4.10 |
| `>>>` | `ashr(dst, x, amt, signed)` / `ashr_const` | 同上 | 結果の型が符号付きなら符号ビットの値 (X/Z を含めてそのまま) で埋める。符号なしなら `>>` と同じ。シフト量 ≥ `w` なら全ビットが符号ビットの値 | §11.4.10 |
- シフト量が 64 ビットを超える幅でも、上位ワードに 1 があれば「≥ `w`」として扱う (切り詰めない)。
- 符号ビットが Z のときの `>>>` で Z を埋める点は「符号ビットの値で埋める」の字義どおりの解釈。[要確認 U-7]

#### (e) 幅の変更・連結・複製・選択 (タスク M0-T6)
| 演算 | 関数 | 規則 | 条 |
|---|---|---|---|
| 拡張・切り詰め | `resize(dst, x, signed)` (dst の幅が目標幅) | 切り詰めは下位ビットを残す。拡張は符号なしなら 0、符号付きなら符号ビットの値 (aval・bval とも) を複製 → 符号ビットが X なら X、Z なら Z で埋まる | §11.6.1, §11.8.2, §11.8.4, §6.24.1 (サイズキャスト) |
| 4 値 → 2 値 | `to_two_state(dst: BitMut, x)` | X/Z → 0 | §6.11.2 |
| 2 値 → 4 値 | `from_two_state(dst: LogicMut, x: BitRef)` | bval = 0 | §6.11.2 |
| 連結 `{a, b, …}` | `concat(dst, parts: &[LogicRef])` | 先頭要素が MSB 側。dst の幅 = 各要素の幅の和。幅 0 の要素は無視 | §11.4.12 |
| 複製 `{n{a}}` | `replicate(dst, x, n: u32)` | n = 0 は幅 0 の値 (連結の中でだけ許される。検査は sema) | §11.4.12.1 |
| ビット選択・部分選択 (読み) | `get_slice(dst, src, offset: i64)` (dst の幅 = 選択幅)、`get_bit(src, offset: i64) -> Logic` | `offset` は LSB を 0 とする物理オフセット (宣言範囲 `[msb:lsb]` や `+:` `-:` からの変換は sema/MIR の責務)。範囲外のビットは **4 値なら X、2 値なら 0**。完全に範囲外でも同じ | §11.5.1 |
| 同 (書き) | `set_slice(dst, offset: i64, src)`, `set_bit` | 範囲内のビットだけ書き、範囲外は無視 | §11.5.1 |
| インデックスの解釈 | `index_value(idx: LogicRef, signed) -> Index` (`enum Index { Unknown, Value(i64) }`) | インデックスに X/Z → `Unknown` (読みは全 X/0、書きは無効: §11.5.1)。i64 に収まらない値は飽和させる (どのみち範囲外) | §11.5.1 |
| 実数の選択 | (なし) | 実数へのビット選択は不正 (§11.5.1)。sema で弾く | — |

#### (f) 条件演算子 (タスク M0-T4)
| 演算 | 関数 | 規則 | 条 |
|---|---|---|---|
| `c ? t : f` で c が X/Z | `cond_merge(dst, t, f)` | ビットごとに: 両方 0 → 0、両方 1 → 1、それ以外 (Z と Z も含む) → X | §11.4.11 (曖昧な条件の結果の表) |
- 条件の真理値は `truth(c)`。真なら `t`、偽なら `f` を評価器が選び、X のときだけ `cond_merge` を呼ぶ。
  t と f の幅は呼び出し側で揃える。実数など整数型でないオペランドの扱い (§11.4.11) は評価器の責務。

#### (g) リテラル・整数変換 (タスク M0-T7)
| 機能 | 関数 | 規則 | 条 |
|---|---|---|---|
| 基数付きリテラル | `parse_based(size: Option<u32>, signed: bool, radix: Radix, digits: &str) -> Result<Literal, LiteralError>` | `_` は無視。`?` は Z。桁数が size より多ければ左を切り詰め (警告フラグ)、少なければ左を 0 で埋める。**最左ビットが X/Z ならその値で埋める**。10 進は X/Z を単独の桁としてだけ許す (`8'dx`)。size 0 はエラー | §5.7.1 |
| サイズなしリテラル | 同 (`size = None`) | 幅は 32 ビット以上。32 ビットを超える値は必要幅に広げ、警告フラグを立てる [要確認 U-6]。最左が X/Z のものは `Literal::xz_extend = true` を返し、文脈幅への拡張は sema が行う | §5.7.1 |
| 10 進のサイズなし整数 | `parse_unsized_decimal(digits)` | 符号付き 32 ビット以上 | §5.7.1 |
| 非基数のサイズなし `'0 '1 'x 'z` | `LogicVec::filled(w, v)` | 文脈幅を全部その値で埋める (幅は sema) | §5.7.1 |
```rust
pub enum Radix { Bin, Oct, Dec, Hex }
pub struct Literal { pub value: LogicVec, pub signed: bool, pub sized: bool,
                     pub xz_extend: bool, pub truncated: bool, pub widened_unsized: bool }
```

#### (h) 文字列との変換 (タスク M0-T7)
| 機能 | 関数 | 規則 | 条 |
|---|---|---|---|
| 文字列リテラル → 整数 | `from_string_bytes(dst, bytes: &[u8])` | 1 文字 8 ビット、最後の文字が LSB 側 (右詰め)。dst が広ければ左を 0 埋め、狭ければ左 (先頭側の文字) を切り詰め | §5.9, §11.10, §11.10.2 |
| 整数 → `string` 型 | `to_string_bytes(x) -> Vec<u8>` | 8 ビットずつ MSB 側から文字にする。幅が 8 の倍数でなければ左を 0 埋め。値 0 のバイト (`"\0"`) は捨てる。X/Z を含むバイトは 0 として扱う [要確認 U-7] | §6.16, §6.24.1 |
| `atoi atohex atooct atobin` | `atoi(s: &[u8], radix: Radix) -> u32` (結果は `integer` の 32 ビット) | 先頭の数字と `_` を読み、他の文字か終端で止まる。数字が無ければ 0。32 ビットを超える分は切り詰め。先頭の `-` / `+` を受けるか [要確認 U-7] | §6.16.9 |
| `itoa hextoa octtoa bintoa` | `itoa(x, signed, radix) -> Vec<u8>` | `%0d %0h %0o %0b` と同じ書式 (§3.9) で生成 | §6.16.11〜§6.16.14 |
- `string` 型そのもの (メソッド群、比較、連結) は M1/M3 のランタイムの仕事。M0 は整数値との変換部分だけ。
- `atoreal` / `realtoa` (§6.16.10, §6.16.15) は実数の書式化と一緒に M1。

#### (i) 実数との変換 (タスク M0-T8)
| 機能 | 関数 | 規則 | 条 |
|---|---|---|---|
| 整数 → 実数 (暗黙変換、`$itor`) | `to_f64(x: LogicRef, signed) -> f64`, `to_f32` | X/Z のビットは 0 として扱う。53 ビットを超える値は IEEE 754 の最近接偶数丸め (1 回の丸めで。多倍長から直接) | §6.12.1, §20.5 |
| 実数 → 整数 (暗黙変換、キャスト) | `from_f64_round(dst, v: f64) -> RealToIntStatus` | **最も近い整数に丸め、ちょうど半分は 0 から遠い方** (例 35.5 → 36、-1.5 → -2)。結果を dst の幅で 2 の補数として切り詰め。dst の幅は任意 (64 ビット超も正確に) | §6.12.1 |
| `$rtoi` | `from_f64_trunc(dst, v)` | 0 方向へ切り捨て | §20.5 |
| NaN / ±∞ → 整数 | 同上 (`RealToIntStatus::NotFinite`) | 規格に記述が見当たらない。暫定で 0 を返し状態を返す [要確認 U-7] | — |
| `$realtobits` / `$bitstoreal` | `f64::to_bits` / `from_bits` の包み (64 ビット) | ビットパターンをそのまま | §20.5 |
| `$shortrealtobits` / `$bitstoshortreal` | 同 (32 ビット) | 同 | §20.5 |

#### (j) 表示用フォーマット (タスク M0-T8)
§3.9 で詳述。

### 3.6 符号と幅の規則 (呼び出し側の責務の確認)
ライブラリは幅と符号を決めないが、呼び出し側 (M1 の sema / 定数評価 / MIR 生成) が守るべき規則を、
`svsim-logic` の crate ドキュメントに表で書く (実装は M1)。

- 式のビット長: §11.6.1 の表 (2017 の Table 11-21)。`+ - * / % & | ^ ^~ ~^` は max(L(i), L(j))、比較・等価・リダクション・
  論理演算は 1 ビット (オペランドは max(L(i), L(j)) に拡張)、シフトと `**` は L(i)、連結は和、複製は n×L、`?:` は max(L(j), L(k))。
- 符号: §11.8.1。結果は全オペランドが符号付きのときだけ符号付き。比較の結果、連結、ビット選択、部分選択は常に符号なし。
  自己決定のオペランドは自分の型のまま。
- 評価の手順: §11.8.2 (型と幅を先に決め、オペランドを拡張してから評価)。
- `$signed` / `$unsigned` (§11.7) は値を変えない (ライブラリでは何もしない)。

### 3.7 モジュール構成とファイル (並列実装のための分割)
```
crates/svsim-logic/src/
  lib.rs            モジュール宣言と再エクスポート (M0-T3 が全モジュールを宣言しておく)
  logic.rs          Logic (スカラ)                                   M0-T3
  layout.rs         words_for, top_mask, MAX_WIDTH, 正規化              M0-T3
  view.rs           LogicRef/LogicMut/BitRef/BitMut                  M0-T3
  vec.rs            LogicVec/BitVec (構築、変換、Debug/Display)        M0-T3
  scratch.rs        Scratch                                         M0-T3
  words.rs          2 値のワード列プリミティブ (add/sub/neg/shift/mul_small/divrem_small/cmp/is_zero)  M0-T3
  abi.rs            svLogicVecVal / svBitVecVal との変換               M0-T3
  scalar.rs         L64 と共通補助 (マスク、符号拡張)。各演算のスカラ版は各演算ファイル内に置く  M0-T3
  ops/mod.rs        (宣言のみ)                                        M0-T3
  ops/bitwise.rs    ビット演算                                        M0-T4
  ops/reduce.rs     リダクション、$countbits 系                       M0-T4
  ops/logical.rs    truth, 論理演算                                   M0-T4
  ops/compare.rs    等価・比較・casez/casex                           M0-T4
  ops/cond.rs       cond_merge                                       M0-T4
  ops/arith.rs      算術                                              M0-T5
  ops/shift.rs      シフト                                            M0-T6
  ops/resize.rs     拡張・切り詰め・2 値/4 値変換                      M0-T6
  ops/concat.rs     連結・複製                                        M0-T6
  ops/select.rs     選択・インデックス                                 M0-T6
  literal.rs        リテラル解析                                       M0-T7
  string.rs         文字列変換、atoi 系                               M0-T7
  real.rs           実数変換                                          M0-T8
  fmt.rs            表示用フォーマット                                 M0-T8
crates/svsim-logic/tests/
  common/mod.rs     proptest の戦略、LogicVec ⇔ RefVec 変換、不変条件の検査   M0-T3
  diff_core.rs      M0-T3   diff_bitwise.rs / diff_compare.rs  M0-T4   diff_arith.rs  M0-T5
  diff_struct.rs    M0-T6   diff_literal_string.rs  M0-T7   diff_real_fmt.rs  M0-T8
  golden_*.rs       規格の表・例の固定テスト (各タスク)
  no_alloc.rs       ヒープ確保をしないことの検査 (M0-T3 が枠を作り、各タスクが追記しない。§5.5)
crates/svsim-logic-ref/src/
  lib.rs, value.rs (M0-T3)、bitwise.rs, compare.rs (M0-T4)、arith.rs (M0-T5)、
  structure.rs (M0-T6)、literal.rs, string.rs (M0-T7)、real.rs, fmt.rs (M0-T8)
```
- M0-T3 は、後続タスクが触るファイルを**空のスタブとして全部作り**、`lib.rs` / `ops/mod.rs` のモジュール宣言を済ませる。
  後続タスクは自分の担当ファイルだけを編集すればよい。
- `words.rs` の 2 値ワード列プリミティブ (特に `mul_small_add` と `divrem_small`) は M0-T5 (算術)、M0-T7 (10 進リテラル)、
  M0-T8 (10 進表示) が共有するので M0-T3 で作る。T7/T8 は T5 の多倍長除算に依存しない (10^19 での繰り返し除算で足りる)。

### 3.8 DPI 形式との変換 (タスク M0-T3、`abi.rs`)
```rust
/// LogicRef → svLogicVecVal 配列 (u32 の aval/bval 交互、長さ 2*ceil(w/32))。
pub fn write_sv_logic_vec_val(src: LogicRef<'_>, dst: &mut [u32]);
/// svLogicVecVal 配列 → LogicMut。最上位要素の w 以上のビットは読み捨ててマスクする。
pub fn read_sv_logic_vec_val(src: &[u32], dst: LogicMut<'_>);
pub fn write_sv_bit_vec_val(src: BitRef<'_>, dst: &mut [u32]);
pub fn read_sv_bit_vec_val(src: &[u32], dst: BitMut<'_>);
```
- C 側から戻る値の未使用ビット (最上位要素の `w` 以上) は、C が何を書いても読み込み時にマスクして正規化不変条件を守る。
  (Annex H での未使用ビットの扱いの記述は M3 で再確認する。)
- M3 で 2 値の `svBitVecVal` をアリーナから直接ポインタで渡す場合、`u32` 単位の要素数 `ceil(w/32)` が `u64` の
  ワード数 ×2 以下なので、境界をはみ出さない。

### 3.9 表示用フォーマット (タスク M0-T8、`fmt.rs`)
```rust
pub enum FmtRadix { Bin, Oct, Dec, Hex }
pub enum FieldWidth { Auto, Min(u32) }           // Auto = 既定の自動幅、Min(0) = %0d 等
pub fn format_int(out: &mut String, x: LogicRef<'_>, radix: FmtRadix, signed: bool,
                  width: FieldWidth, s: &mut Scratch);
pub fn format_int2(out: &mut String, x: BitRef<'_>, ...);
```
規則 (§21.2.1.2 書式指定、§21.2.1.3 表示データのサイズ、§21.2.1.4 不定値・ハイインピーダンス値。
マネージャ指示では「§21.2.1.3」とあるが、X/Z の表示規則は 1800-2017 では 21.2.1.4 にある。レビューで照合すること):

1. **`%b %o %h` (`%x` は `%h` と同じ)**: LSB 側から 1/3/4 ビットずつ桁にする (最上位桁は端数)。各桁は
   - 全ビットが X → `x`、全ビットが Z → `z`、
   - 一部が X → `X`、X は無く一部が Z → `Z`、
   - 全ビット既知 → 数字 (16 進は小文字 `a-f`)。
2. **`%d`**: 値全体で判定する。全ビット X → `x`、全ビット Z → `z`、一部 X → `X`、X が無く一部 Z → `Z`。
   全ビット既知なら 10 進 (符号付きで負なら `-` を付ける)。
3. **自動幅 (`Auto`)**: その幅の値が取りうる最大の表示に必要な文字数にする。`%b %o %h` は先頭を 0 で埋める。
   `%d` は先頭を空白で埋める (右詰め)。`%d` の文字数は、符号なしは `2^w - 1` の桁数、符号付きは `-2^(w-1)` の文字数
   (符号を含む)。X/Z の 1 文字表示も同じ幅で右詰めにする。
4. **`%0d` 等 (`Min(0)`)**: 先頭の 0 / 空白を付けない最小の表示。`%0h` で上位桁が X/Z の場合はそこで止まる (X/Z の桁は 0 ではない)。
5. **`Min(n)` (n > 0)**: 埋め文字は自動幅と同じ (`%d` は空白、それ以外は 0) とする。値が n より長い場合は切り詰めない。[要確認 U-7]
6. 2 値の値は X/Z が無いだけで同じ規則。

- 例 (ゴールデンテストに入れる): `%b` of `4'b10xz` → `10xz`、`%h` of `8'bxxxx_01z0` → `xZ`、`%h` of `8'bzzzz_xxxx` → `zx`、
  `%h` of `8'b0x01_0000` → `X0`、`%d` of `8'bx` → `  x` (3 文字)、`%d` of `8'sb1111_1111` → `  -1` (4 文字)、
  `%d` of 32 ビット符号付き `1` → `          1` (11 文字)、`%0d` of `-5` → `-5`。
  自動幅の文字数は Icarus / VCS と M1 以降に突き合わせる。

### 3.10 JIT・アリーナ・並列化・チェックポイントとの関係の確認
| 後続 | 本設計で守っていること |
|---|---|
| アリーナ (M1〜) | 信号の値は §3.2 の配置で `WordArena` (§4.4) の連続ワードに置く。`LogicRef::from_planar` / `LogicMut::from_planar` でビューを作って層 2 のカーネルを in-place で呼ぶ。値型の一時オブジェクトは要らない |
| JIT (M5) | 幅 ≤ 64 は §3.4 の式を Cranelift IR で直接生成 (ヘルパ呼び出しなし)。幅 > 64 は層 2 のカーネルを `extern "C" fn(dst: *mut u64, x: *const u64, y: *const u64, width: u32)` で包んで呼ぶ (M5 で svsim-jit 側に作る。unsafe はそこに閉じる)。カーネルはヒープ確保せず、panic は幅の assert だけ |
| 2 値推論 (M5) | 2 値の値は aval プレーンだけで、4 値の aval プレーンと同じ配置。2 値と証明された信号は bval を持たない配置に切り替えられる |
| チェックポイント (M4) | 状態はワード列だけなので memcpy でスナップショットできる。値の内部にポインタを持たない (`LogicVec` のヒープは一時値だけで、アリーナには入れない) |
| 並列 (M6) | カーネルはグローバル状態を持たない (`Scratch` は呼び出し側持ち)。どのスレッドで実行しても同じ結果 |
| DPI (M3) | §3.8 の変換。スカラ `svLogic` は `Logic as u8` のまま |
| デバッグ (M4) | `fmt.rs` を波形・問い合わせ API の値表示にも使う |

### 3.11 強度 (strength) との関係
強度付き値 (PLAN §4、M2。IEEE 1800 §28.11 以降) は、値 (0/1/X/Z) と 0 側・1 側の強度を持つ別の表現にし、
`svsim-logic` の 4 値表現は強度の解決後の値として使う。M0 では強度を扱わない。強度の型は M2 で
`svsim-logic` に別モジュールとして追加する想定 (現在の型を変更する必要はない)。

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
    Virtual { name: String },                          // コマンドライン define 等
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
- テキストは**バイト列**で持つ (SV ソースが UTF-8 とは限らない。字句解析もバイト単位)。表示時だけ損失のある UTF-8 変換をする。
- 1 ファイルの上限は 4 GiB (`u32` オフセット)。`Span` は 12 バイト。
- マクロ展開の追跡 (PLAN §2) は `FileOrigin::MacroExpansion` の仮想ファイルで表す想定。具体化は M1 で、M0 では列挙子を用意するだけ。

### 4.2 診断 (`diag.rs`, `diag/render_human.rs`, `diag/render_json.rs`、タスク M0-T10)
```rust
pub enum Severity { Error, Warning, Note, Help }
pub struct DiagCode(pub &'static str);           // 例 "E0001"。番号体系の割り当ては M1 で決める
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
pub fn render_human(sm: &SourceMap, d: &Diagnostic, color: bool) -> String;   // rustc 風: 位置、該当行、^^^ の下線、注記
pub fn render_json(sm: &SourceMap, d: &Diagnostic) -> serde_json::Value;      // {"version":1, "severity", "code", "message", "labels":[{"file","line","col","end_line","end_col","message","primary"}], "notes", "fixits"}
```
- JSON は AI 向けの構造化診断 (PLAN §8.4)。`version` フィールドを付けてスキーマを版管理する。
- 表示部は自前実装 (外部の診断表示 crate は使わない。小さいので依存を増やさない)。

### 4.3 文字列インターン (`intern.rs`、タスク M0-T11)
```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Symbol(u32);
pub struct Interner { map: rustc_hash::FxHashMap<Box<str>, Symbol>, strings: Vec<Box<str>> }  // 実装は自由 (重複保持を避ける工夫も可)
impl Interner {
    pub fn intern(&mut self, s: &str) -> Symbol;
    pub fn get(&self, s: &str) -> Option<Symbol>;
    pub fn resolve(&self, sym: Symbol) -> &str;
}
```
- **規則**: `Symbol` の番号順を出力順に使わない (並列化したときに番号が実行ごとに変わってもよいように)。crate ドキュメントに書く。
- 並列パース用のスレッド安全版は M1 以降で必要になった時点で決める (候補: スレッドごとのローカル表 + 後で統合、
  または `lasso` 等の crate)。M0 の API は `&mut self` の単一スレッド版。

### 4.4 アリーナ (`index.rs`, `arena.rs`、タスク M0-T11)
2 種類ある。

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
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct WordOffset(pub u32);                  // ワード (u64) 単位のオフセット
pub struct ArenaLayoutBuilder { next: u32 }
impl ArenaLayoutBuilder {
    pub fn alloc(&mut self, words: u32, align_words: u32) -> WordOffset;   // align_words は 2 のべき (1, 2, 8 …)
    pub fn finish(self) -> ArenaLayout;
}
pub struct ArenaLayout { len_words: u32 }
pub struct WordArena { words: Vec<u64> }          // 0 初期化
impl WordArena {
    pub fn new(layout: &ArenaLayout) -> Self;
    pub fn words(&self) -> &[u64];
    pub fn words_mut(&mut self) -> &mut [u64];
    pub fn slice(&self, off: WordOffset, len: u32) -> &[u64];
    pub fn slice_mut(&mut self, off: WordOffset, len: u32) -> &mut [u64];
    /// 重ならない複数の可変スライス (dst と src を同時に取る用)。重なりや範囲外は Err。
    pub fn disjoint_mut<const N: usize>(&mut self, ranges: [(WordOffset, u32); N]) -> Result<[&mut [u64]; N], ArenaError>;
    pub fn snapshot(&self) -> ArenaSnapshot;          // 全体コピー (memcpy)
    pub fn restore(&mut self, s: &ArenaSnapshot);     // 同じ layout から作ったものに限る (長さ検査)
    /// 決定性テスト用の内容ハッシュ (FNV-1a 64 など、プラットフォーム・実行に依存しない固定アルゴリズム)。
    pub fn content_hash(&self) -> u64;
}
```
- アリーナは「何がどこにあるか」(信号名 → オフセット・幅・2/4 値) を知らない。その対応表は M1 のエラボ/MIR が持つ。
  M0 では、`svsim-common` の結合テスト (`tests/arena_logic.rs`、dev-dependency に `svsim-logic`) で、
  アリーナ上に 4 値 / 2 値の信号を置き、`LogicRef::from_planar` / `LogicMut::from_planar` で in-place 演算し、
  スナップショット → 変更 → 復元で元に戻ることを示す。
- 初期値 0 は 4 値でも「0」を意味する。`logic` の初期値 X (§6.8 の表: 4 値型の既定の初期値は X) は、
  M1 のエラボ側が明示的に `fill(X)` する (アリーナは型を知らないため)。
- キャッシュライン境界 (64 バイト) へのベースアドレス合わせと、ページ単位の差分スナップショットは M4/M6。
  M0 の `align_words` はベースからの相対位置合わせだけ。

---

## 5. 差分テスト戦略 (`svsim-logic`)

### 5.1 参照モデル `svsim-logic-ref`
- `svsim-logic` に**依存しない**独立実装 (同じ誤りを共有しないため)。
- 値は `RefVec(Vec<RefLogic>)` (LSB 先頭、1 要素 1 ビット)。`enum RefLogic { Zero, One, X, Z }`。
- ビット演算・リダクション・等価・ワイルドカード・casez/casex・`?:` のマージ: **規格の真理値表を 4×4 の表としてそのまま書き**、
  ビットごとに適用する (§11.4.8 の各表、§11.4.11 の表)。式 (§3.4) は使わない。
- シフト・連結・選択・拡張: `Vec` の要素の並べ替えとして素朴に書く。
- 算術・比較・べき乗: X/Z があれば全 X。無ければ `num-bigint` の `BigInt` に変換して計算し、`2^w` の剰余で戻す。
  除算・剰余は `BigInt` の `/` `%` (0 方向切り捨て、剰余は被除数の符号) を使う。べき乗は負の指数を §11.4.3 の表で場合分けし、
  非負は `modpow(exp, 2^w)`。
- 10 進表示: `BigInt::to_str_radix`。2/8/16 進表示: ビットを桁ごとに集め、§3.9 の規則を素朴に適用。
- 実数: 整数 → 実数は `BigInt` → `f64` の正しい丸め (自前で 1 回丸め)、実数 → 整数は `f64` を分解して `BigInt` へ正確に変換して丸める。
- リテラル: 文字ごとに桁を `RefVec` に積む素朴実装。

### 5.2 比較の方法
- テストごとに `LogicVec` (層 3) / スライスカーネル (層 2) / スカラ (層 1、幅 ≤ 64 のとき) の 3 つを参照モデルと比べる。
  比較は「全ビットが同一 (aval・bval とも)」。
- 毎回、出力の**正規化不変条件** (§3.2) も検査する。
- `_assign` 版も同じ入力で比べる。

### 5.3 入力の生成 (proptest)
- 幅: 境界値 `{1, 2, 3, 7, 8, 31, 32, 33, 63, 64, 65, 127, 128, 129, 191, 192, 193, 255, 256, 257}` から 40%、
  `1..=200` から 40%、`201..=1024` から 20%。
- 値: ワードごとに「全 0 / 全 1 / 乱数 / 1 ビットだけ 1 / 符号ビットだけ 1 / 符号付き最大・最小」を混ぜ、
  X/Z の密度を `{0, 1 ビットだけ, 1%, 50%, 全部}` から選ぶ (X と Z の比も変える)。算術は X/Z 無しの比率を高くする (意味のある計算が多くなるように)。
- シフト量: `0..=w+2` の小さい値、64 の倍数前後、64 ビット超の巨大値、X/Z を含む値。
- 選択のオフセット: 負、0、範囲の境界、完全に範囲外、`i64` の端。
- 除数: 0、1、-1、2 のべき、乱数。べき乗の指数: 負、0、1、大きい値。
- `PROPTEST_CASES` は既定 256 (CI の通常ジョブ)。定期ジョブ (§6) は 100,000。失敗の再現ファイル (`proptest-regressions/`) はコミットする。

### 5.4 網羅テスト・ゴールデンテスト
- **網羅**: 幅 1〜3 の 4 値の全値 (4^3 = 64 値) の全組合せで、ビット演算・比較・等価・`?:` マージ・ワイルドカード・casez/casex。
  幅 1〜4 の全値 × シフト量 `0..=w+1` と X。幅 1〜6 の 2 値の全組合せで算術 (符号付き/符号なし)。
- **ゴールデン** (規格の表と例を固定値で): §11.4.8 の各表、§11.4.9 のリダクションの例、§11.4.3 のべき乗の表と剰余の例、
  §11.4.10 のシフトの例、§11.4.11 の表、§11.4.12.1 の複製の例、§11.5.1 の範囲外選択、§5.7.1 のリテラルの例、
  §6.12.1 の丸めの例、§3.9 の表示例。テスト名やコメントに条番号を書く。

### 5.5 ヒープ確保の検査
- `tests/no_alloc.rs` で確保回数を数える `GlobalAlloc` を設定し (テスト crate は lib と別なので `unsafe` を
  `// SAFETY:` 付きで許可)、(1) 幅 ≤ 64 の `LogicVec` の全演算、(2) 事前確保済みの `Scratch` を渡した任意幅のスライスカーネル、
  が確保 0 回であることを検査する。M0-T3 が枠と T3 範囲の検査を作り、各タスクはこのファイルに自分の演算を足す
  (このファイルだけは T4〜T8 の並列実装で競合しうるので、各タスクは**別ファイル** `no_alloc_<area>.rs` を作ってよい)。

---

## 6. CI (GitHub Actions、タスク M0-T2 / 追加は M0-T13)
`.github/workflows/ci.yml`:
| ジョブ | トリガ | 内容 |
|---|---|---|
| `fmt` | push / PR | `cargo fmt --all -- --check` |
| `clippy` | push / PR | `cargo clippy --workspace --all-targets -- -D warnings` |
| `test` | push / PR | `cargo build --workspace --locked`、`cargo test --workspace --locked` (debug)。proptest は既定件数 |
| `doc` | push / PR | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` |
| `deny` | push / PR | `cargo deny check licenses bans sources` (許可ライセンス: MIT, Apache-2.0, Unicode-3.0, Unlicense, BSD-3-Clause, Zlib) |
| `corpus-fetch` | push / PR (M0-T13 で追加) | `cargo run -p svsim-corpus -- fetch sv-tests` と `discover sv-tests`。`.corpus-cache` を `actions/cache` でキャッシュ (キーはマニフェストのハッシュ) |
| `proptest-long` | 毎日 (schedule) + 手動 | `PROPTEST_CASES=100000 cargo test -p svsim-logic --release` |
- ツールチェーンは `rust-toolchain.toml` に従う (`rustup show` で導入)。ビルドキャッシュは `Swatinem/rust-cache`。
- サードパーティの action はメジャーバージョンのタグで指定する (SHA 固定にするかは §10 U-5)。
- M0 は Linux (ubuntu-latest) だけ。macOS/Windows は DPI の時期 (M3) に検討。

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
submodules = []                                   # M0 は取得しない (third_party のコア/UVM は後で)
sparse = []                                       # 空 = 全体。必要なら ["tests/", "LICENSE"]
[entry.discover]
kind = "sv-tests"                                 # アダプタ: sv-tests のメタデータを解釈
root = "tests"
```
- 他のアダプタ: `kind = "glob"` (`include = ["rtl/**/*.sv"]`, `exclude`, `top`, `incdirs`, `defines` を指定する一般形)。
  M0 は `sv-tests` と `glob` の 2 種。
- ソースは `git` のほか `path = "/abs/dir"` (ローカルの非公開コード用、取得しない)。
- マニフェストの読み込み元: `corpus/manifest.toml` + `corpus/private/*.toml` (gitignore) + `--manifest <path>` (複数可)。
  エントリ名の重複はエラー。
- `rev` はコミット ID だけ許す (ブランチ名やタグは不可: 再現性のため)。

### 7.2 取得とキャッシュ
- キャッシュ場所: `$SVSIM_CORPUS_CACHE`、未設定ならリポジトリ直下の `.corpus-cache/` (gitignore)。
  配置は `<cache>/<name>/<rev>/`。
- 取得手順 (外部コマンド `git` を使う。libgit2 等の依存は入れない):
  `git init` → `git fetch --depth 1 <url> <rev>` → `git checkout --detach FETCH_HEAD` →
  `git rev-parse HEAD` が `rev` と一致することを検証 → (sparse / submodules の指定があれば適用) →
  一時ディレクトリ (`<cache>/<name>/.tmp-<rev>-<pid>`) から最終位置へ `rename` (原子的に完了させる) → 完了印 `.svsim-complete` を置く。
- 完了印があれば何もしない (冪等)。`--offline` ではネットワークに出ず、キャッシュに無ければエラー。
- 同時実行は `<cache>/<name>/.lock` (`create_new` での排他) で防ぐ。
- **安全**: 取得したファイルは信頼しない入力として扱う。コーパスマネージャはファイルを読むだけで、
  取得したリポジトリ内のスクリプト (sv-tests の Python の生成器など) は実行しない。git のフックは新規 init なので存在しない。
- sv-tests の生成テスト (`conf/generators`、サブモジュールのコアや UVM を使うもの) は M0 の対象外。

### 7.3 テストケースの列挙
```rust
pub struct TestCase {
    pub id: String,                 // "<entry>/<root からの相対パス>"
    pub files: Vec<PathBuf>, pub incdirs: Vec<PathBuf>, pub defines: Vec<String>,
    pub top: Option<String>,
    pub expect: Expectation,
    pub meta: BTreeMap<String, String>,   // アダプタ固有 (sv-tests の :tags: 等をそのまま)
}
pub struct Expectation { pub should_fail: bool, pub reason: Option<String>, pub target: Stage }
```
- **sv-tests アダプタ** (sv-tests 本体の `tools/runner` の解釈に合わせる。2026-11-27 のコミットで確認):
  - 対象: `tests/` 以下の `*.sv` (大文字小文字を区別しない。sv-tests の Makefile と同じ `find -iname '*.sv'`)。
    固定コミットで **1028 件**。
  - 各行を `^:([a-zA-Z_-]+):\s*(.+)` で照合してメタデータを得る。対応するキー: `name tags description files incdirs
    top_module timeout type should_fail should_fail_because defines compatible-runners unsynthesizable results_group simvariants`、
    および `runner_<tool>_flags`。それ以外は警告して無視。
  - 既定値: `files` = そのファイル、`incdirs` = そのファイルのディレクトリ、`type` = `"parsing elaboration"`、
    `should_fail` = `should_fail_because` があれば 1。`files` / `incdirs` / `defines` は空白区切り。
  - `:type:` の語 (`preprocessing parsing elaboration simulation`) のうち最も後の段階を `Expectation::target` にする。
  - `tags` に UVM 等のライブラリ名がある場合の追加ファイル (sv-tests の `libs` 設定) は M0 では扱わず、`meta` に残すだけ。
- 列挙結果は `id` のバイト順で並べる (決定性)。

### 7.4 段階評価の枠組み
```rust
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize)]
pub enum Stage { Preprocess, Parse, Elaborate, Simulate, Compare }
pub enum StageOutcome { Pass, Fail { category: String, message: String }, NotImplemented, Skipped { reason: String }, Timeout }
pub trait StageRunner: Send + Sync {
    fn stage(&self) -> Stage;
    fn run(&self, case: &TestCase, env: &RunEnv) -> StageOutcome;
}
pub struct Pipeline { runners: Vec<Box<dyn StageRunner>> }   // M1 で Preprocess / Parse の runner を登録する
```
- ケースごとに `Preprocess` から `target` まで順に実行し、`Pass` 以外で止める。
- 期待失敗 (`should_fail`) のケースは、生の結果と「期待どおりか」の判定を両方記録する
  (判定規則は「`target` までのどこかで `Fail` なら期待どおり」。詳細は M1 で runner が入ったときに見直す)。
- M0 は runner が 1 つも無いので、全ケースが `Preprocess: NotImplemented` になる。
- 出力: `<out>/results.jsonl` (1 行 1 ケース: id、段階ごとの結果、所要時間)、`<out>/summary.json` と `summary.md`
  (段階 × 結果の件数表、失敗の `category` 別件数)。既定の `<out>` は `target/corpus-results/<entry>/`。
  所要時間以外は決定的 (比較・差分表示に使える) にする。
- ケースの並列実行は `--jobs N` (標準ライブラリのスレッドで十分。結果は id 順に並べてから書く)。

### 7.5 CLI
```
svsim-corpus list                          マニフェストのエントリ一覧
svsim-corpus fetch [NAME...] [--offline]   取得 (名前省略で全部)
svsim-corpus discover NAME [--json]        ケースを列挙して件数とメタデータの統計を表示
svsim-corpus run NAME [--jobs N] [--out DIR] [--filter GLOB]
```
- `cargo run -p svsim-corpus -- <subcmd>` で動かす。`.cargo/config.toml` に `corpus = "run -p svsim-corpus --"` の
  エイリアスを置く (`cargo corpus fetch sv-tests`)。
- 終了コード: 0 = 成功、1 = 実行時エラー (取得失敗等)、2 = 使い方の誤り。`run` はケースの失敗では非 0 にしない (計測用)。

---

## 8. タスク分解

### 8.1 一覧
| ID | 内容 | 変更範囲 | 依存 | 並列 | 規模 |
|---|---|---|---|---|---|
| **M0-T1** | workspace の骨組み: ルート `Cargo.toml` (workspace 設定・lint・`[workspace.dependencies]` に M0 で使う全依存)、`rust-toolchain.toml`、`rustfmt.toml`、`.gitignore`、`.cargo/config.toml`、4 crate の `Cargo.toml` と空の `lib.rs`/`main.rs`、`svsim-common` の全モジュールの空スタブ (`source.rs diag.rs diag/*.rs intern.rs index.rs arena.rs`) と `lib.rs` の宣言、`Cargo.lock` の確定 | ルート直下の設定ファイル、`crates/*/Cargo.toml`、`crates/svsim-common/src/**` (スタブのみ)、`crates/svsim-{logic,logic-ref,corpus}/src/{lib,main}.rs` (空) | なし | — (最初) | S |
| **M0-T2** | CI: `.github/workflows/ci.yml` (§6 の `corpus-fetch` 以外)、`deny.toml` | `.github/`、`deny.toml` | T1 | T3, T10, T12 と並列可 | S |
| **M0-T3** | logic の中核: §3.1〜3.3 の型 (`Logic`, layout, ビュー, `LogicVec`/`BitVec`, `Scratch`)、`words.rs`、`abi.rs` (§3.8)、`scalar.rs` の共通部、**後続タスクの全ファイルのスタブとモジュール宣言** (§3.7)。参照モデル crate の中核 (`RefLogic`, `RefVec`, 文字列 ⇔ 値)。`tests/common/mod.rs` (戦略・変換・不変条件検査)、`diff_core.rs`、`no_alloc.rs` の枠 | `crates/svsim-logic/**`、`crates/svsim-logic-ref/**` | T1 | T2, T10, T12 と並列可 | M |
| **M0-T4** | ビット演算・リダクション・`$countbits` 系・論理演算・等価/比較/ワイルドカード/casez/casex・`cond_merge` (§3.5 (a)(b)(f))。スカラ版・スライス版・`_assign` 版・`LogicVec` メソッド・2 値版。参照モデルの該当部。差分・網羅・ゴールデンテスト | `svsim-logic/src/ops/{bitwise,reduce,logical,compare,cond}.rs`、`svsim-logic-ref/src/{bitwise,compare}.rs`、`svsim-logic/tests/{diff_bitwise,diff_compare,golden_bitwise,no_alloc_bitwise}.rs` | T3 | T5〜T8, T11, T13 と並列可 (**worktree 分離**) | M |
| **M0-T5** | 算術 (§3.5 (c)): add/sub/neg/inc/dec/mul/div/rem/pow、符号付き/符号なし、Knuth D。参照モデル (num-bigint)。テスト | `svsim-logic/src/ops/arith.rs`、`svsim-logic-ref/src/arith.rs`、`svsim-logic/tests/{diff_arith,golden_arith,no_alloc_arith}.rs` | T3 | 同上 | L |
| **M0-T6** | シフト・拡張/切り詰め・2/4 値変換・連結・複製・選択・インデックス (§3.5 (d)(e)) | `svsim-logic/src/ops/{shift,resize,concat,select}.rs`、`svsim-logic-ref/src/structure.rs`、`svsim-logic/tests/{diff_struct,golden_struct,no_alloc_struct}.rs` | T3 | 同上 | M |
| **M0-T7** | リテラル解析・文字列変換・atoi/itoa 系 (§3.5 (g)(h))。itoa 系は `fmt.rs` に依存するので、M0-T7 では atoi 系と文字列⇔整数だけ実装し、itoa 系は M0-T8 が担当する | `svsim-logic/src/{literal,string}.rs`、`svsim-logic-ref/src/{literal,string}.rs`、`svsim-logic/tests/{diff_literal_string,golden_literal}.rs` | T3 | 同上 | M |
| **M0-T8** | 実数変換 (§3.5 (i))、表示用フォーマット (§3.9)、itoa/hextoa/octtoa/bintoa (`string.rs` ではなく `fmt.rs` に `itoa` を置く) | `svsim-logic/src/{real,fmt}.rs`、`svsim-logic-ref/src/{real,fmt}.rs`、`svsim-logic/tests/{diff_real_fmt,golden_fmt}.rs` | T3 | 同上 | L |
| **M0-T10** | ソースマップ・診断・人間向け/JSON 表示 (§4.1, §4.2)。表示のゴールデンテスト | `svsim-common/src/{source,diag}.rs`、`svsim-common/src/diag/**`、`svsim-common/tests/diag_*.rs` | T1 | T2, T3, T12 と並列可 | M |
| **M0-T11** | インターン、`IndexVec`/`define_index!`、`WordArena` (§4.3, §4.4)。アリーナとロジック値ビューの結合テスト | `svsim-common/src/{intern,index,arena}.rs`、`svsim-common/tests/{arena,arena_logic,intern}.rs`、`svsim-common/Cargo.toml` の dev-dependency 行 (T1 で入れておけば不要) | T1, T3 | T4〜T8, T13 と並列可。T10 と同じ crate なので T10 と同時なら **worktree 分離** | M |
| **M0-T12** | コーパス: マニフェスト解析、取得・キャッシュ・ロック・検証、CLI の `list` / `fetch`、`corpus/manifest.toml` (sv-tests)、単体テスト (ローカルの bare リポジトリを `file://` で取得するテスト) | `crates/svsim-corpus/src/{lib,main,manifest,fetch,cache}.rs`、`crates/svsim-corpus/tests/**`、`corpus/manifest.toml` | T1 | T2, T3, T10 と並列可 | M |
| **M0-T13** | コーパス: 列挙 (sv-tests / glob アダプタ)、段階評価の枠組み、結果出力、CLI の `discover` / `run`、CI に `corpus-fetch` ジョブ追加 | `crates/svsim-corpus/src/{discover,discover/*,stage,report}.rs`、`main.rs` のサブコマンド追加、`crates/svsim-corpus/tests/**` (新規ファイル)、`.github/workflows/ci.yml` (ジョブ追加のみ) | T12, T2 | T4〜T8, T11 と並列可 | M |

(T9 は欠番。)

### 8.2 実行の順序 (波)
```
波 1:  T1
波 2:  T2   T3   T10   T12                          (変更範囲が重ならない)
波 3:  T4   T5   T6   T7   T8   T11   T13           (T4〜T8 は同じ crate なので各自 worktree で並列、マージは機械的)
```
- **同じ crate 内の並列タスク (T4〜T8、T10 と T11) は `isolation: worktree` で実行する**。同じ作業ツリーで並列にすると、
  他タスクの書きかけのファイルでビルドが壊れ、互いのテストが通らなくなるため。担当ファイルが重ならないので、
  マネージャによるマージは衝突しない。`lib.rs` / `ops/mod.rs` は T3 で宣言済みなので触らない。
- どのタスクも依存 crate を追加しない (§2.2)。必要になったら報告する。
- 各タスクの完了条件: `cargo build --workspace`、`cargo test --workspace`、`cargo fmt --all -- --check`、
  `cargo clippy --workspace --all-targets -- -D warnings` が通る (CLAUDE.md / implementer.md)。
  加えて T4〜T8 は、担当演算のすべてについて §5 の差分テスト (3 層 × 参照モデル)、網羅テスト、ゴールデンテストがあること。

### 8.3 各タスクの補足
- **T3**: `LogicVec` のヒープ版とインライン版の切り替えは幅だけで決まる (幅 ≤ 64 → インライン)。同じ幅なら常に同じ表現にし、
  `PartialEq`/`Hash` が表現の違いに左右されないようにする。`words.rs` は 2 値の公開 API ではなく `pub(crate)`
  (ただし T5/T7/T8 が使う)。`tests/common/mod.rs` は T4〜T8 が**読むだけ**にする (追加の戦略は各テストファイル内に書く)。
- **T5**: 4 値の算術は「未知があれば全 X」の判定を先にし、2 値カーネルに委ねる。`div` の符号付き処理で、
  最小値の絶対値が `w` ビットに収まらない点に注意 (`w+1` ビットの作業領域か、符号なしで扱う)。
- **T6**: `get_slice` の `offset` は `i64` で、`offset + width` のオーバフローに注意。
- **T8**: 10 進表示は `words::divrem_small` で 10^19 ずつ割る。自動幅の文字数は幅だけから計算できる (`-2^(w-1)` と `2^w - 1` の桁数)
  ので、表で持つか `BigInt` を使わずに計算する (ライブラリ本体は依存ゼロ)。
- **T12**: 取得のテストはネットワークに出ない (一時ディレクトリに `git init --bare` したリポジトリを `file://` で使う)。
  実際の sv-tests の取得は CI の `corpus-fetch` ジョブと受け入れ確認で行う。
- **T13**: sv-tests アダプタの単体テストは、メタデータ付きの小さな `.sv` を一時ディレクトリに作って行う
  (sv-tests のファイルをリポジトリに取り込まない: CLAUDE.md §3)。

---

## 9. 受け入れ基準 (M0 完了の判定)
テスト計画エージェントはここから test-plan.md を作る。

### 9.1 共通
- A-1 `cargo build --workspace --locked`、`cargo test --workspace --locked`、`cargo fmt --all -- --check`、
  `cargo clippy --workspace --all-targets -- -D warnings`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`、
  `cargo deny check licenses bans sources` が rustc 1.97.0 で通る。
- A-2 GitHub Actions の CI (§6 の push/PR ジョブ全部) が緑。
- A-3 全 crate が `#![forbid(unsafe_code)]` (テスト用の確保回数カウンタを除く)。

### 9.2 `svsim-logic`
- L-1 §3.5 の表のすべての演算 (4 値版・2 値版) が実装され、層 1 (幅 ≤ 64)・層 2・層 3 のそれぞれが参照モデルとの
  proptest (既定 256 件/性質) で一致する。`_assign` 版も同様。
- L-2 `PROPTEST_CASES=100000` で `svsim-logic` のテストが通る (受け入れ時に 1 回、以後は定期ジョブ)。
- L-3 §5.4 の網羅テストとゴールデンテスト (条番号付き) が全部通る。
- L-4 全演算の出力が正規化不変条件 (§3.2) を満たす (差分テストの中で検査)。
- L-5 幅の境界 (1, 63, 64, 65, 127, 128, 129) と幅 > 1000 を含むテストがある。幅 0 の連結・複製が動く。
- L-6 幅 ≤ 64 の `LogicVec` の演算と、`Scratch` 事前確保済みのスライスカーネルがヒープ確保をしない (§5.5)。
- L-7 `abi.rs` の変換が往復で元に戻り、`svLogicVecVal` の配置 (u32 の aval/bval 交互) と一致する (手で作った期待値と比較)。
- L-8 公開 API に doc コメントがあり、crate ドキュメントに §3.2 の配置の契約と §3.6 の幅・符号の規則の表がある。

### 9.3 `svsim-common`
- C-1 ソースマップの行・列計算 (CRLF、最終行に改行なし、空ファイル、不正な UTF-8) のテスト。
- C-2 診断の人間向け表示と JSON のゴールデンテスト (主ラベル + 副ラベル + 注記 + fix-it、複数行にまたがる span)。
- C-3 `DiagSink::into_sorted` の順序が発行順の入れ替えに依存しない。
- C-4 インターン: 同じ文字列は同じ `Symbol`、`resolve` で戻る。
- C-5 `WordArena`: 位置合わせ付き確保、`disjoint_mut` の重なり検出、スナップショット → 変更 → 復元で `content_hash` が元に戻る。
  アリーナ上の 4 値 / 2 値信号に `svsim-logic` のカーネルを in-place で適用できる (結合テスト)。

### 9.4 コーパスマネージャ
- K-1 `cargo corpus fetch sv-tests` が固定コミット `7274242b…` を `.corpus-cache/sv-tests/<rev>/` に取得し、
  `git rev-parse HEAD` の検証に通る。2 回目はネットワークに出ずに即終了する (`--offline` でも成功)。
- K-2 `cargo corpus discover sv-tests` が **1028 件**を列挙し、メタデータの解析エラーが 0 件。`should_fail_because` を持つケース (79 件)、
  `:type:` の分布が sv-tests の実ファイルと一致する (テスト計画エージェントが取得済みツリーから期待値を作る)。
- K-3 `cargo corpus run sv-tests` が `results.jsonl` と `summary.md` を出し、全ケースが `Preprocess: NotImplemented`。
  2 回実行して所要時間以外が同一 (決定性)。
- K-4 マニフェストの誤り (rev がコミット ID でない、名前の重複、未知のキー) がエラーとして報告される。
- K-5 CI の `corpus-fetch` ジョブが緑。

---

## 10. リスクと未決事項

### 10.1 要ユーザ確認
| ID | 事項 | 計画での暫定 |
|---|---|---|
| **U-1** | **4 値の内部配置** (§3.2 D-1)。PLAN §3/§6.1 は `svLogicVecVal` と「メモリ表現が一致」「引数の変換なし」と書いているが、`svLogicVecVal` は 32 ビット単位で aval/bval が交互に並ぶので、64 ビットのワード並列演算と両立しない | 内部は 64 ビットのプレーン分離、DPI 境界で変換 (2 値はゼロコピー)。承認されれば PLAN.md §3 と §6.1 の記述を「エンコーディングが一致、配置は境界で変換」に直す |
| **U-2** | M0 完了条件が PLAN §11 (「sv-tests のパース段階を計測開始」) とマネージャ指示 (「取得できる状態」) で違う | マネージャ指示に従う。PLAN.md §11 の M0 行の更新が必要 |
| **U-3** | ツールチェーンを `1.97.0` に完全固定するか、`stable` 追従にするか | `1.97.0` 固定 (再現性)。更新は明示的に行う |
| **U-4** | GitHub Actions を使えるか (リポジトリの公開先、Actions の利用可否、CI から GitHub への取得) | 使える前提。使えない場合は同じコマンドを `cargo xtask ci` 相当のスクリプトにして手元で流す |
| **U-5** | CI のサードパーティ action の指定をコミット SHA 固定にするか (サプライチェーン対策) | メジャータグで指定。SHA 固定にするなら T2 で対応 |
| **U-6** | 処理系依存の上限: `MAX_WIDTH` (2^24 ビット)、32 ビットを超えるサイズなしリテラルの扱い (広げて警告) | 括弧内の暫定。VCS の挙動に合わせたい場合は VCS での確認が必要 |
| **U-7** | 規格の記述が見つからない・処理系依存の挙動 (下記 10.2) を **VCS の実機で確認できるか**。確認用の小さな SV テスト集を M1 で用意し、業務環境の VCS で流してもらう形を提案 | 暫定の挙動で実装し、テストに「[要VCS確認]」の印を付ける |

### 10.2 規格で決まっていない / 確認できなかった挙動 (暫定の挙動)
| 挙動 | 暫定 |
|---|---|
| 符号付き除算の最小値 / -1 | 2 の補数で折り返し (= 最小値) |
| 2 値の除算・剰余の 0 除算 | 0 (X → 0 の変換と同じ考え方) |
| `>>>` で符号ビットが Z | Z で埋める |
| 実数 NaN/±∞ → 整数 | 0 と状態を返す |
| 整数 → `string` で X/Z を含むバイト | 0 として扱い、0 のバイトは捨てる |
| `atoi` 系の先頭の符号 | 先頭の `-` を受ける (多くの処理系の挙動。規格本文で要確認) |
| `%5d` などの明示幅の埋め文字 (`%h` 等で 0 埋めか空白か) | 自動幅と同じ埋め文字 |
| `%d` の自動幅の文字数 (符号付きの符号の分) | §3.9 の規則 |
| svLogicVecVal の未使用ビット (Annex H) | 読み込み時にマスク |

### 10.3 その他のリスク
- **条番号の照合**: 計画作成時に規格本文を参照できなかった。計画レビューで、特に §11.4.3 (べき乗の表)、§11.4.11 の表、
  §21.2.1.3/§21.2.1.4、§6.11.2、§6.12.1、§6.16 (整数 → 文字列)、§5.7.1 の記述を照合すること。
- **参照モデルが同じ誤解を共有する危険**: 参照モデルは本体と独立に書くが、規格の読み違いは両方に入る。
  ゴールデンテスト (規格の例) と、M1 以降の Icarus/VCS との出力比較で補う。
- **多倍長の除算と表示の性能**: Knuth D と 10^19 での繰り返し除算は O(n^2) で、M0 の用途 (幅 ≤ 数千) では十分。
  巨大幅の `%d` 表示が遅いことが分かったら分割統治に替える (後で)。
- **sv-tests の固定コミット**: 上流でファイル構成が変わると K-2 の件数が変わる。固定コミットを更新するときは期待値も更新する。
