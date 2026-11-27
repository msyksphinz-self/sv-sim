//! テストケースの列挙 (plan.md §7.3、タスク M0-T13)。
//!
//! M0 のアダプタは `sv-tests` だけ ([`DiscoverKind::SvTests`])。
//!
//! # sv-tests アダプタ
//!
//! sv-tests 本体の `tools/runner` (固定コミット 7274242b) の解釈に合わせる:
//!
//! - 対象は `<root>/` 以下の `*.sv` (大文字小文字を区別しない。Makefile の `find -iname '*.sv'` と同じ)。
//!   ライブラリ用の `tests/testbenches/*.sv` なども数える。シンボリックリンクはたどらず、警告して飛ばす
//!   (取得したファイルは信頼しない入力なので、リポジトリの外を読まないため)。
//! - メタデータは、ファイルの全行を `^:([a-zA-Z_-]+):\s*(.+)` で照合して取る。
//!   行の区切りは `\n`、`\r\n`、`\r` (Python の universal newlines と同じ)。キー名は小文字にする。
//!   値は前後の空白を除く (固定コミットには CRLF のファイルが 8 件ある)。同じキーは後のものが勝つ。
//! - 対応するキーは [`SV_TESTS_KEYS`] と `runner_<tool>_flags`。それ以外は警告して無視する。
//! - 既定値: `files` = そのファイル、`incdirs` = そのファイルのディレクトリ、`type` = `parsing elaboration`、
//!   `should_fail` = `should_fail_because` があれば 1。`files` / `incdirs` / `defines` は空白区切りで、
//!   明示された `files` と `incdirs` は `<root>/` からの相対パスとして解釈する。
//! - 期待する段階 ([`Expectation::target`]) は `:type:` の語のうち最も後の段階。
//! - `tags` によるライブラリの追加ファイル (sv-tests の `libs.json`) は M0 では扱わず、`meta` に残すだけ。
//!
//! sv-tests の runner は、対応する全キーが揃った時点で読むのをやめるが、ここでは常に全行を読む
//! (固定コミットでは全キーを持つファイルは無いので、結果は変わらない)。
//!
//! # パス
//!
//! [`TestCase`] のパス (`files`、`incdirs`) は、チェックアウトのディレクトリからの相対パス
//! (例: `tests/a.sv`)。キャッシュの場所に依らない結果にするため。実行時は
//! [`crate::stage::RunEnv::checkout_dir`] と結合する。

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::Error;
use crate::manifest::{DiscoverKind, Entry};
use crate::stage::Stage;

/// 列挙したテストケース 1 件。
#[derive(Clone, PartialEq, Eq, Debug, Serialize)]
pub struct TestCase {
    /// `<エントリ名>/<root からの相対パス>` (区切りは `/`)。
    pub id: String,
    /// ソースファイル (チェックアウトからの相対パス)。
    pub files: Vec<PathBuf>,
    /// インクルードディレクトリ (チェックアウトからの相対パス)。
    pub incdirs: Vec<PathBuf>,
    /// マクロの定義 (`NAME` または `NAME=VALUE`)。
    pub defines: Vec<String>,
    /// トップモジュール。
    pub top: Option<String>,
    /// 期待する結果。
    pub expect: Expectation,
    /// アダプタ固有の情報 (sv-tests のメタデータのキーと値をそのまま入れる)。
    pub meta: BTreeMap<String, String>,
}

/// 期待する結果。
#[derive(Clone, PartialEq, Eq, Debug, Serialize)]
pub struct Expectation {
    /// 失敗するのが正しいケースか。
    pub should_fail: bool,
    /// 失敗するべき理由 (sv-tests の `should_fail_because`)。
    pub reason: Option<String>,
    /// どの段階まで評価するか。
    pub target: Stage,
}

/// 列挙中の警告やエラー 1 件。
#[derive(Clone, PartialEq, Eq, Debug, Serialize)]
pub struct Diagnostic {
    /// 対象のファイル (チェックアウトからの相対パス)。
    pub path: PathBuf,
    /// 内容。
    pub message: String,
}

/// 列挙の結果。
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Discovery {
    /// ケース。`id` のバイト順に並べてある (決定性のため)。
    pub cases: Vec<TestCase>,
    /// 警告 (未知のメタデータのキーなど。ケースには含めている)。
    pub warnings: Vec<Diagnostic>,
    /// エラー (読めない、UTF-8 でない等。ケースには含めていない)。
    pub errors: Vec<Diagnostic>,
}

/// 列挙の統計 (`discover` の表示用)。
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize)]
pub struct DiscoverStats {
    /// ケース数。
    pub total: usize,
    /// `should_fail` のケース数。
    pub should_fail: usize,
    /// `target` ごとの件数 (全段階を 0 も含めて出す)。
    pub by_target: BTreeMap<Stage, usize>,
    /// `:type:` の生の値ごとの件数。指定なしは [`NO_TYPE`]。
    pub by_type: BTreeMap<String, usize>,
    /// メタデータのキーごとの、そのキーを持つケースの数。
    pub keys: BTreeMap<String, usize>,
    /// 警告の数。
    pub warnings: usize,
    /// エラー (解析できなかったファイル) の数。
    pub errors: usize,
}

/// [`DiscoverStats::by_type`] で `:type:` が無いケースのキー。
pub const NO_TYPE: &str = "(none)";

/// sv-tests の runner が対応するメタデータのキー (`runner_<tool>_flags` を除く)。
pub const SV_TESTS_KEYS: [&str; 15] = [
    "name",
    "tags",
    "description",
    "files",
    "incdirs",
    "top_module",
    "timeout",
    "type",
    "should_fail",
    "should_fail_because",
    "defines",
    "compatible-runners",
    "unsynthesizable",
    "results_group",
    "simvariants",
];

/// 読み込むファイルの大きさの上限 (取得したファイルは信頼しないため)。
const MAX_FILE_SIZE: u64 = 64 << 20;

impl Discovery {
    /// 統計を計算する。
    pub fn stats(&self) -> DiscoverStats {
        let mut s = DiscoverStats {
            total: self.cases.len(),
            warnings: self.warnings.len(),
            errors: self.errors.len(),
            ..DiscoverStats::default()
        };
        for st in Stage::ALL {
            s.by_target.insert(st, 0);
        }
        for c in &self.cases {
            if c.expect.should_fail {
                s.should_fail += 1;
            }
            *s.by_target.entry(c.expect.target).or_default() += 1;
            let ty = c.meta.get("type").map_or(NO_TYPE, String::as_str);
            *s.by_type.entry(ty.to_string()).or_default() += 1;
            for k in c.meta.keys() {
                *s.keys.entry(k.clone()).or_default() += 1;
            }
        }
        s
    }
}

/// エントリのケースを、チェックアウト `checkout_dir` から列挙する。
pub fn discover_entry(entry: &Entry, checkout_dir: &Path) -> Result<Discovery, Error> {
    match entry.discover.kind {
        DiscoverKind::SvTests => discover_sv_tests(&entry.name, checkout_dir, &entry.discover.root),
    }
}

/// sv-tests アダプタ。`root` はチェックアウトからの相対パス (マニフェストの `discover.root`)。
pub fn discover_sv_tests(
    entry_name: &str,
    checkout_dir: &Path,
    root: &Path,
) -> Result<Discovery, Error> {
    let root_abs = checkout_dir.join(root);
    // root がシンボリックリンク等でチェックアウトの外を指していないこと
    let canon_checkout = fs::canonicalize(checkout_dir).map_err(|e| Error::io(checkout_dir, e))?;
    let canon_root = fs::canonicalize(&root_abs).map_err(|e| Error::io(&root_abs, e))?;
    if !canon_root.starts_with(&canon_checkout) || !canon_root.is_dir() {
        return Err(Error::Other(format!(
            "discover.root `{}` is not a directory inside {}",
            root.display(),
            checkout_dir.display()
        )));
    }

    let mut d = Discovery::default();
    let walker = walkdir::WalkDir::new(&root_abs)
        .follow_links(false)
        .sort_by_file_name();
    for ent in walker {
        let ent = match ent {
            Ok(e) => e,
            Err(e) => {
                let p = e
                    .path()
                    .and_then(|p| p.strip_prefix(checkout_dir).ok())
                    .map_or_else(|| root.to_path_buf(), Path::to_path_buf);
                d.errors.push(Diagnostic {
                    path: p,
                    message: format!("cannot read directory entry: {e}"),
                });
                continue;
            }
        };
        if !has_sv_extension(ent.file_name().as_encoded_bytes()) {
            continue;
        }
        let rel_root = ent
            .path()
            .strip_prefix(&root_abs)
            .expect("walkdir yields paths under its root");
        let rel_checkout = root.join(rel_root);
        let ft = ent.file_type();
        if ft.is_dir() {
            continue;
        }
        if !ft.is_file() {
            d.warnings.push(Diagnostic {
                path: rel_checkout,
                message: "not a regular file (symbolic links are not followed); skipped".into(),
            });
            continue;
        }
        let Some(rel_str) = utf8_rel(rel_root) else {
            d.errors.push(Diagnostic {
                path: rel_checkout,
                message: "file name is not valid UTF-8".into(),
            });
            continue;
        };
        let text = match read_text(ent.path()) {
            Ok(t) => t,
            Err(msg) => {
                d.errors.push(Diagnostic {
                    path: rel_checkout,
                    message: msg,
                });
                continue;
            }
        };
        let id = format!("{entry_name}/{rel_str}");
        let (case, warns) = sv_tests_case(id, root, rel_root, &text);
        d.warnings
            .extend(warns.into_iter().map(|message| Diagnostic {
                path: rel_checkout.clone(),
                message,
            }));
        d.cases.push(case);
    }
    d.cases.sort_by(|a, b| a.id.as_bytes().cmp(b.id.as_bytes()));
    Ok(d)
}

/// ファイル名が `*.sv` (大文字小文字を区別しない) か。
fn has_sv_extension(name: &[u8]) -> bool {
    name.len() >= 3 && name[name.len() - 3..].eq_ignore_ascii_case(b".sv")
}

/// `/` 区切りの相対パス。UTF-8 でない部分があれば `None`。
fn utf8_rel(p: &Path) -> Option<String> {
    let mut parts = Vec::new();
    for c in p.components() {
        match c {
            Component::Normal(s) => parts.push(s.to_str()?),
            _ => return None,
        }
    }
    Some(parts.join("/"))
}

fn read_text(p: &Path) -> Result<String, String> {
    let f = fs::File::open(p).map_err(|e| format!("cannot open: {e}"))?;
    let mut buf = Vec::new();
    f.take(MAX_FILE_SIZE + 1)
        .read_to_end(&mut buf)
        .map_err(|e| format!("cannot read: {e}"))?;
    if buf.len() as u64 > MAX_FILE_SIZE {
        return Err(format!("file is larger than {MAX_FILE_SIZE} bytes"));
    }
    String::from_utf8(buf).map_err(|e| format!("not valid UTF-8: {e}"))
}

/// sv-tests のメタデータを取り出す。`(小文字のキー, 前後の空白を除いた値)` を出現順に返す。
///
/// 各行を `^:([a-zA-Z_-]+):\s*(.+)` で照合する (sv-tests `tools/runner`)。
/// `(.+)` は 1 文字以上を要するので、`:tags:` の直後で行が終わる場合は一致しない。
pub fn parse_sv_tests_metadata(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.split(['\n', '\r']) {
        let Some(rest) = line.strip_prefix(':') else {
            continue;
        };
        let key_len = rest
            .bytes()
            .take_while(|b| b.is_ascii_alphabetic() || matches!(b, b'_' | b'-'))
            .count();
        if key_len == 0 {
            continue;
        }
        let (key, after) = rest.split_at(key_len);
        let Some(value) = after.strip_prefix(':') else {
            continue;
        };
        if value.is_empty() {
            continue;
        }
        out.push((key.to_ascii_lowercase(), value.trim().to_string()));
    }
    out
}

/// sv-tests の runner が受け付けるキーか (`runner_.*_flags$` を含む)。
pub fn is_sv_tests_key(key: &str) -> bool {
    SV_TESTS_KEYS.contains(&key)
        || key
            .strip_prefix("runner_")
            .is_some_and(|r| r.ends_with("_flags"))
}

/// `:type:` の語に対応する段階。
fn type_word_stage(word: &str) -> Option<Stage> {
    match word {
        "preprocessing" => Some(Stage::Preprocess),
        "parsing" => Some(Stage::Parse),
        "elaboration" => Some(Stage::Elaborate),
        "simulation" => Some(Stage::Simulate),
        _ => None,
    }
}

/// sv-tests の既定の `:type:`。
pub const SV_TESTS_DEFAULT_TYPE: &str = "parsing elaboration";

/// 1 ファイル分のケースを作る。`root` はチェックアウトからの `discover.root`、
/// `rel` は root からのファイルの相対パス。警告のメッセージも返す。
pub fn sv_tests_case(id: String, root: &Path, rel: &Path, text: &str) -> (TestCase, Vec<String>) {
    let mut warnings = Vec::new();
    let mut meta = BTreeMap::new();
    for (k, v) in parse_sv_tests_metadata(text) {
        if is_sv_tests_key(&k) {
            meta.insert(k, v);
        } else {
            warnings.push(format!("unsupported metadata key `{k}`; ignored"));
        }
    }
    let get = |k: &str| meta.get(k).map(String::as_str);

    let files = match get("files") {
        Some(v) => v.split_whitespace().map(|f| root.join(f)).collect(),
        None => vec![root.join(rel)],
    };
    let incdirs = match get("incdirs") {
        Some(v) => v.split_whitespace().map(|f| root.join(f)).collect(),
        None => {
            let parent = rel.parent().filter(|p| !p.as_os_str().is_empty());
            vec![parent.map_or_else(|| root.to_path_buf(), |p| root.join(p))]
        }
    };
    let defines = get("defines")
        .map(|v| v.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default();
    let top = get("top_module")
        .filter(|v| !v.is_empty())
        .map(str::to_string);

    // should_fail の決め方は sv-tests の runner と同じ:
    // 既定は should_fail_because の有無、"0" なのに理由があれば 1 にする。比較は文字列の "1"。
    let because = get("should_fail_because").filter(|v| !v.is_empty());
    let mut sf = get("should_fail").map(str::to_string).unwrap_or_else(|| {
        if meta.contains_key("should_fail_because") {
            "1".into()
        } else {
            "0".into()
        }
    });
    if sf == "0" && because.is_some() {
        sf = "1".into();
    }
    let should_fail = sf == "1";

    let ty = get("type").unwrap_or(SV_TESTS_DEFAULT_TYPE);
    let mut target = None;
    for w in ty.split_whitespace() {
        match type_word_stage(w) {
            Some(s) => target = target.max(Some(s)),
            None => warnings.push(format!("unknown :type: word `{w}`; ignored")),
        }
    }
    let target = target.unwrap_or_else(|| {
        warnings.push(format!(
            ":type: `{ty}` names no known stage; using the default `{SV_TESTS_DEFAULT_TYPE}`"
        ));
        Stage::Elaborate
    });

    let case = TestCase {
        id,
        files,
        incdirs,
        defines,
        top,
        expect: Expectation {
            should_fail,
            reason: because.map(str::to_string),
            target,
        },
        meta,
    };
    (case, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case_of(text: &str) -> (TestCase, Vec<String>) {
        sv_tests_case(
            "sv-tests/a.sv".into(),
            Path::new("tests"),
            Path::new("a.sv"),
            text,
        )
    }

    // TC76: 既定値
    #[test]
    fn defaults() {
        let (c, w) = case_of(":name: a\n:tags: 5.6\nmodule a; endmodule\n");
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(c.files, vec![PathBuf::from("tests/a.sv")]);
        assert_eq!(c.incdirs, vec![PathBuf::from("tests")]);
        assert_eq!(c.expect.target, Stage::Elaborate);
        assert!(!c.expect.should_fail);
        assert_eq!(c.expect.reason, None);
        assert_eq!(c.top, None);
        assert!(c.defines.is_empty());
        assert_eq!(c.meta.get("name").map(String::as_str), Some("a"));
        assert_eq!(c.meta.get("tags").map(String::as_str), Some("5.6"));
    }

    #[test]
    fn default_incdir_is_file_dir() {
        let (c, _) = sv_tests_case(
            "x".into(),
            Path::new("tests"),
            Path::new("sub/dir/z.sv"),
            "",
        );
        assert_eq!(c.files, vec![PathBuf::from("tests/sub/dir/z.sv")]);
        assert_eq!(c.incdirs, vec![PathBuf::from("tests/sub/dir")]);
    }

    // TC76: CRLF
    #[test]
    fn crlf_values_are_trimmed() {
        let (c, w) = case_of(":name: crlf\r\n:type: simulation elaboration parsing\r\n");
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(c.expect.target, Stage::Simulate);
        assert_eq!(
            c.meta.get("type").map(String::as_str),
            Some("simulation elaboration parsing")
        );
        assert!(c.meta.values().all(|v| !v.contains('\r')));
        // 単独の \r も行の区切り
        let (c, _) = case_of(":type: parsing\r:name: x\r");
        assert_eq!(c.expect.target, Stage::Parse);
        assert_eq!(c.meta.get("name").map(String::as_str), Some("x"));
    }

    // TC76: キー名は小文字にする。should_fail_because で should_fail
    #[test]
    fn uppercase_key_and_should_fail_because() {
        let (c, _) = case_of(":Should_Fail_Because: x\n");
        assert!(c.expect.should_fail);
        assert_eq!(c.expect.reason.as_deref(), Some("x"));
        assert!(c.meta.contains_key("should_fail_because"));
    }

    #[test]
    fn should_fail_rules() {
        // sv-tests の runner と同じ
        let sf = |t: &str| case_of(t).0.expect.should_fail;
        assert!(sf(":should_fail: 1\n"));
        assert!(!sf(":should_fail: 0\n"));
        assert!(sf(":should_fail: 0\n:should_fail_because: r\n"));
        assert!(!sf(":should_fail: yes\n:should_fail_because: r\n"));
        // 値が空白だけの should_fail_because でもキーがあれば既定は 1
        assert!(sf(":should_fail_because:   \n"));
        assert_eq!(case_of(":should_fail_because:   \n").0.expect.reason, None);
    }

    // TC76: 重複は後が勝つ
    #[test]
    fn later_key_wins() {
        let (c, _) = case_of(":type: parsing\n:type: simulation\n");
        assert_eq!(c.expect.target, Stage::Simulate);
        assert_eq!(c.meta.get("type").map(String::as_str), Some("simulation"));
    }

    // TC76: 未知のキーは警告して無視。runner_*_flags は meta に残る
    #[test]
    fn unknown_keys_and_runner_flags() {
        let (c, w) = case_of(":foo_bar: 1\n:runner_verilator_flags: -Wno-x\n");
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("foo_bar"));
        assert!(!c.meta.contains_key("foo_bar"));
        assert_eq!(
            c.meta.get("runner_verilator_flags").map(String::as_str),
            Some("-Wno-x")
        );
        assert!(is_sv_tests_key("runner_x_flags"));
        assert!(is_sv_tests_key("runner__flags"));
        assert!(!is_sv_tests_key("runner_flags"));
        assert!(!is_sv_tests_key("runner_x_flag"));
    }

    // TC76: files / incdirs は root からの相対、defines、top_module
    #[test]
    fn explicit_files_incdirs_defines_top() {
        let (c, _) = case_of(
            ":files: sub/x.sv sub/y.sv\n:incdirs: inc\n:defines: A=1 B\n:top_module: top\n",
        );
        assert_eq!(
            c.files,
            vec![
                PathBuf::from("tests/sub/x.sv"),
                PathBuf::from("tests/sub/y.sv")
            ]
        );
        assert_eq!(c.incdirs, vec![PathBuf::from("tests/inc")]);
        assert_eq!(c.defines, vec!["A=1".to_string(), "B".to_string()]);
        assert_eq!(c.top.as_deref(), Some("top"));
    }

    // TC76: :type: の語のうち最も後の段階
    #[test]
    fn type_to_target() {
        for (ty, want) in [
            ("preprocessing", Stage::Preprocess),
            ("parsing", Stage::Parse),
            ("elaboration", Stage::Elaborate),
            ("preprocessing parsing", Stage::Parse),
            ("simulation elaboration preprocessing", Stage::Simulate),
            ("simulation", Stage::Simulate),
            ("parsing  elaboration", Stage::Elaborate),
        ] {
            let (c, w) = case_of(&format!(":type: {ty}\n"));
            assert_eq!(c.expect.target, want, "{ty}");
            assert!(w.is_empty(), "{ty}: {w:?}");
        }
        let (c, w) = case_of(":type: bogus\n");
        assert_eq!(c.expect.target, Stage::Elaborate);
        assert_eq!(w.len(), 2, "{w:?}");
        let (c, w) = case_of(":type: parsing bogus\n");
        assert_eq!(c.expect.target, Stage::Parse);
        assert_eq!(w.len(), 1, "{w:?}");
    }

    // TC76: 行頭でない、値が無い
    #[test]
    fn metadata_regex_edges() {
        let m = parse_sv_tests_metadata(
            " :name: x\n// :name: y\n:tags:\n:tags:  \n::v\n:a1: v\n:a-b_C: v w \n:k:v",
        );
        assert_eq!(
            m,
            vec![
                ("tags".to_string(), String::new()),
                ("a-b_c".to_string(), "v w".to_string()),
                ("k".to_string(), "v".to_string()),
            ]
        );
    }

    #[test]
    fn sv_extension() {
        assert!(has_sv_extension(b"a.sv"));
        assert!(has_sv_extension(b"b.SV"));
        assert!(has_sv_extension(b"c.Sv"));
        assert!(!has_sv_extension(b"c.svh"));
        assert!(!has_sv_extension(b"d.v"));
        assert!(!has_sv_extension(b"sv"));
    }

    fn write(dir: &Path, rel: &str, text: &[u8]) {
        let p = dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    // TC76: ファイルの列挙と順序
    #[test]
    fn discovers_files_in_byte_order() {
        let d = tempfile::tempdir().unwrap();
        let co = d.path();
        write(co, "tests/a.sv", b":name: a\n:tags: 5.6\n");
        write(co, "tests/B.sv", b"");
        write(co, "tests/b.SV", b"");
        write(co, "tests/sub/z.sv", b"");
        write(co, "tests/c.svh", b"");
        write(co, "tests/d.v", b"");
        write(co, "other/e.sv", b"");
        let disc = discover_sv_tests("sv-tests", co, Path::new("tests")).unwrap();
        let ids: Vec<&str> = disc.cases.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "sv-tests/B.sv",
                "sv-tests/a.sv",
                "sv-tests/b.SV",
                "sv-tests/sub/z.sv"
            ]
        );
        assert!(disc.errors.is_empty() && disc.warnings.is_empty());
        let a = &disc.cases[1];
        assert_eq!(a.files, vec![PathBuf::from("tests/a.sv")]);
        assert_eq!(a.incdirs, vec![PathBuf::from("tests")]);
        let z = &disc.cases[3];
        assert_eq!(z.incdirs, vec![PathBuf::from("tests/sub")]);
    }

    #[test]
    fn non_utf8_file_is_an_error() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "tests/bad.sv", b":name: \xff\n");
        write(d.path(), "tests/ok.sv", b"");
        let disc = discover_sv_tests("e", d.path(), Path::new("tests")).unwrap();
        assert_eq!(disc.cases.len(), 1);
        assert_eq!(disc.errors.len(), 1);
        assert_eq!(disc.errors[0].path, PathBuf::from("tests/bad.sv"));
        assert_eq!(disc.stats().errors, 1);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_followed() {
        let d = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        write(outside.path(), "secret.sv", b":name: secret\n");
        write(d.path(), "tests/ok.sv", b"");
        std::os::unix::fs::symlink(
            outside.path().join("secret.sv"),
            d.path().join("tests/link.sv"),
        )
        .unwrap();
        std::os::unix::fs::symlink(outside.path(), d.path().join("tests/dirlink")).unwrap();
        let disc = discover_sv_tests("e", d.path(), Path::new("tests")).unwrap();
        assert_eq!(disc.cases.len(), 1);
        assert_eq!(disc.warnings.len(), 1);
        assert_eq!(disc.warnings[0].path, PathBuf::from("tests/link.sv"));

        // root 自体がチェックアウトの外を指すならエラー
        std::os::unix::fs::symlink(outside.path(), d.path().join("out")).unwrap();
        assert!(discover_sv_tests("e", d.path(), Path::new("out")).is_err());
    }

    #[test]
    fn missing_root_is_an_error() {
        let d = tempfile::tempdir().unwrap();
        assert!(discover_sv_tests("e", d.path(), Path::new("tests")).is_err());
    }

    #[test]
    fn stats_counts() {
        let d = tempfile::tempdir().unwrap();
        let co = d.path();
        write(co, "tests/a.sv", b":type: preprocessing\n");
        write(co, "tests/b.sv", b":should_fail_because: r\n:foo: 1\n");
        write(co, "tests/c.sv", b":type: simulation elaboration parsing\n");
        let s = discover_sv_tests("e", co, Path::new("tests"))
            .unwrap()
            .stats();
        assert_eq!(s.total, 3);
        assert_eq!(s.should_fail, 1);
        assert_eq!(s.warnings, 1);
        assert_eq!(s.by_target[&Stage::Preprocess], 1);
        assert_eq!(s.by_target[&Stage::Parse], 0);
        assert_eq!(s.by_target[&Stage::Elaborate], 1);
        assert_eq!(s.by_target[&Stage::Simulate], 1);
        assert_eq!(s.by_target[&Stage::Compare], 0);
        assert_eq!(s.by_type[NO_TYPE], 1);
        assert_eq!(s.by_type["preprocessing"], 1);
        assert_eq!(s.keys["type"], 2);
        assert_eq!(s.keys["should_fail_because"], 1);
    }
}
