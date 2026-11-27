//! コーパスのマニフェスト (`corpus/manifest.toml`) の読み込みと検証 (plan.md §7.1)。
//!
//! - 形式は `version = 1` と `[[entry]]` の並び。未知のキーはエラー (`deny_unknown_fields`)。
//! - M0 のソースは `git` だけ。`rev` は 40 桁のコミット ID だけを許す (再現性のため)。
//! - 複数のマニフェストを読み込めるが、エントリ名が重複したらエラー。
//! - `source.git` は `https://` の URL だけを許す。`file://` はテスト用の
//!   [`ManifestOptions::allow_file_urls`] のときだけ許す (マネージャ決定)。
//!   取得側も `git` を https だけ (テスト時は file も) 許す設定で実行する ([`crate::fetch`])。

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::Error;

/// 対応しているマニフェストの形式の版。
pub const MANIFEST_VERSION: u32 = 1;

/// 既定のマニフェストの、リポジトリのルートからの相対パス。
pub const DEFAULT_MANIFEST_REL: &str = "corpus/manifest.toml";

/// マニフェストの読み込みの設定。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ManifestOptions {
    /// `source.git` に `file://` の URL を許す。テスト専用 (CLI の隠しオプション `--allow-file-protocol`)。
    pub allow_file_urls: bool,
}

/// マニフェスト 1 ファイル分 (TOML のトップレベル)。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestFile {
    version: u32,
    #[serde(default)]
    entry: Vec<Entry>,
}

/// コーパスのエントリ 1 件 (`[[entry]]`)。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// エントリ名。キャッシュのディレクトリ名にも使うので、`[A-Za-z0-9._-]` だけを許す。
    pub name: String,
    /// 説明 (人間向け)。
    #[serde(default)]
    pub description: String,
    /// 取得するコーパスのライセンス (SPDX 式)。
    pub license: String,
    /// 分類用のタグ。
    #[serde(default)]
    pub tags: Vec<String>,
    /// 取得元。
    pub source: Source,
    /// テストケースの列挙方法。
    pub discover: Discover,
}

/// 取得元 (`[entry.source]`)。M0 は git だけ。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// git リポジトリの URL。
    pub git: String,
    /// 40 桁 (小文字の 16 進) のコミット ID。ブランチ名やタグは不可。
    pub rev: String,
    /// 取得するサブモジュールのパス (リポジトリのルートからの相対)。空なら取得しない。
    #[serde(default)]
    pub submodules: Vec<String>,
    /// sparse checkout で取り出すパス (リポジトリのルートからの相対)。空なら全体。
    #[serde(default)]
    pub sparse: Vec<String>,
}

/// テストケースの列挙方法 (`[entry.discover]`)。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Discover {
    /// アダプタの種類。
    pub kind: DiscoverKind,
    /// 列挙の起点 (取得したリポジトリのルートからの相対パス)。
    pub root: PathBuf,
}

/// 列挙アダプタの種類。M0 は `sv-tests` だけ (`glob` は M3 で追加する)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiscoverKind {
    /// CHIPS Alliance sv-tests のメタデータ (`:name:` 等) を解釈する。
    SvTests,
}

impl fmt::Display for DiscoverKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiscoverKind::SvTests => f.write_str("sv-tests"),
        }
    }
}

/// 読み込んだマニフェストの全体 (複数ファイルの和)。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Manifest {
    /// エントリ。読み込んだ順 (ファイルの順、ファイル内の順)。名前は一意。
    pub entries: Vec<Entry>,
}

impl Manifest {
    /// マニフェストの文字列を解析して検証する。`origin` はエラーメッセージ用 (ファイル名など)。
    ///
    /// 既定の設定 ([`ManifestOptions::default`]。`file://` は不可) で検証する。
    pub fn parse_str(text: &str, origin: &str) -> Result<Manifest, Error> {
        Manifest::parse_str_with(text, origin, ManifestOptions::default())
    }

    /// [`Manifest::parse_str`] の、設定を指定する版。
    pub fn parse_str_with(
        text: &str,
        origin: &str,
        opts: ManifestOptions,
    ) -> Result<Manifest, Error> {
        let file: ManifestFile = toml::from_str(text).map_err(|e| Error::Manifest {
            origin: origin.to_string(),
            message: e.to_string(),
        })?;
        let bad = |message: String| Error::Manifest {
            origin: origin.to_string(),
            message,
        };
        if file.version != MANIFEST_VERSION {
            return Err(bad(format!(
                "unsupported manifest version {} (expected {MANIFEST_VERSION})",
                file.version
            )));
        }
        let mut m = Manifest::default();
        for e in file.entry {
            validate_entry(&e, opts).map_err(|msg| bad(format!("entry `{}`: {msg}", e.name)))?;
            m.push_unique(e, origin)?;
        }
        Ok(m)
    }

    /// ファイルから読み込む (既定の設定)。
    pub fn load_file(path: &Path) -> Result<Manifest, Error> {
        Manifest::load_file_with(path, ManifestOptions::default())
    }

    /// [`Manifest::load_file`] の、設定を指定する版。
    pub fn load_file_with(path: &Path, opts: ManifestOptions) -> Result<Manifest, Error> {
        let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
        Manifest::parse_str_with(&text, &path.display().to_string(), opts)
    }

    /// 複数のファイルを順に読み込んで併合する (既定の設定)。同じファイル (正規化したパスが同じ) は
    /// 1 回だけ読む。異なるファイルの間でエントリ名が重複したらエラー。
    pub fn load_files(paths: &[PathBuf]) -> Result<Manifest, Error> {
        Manifest::load_files_with(paths, ManifestOptions::default())
    }

    /// [`Manifest::load_files`] の、設定を指定する版。
    pub fn load_files_with(paths: &[PathBuf], opts: ManifestOptions) -> Result<Manifest, Error> {
        let mut seen = BTreeSet::new();
        let mut all = Manifest::default();
        for p in paths {
            let key = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
            if !seen.insert(key) {
                continue;
            }
            let m = Manifest::load_file_with(p, opts)?;
            all.merge(m, &p.display().to_string())?;
        }
        Ok(all)
    }

    /// 別のマニフェストを併合する。エントリ名の重複はエラー。
    pub fn merge(&mut self, other: Manifest, origin: &str) -> Result<(), Error> {
        for e in other.entries {
            self.push_unique(e, origin)?;
        }
        Ok(())
    }

    fn push_unique(&mut self, e: Entry, origin: &str) -> Result<(), Error> {
        if self.get(&e.name).is_some() {
            return Err(Error::Manifest {
                origin: origin.to_string(),
                message: format!("duplicate entry name `{}`", e.name),
            });
        }
        self.entries.push(e);
        Ok(())
    }

    /// 名前でエントリを引く。
    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name == name)
    }
}

/// `rev` が 40 桁の小文字 16 進 (SHA-1 のコミット ID) か。
pub fn is_commit_id(rev: &str) -> bool {
    rev.len() == 40 && rev.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// エントリ名として使えるか (キャッシュのディレクトリ名になるため、パス区切りや `.` 始まりを禁じる)。
pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// リポジトリ内の相対パスとして安全か (絶対パス、`..`、空を禁じる)。
fn is_safe_relative(p: &Path) -> bool {
    !p.as_os_str().is_empty()
        && p.components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

/// `source.git` の URL を検証する。
///
/// `https://<何か>` だけを許す (スキームは小文字の `https` に限る)。`file://` は
/// `opts.allow_file_urls` のときだけ許す (`file` の大文字小文字は区別しない)。
/// それ以外 (`HTTPS://`、`ssh://`、`git@host:path`、`http://`、ローカルパスなど) はエラー。
fn validate_git_url(url: &str, opts: ManifestOptions) -> Result<(), String> {
    if url.is_empty()
        || url.starts_with('-')
        || url.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(format!("source.git `{url}` is not a valid URL"));
    }
    match url.split_once("://") {
        Some(("https", rest)) if !rest.is_empty() => Ok(()),
        Some((s, _)) if s.eq_ignore_ascii_case("file") => {
            if opts.allow_file_urls {
                Ok(())
            } else {
                Err(format!(
                    "source.git `{url}`: file:// URLs are only allowed for tests (--allow-file-protocol)"
                ))
            }
        }
        _ => Err(format!("source.git `{url}` must be an https:// URL")),
    }
}

fn validate_entry(e: &Entry, opts: ManifestOptions) -> Result<(), String> {
    if !is_valid_name(&e.name) {
        return Err(
            "name must be non-empty, consist of [A-Za-z0-9._-] and not start with `.`".into(),
        );
    }
    if !is_commit_id(&e.source.rev) {
        return Err(format!(
            "source.rev `{}` must be a 40-digit lowercase hex commit id (branch or tag names are not allowed)",
            e.source.rev
        ));
    }
    validate_git_url(&e.source.git, opts)?;
    for (key, list) in [
        ("source.submodules", &e.source.submodules),
        ("source.sparse", &e.source.sparse),
    ] {
        for p in list {
            if p.starts_with('-') || !is_safe_relative(Path::new(p)) {
                return Err(format!(
                    "{key}: `{p}` must be a relative path inside the repository"
                ));
            }
        }
    }
    if !is_safe_relative(&e.discover.root) {
        return Err(format!(
            "discover.root `{}` must be a relative path inside the repository",
            e.discover.root.display()
        ));
    }
    Ok(())
}

/// 既定のマニフェストのパスを探す。
///
/// カレントディレクトリから上にたどって `corpus/manifest.toml` を探し、見つからなければ
/// この crate のソースの位置 (ビルド時のワークスペース) にあるものを使う。どちらも無ければ `None`。
pub fn default_manifest_path() -> Option<PathBuf> {
    if let Ok(cwd) = std::env::current_dir() {
        for dir in cwd.ancestors() {
            let p = dir.join(DEFAULT_MANIFEST_REL);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(DEFAULT_MANIFEST_REL);
    p.is_file().then_some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    const REV: &str = "7274242bcf99a2f6f8282493d0200c92eb87d347";

    fn entry_toml(name: &str, rev: &str) -> String {
        format!(
            r#"
[[entry]]
name = "{name}"
description = "d"
license = "ISC"
tags = ["standard"]
[entry.source]
git = "https://example.invalid/repo"
rev = "{rev}"
submodules = []
sparse = []
[entry.discover]
kind = "sv-tests"
root = "tests"
"#
        )
    }

    fn manifest(body: &str) -> String {
        format!("version = 1\n{body}")
    }

    fn err_msg(r: Result<Manifest, Error>) -> String {
        match r {
            Ok(m) => panic!("expected error, got {m:?}"),
            Err(e) => e.to_string(),
        }
    }

    #[test]
    fn parses_valid_manifest() {
        let m = Manifest::parse_str(&manifest(&entry_toml("sv-tests", REV)), "t").unwrap();
        assert_eq!(m.entries.len(), 1);
        let e = &m.entries[0];
        assert_eq!(e.name, "sv-tests");
        assert_eq!(e.source.rev, REV);
        assert_eq!(e.discover.kind, DiscoverKind::SvTests);
        assert_eq!(e.discover.root, PathBuf::from("tests"));
        assert!(m.get("sv-tests").is_some());
        assert!(m.get("nope").is_none());
    }

    #[test]
    fn optional_fields_default() {
        let text = format!(
            "version = 1\n[[entry]]\nname = \"a\"\nlicense = \"MIT\"\n[entry.source]\ngit = \"https://example.invalid/x\"\nrev = \"{REV}\"\n[entry.discover]\nkind = \"sv-tests\"\nroot = \"tests\"\n"
        );
        let m = Manifest::parse_str(&text, "t").unwrap();
        let e = &m.entries[0];
        assert!(e.description.is_empty() && e.tags.is_empty());
        assert!(e.source.sparse.is_empty() && e.source.submodules.is_empty());
    }

    #[test]
    fn empty_manifest_has_no_entries() {
        assert!(
            Manifest::parse_str("version = 1\n", "t")
                .unwrap()
                .entries
                .is_empty()
        );
    }

    // K-4: rev がコミット ID でない
    #[test]
    fn rejects_non_commit_rev() {
        for rev in [
            "main",
            "v1.0",
            "7274242b",
            "7274242BCF99A2F6F8282493D0200C92EB87D347",
            "7274242bcf99a2f6f8282493d0200c92eb87d34g",
            "7274242bcf99a2f6f8282493d0200c92eb87d3470",
        ] {
            let msg = err_msg(Manifest::parse_str(&manifest(&entry_toml("a", rev)), "t"));
            assert!(msg.contains("commit id"), "{rev}: {msg}");
        }
    }

    // K-4: 名前の重複 (同じファイル内)
    #[test]
    fn rejects_duplicate_name_in_one_file() {
        let body = format!("{}{}", entry_toml("a", REV), entry_toml("a", REV));
        let msg = err_msg(Manifest::parse_str(&manifest(&body), "t"));
        assert!(msg.contains("duplicate entry name `a`"), "{msg}");
    }

    // K-4: 名前の重複 (ファイルをまたぐ)
    #[test]
    fn rejects_duplicate_name_across_files() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("a.toml");
        let p2 = dir.path().join("b.toml");
        std::fs::write(&p1, manifest(&entry_toml("x", REV))).unwrap();
        std::fs::write(&p2, manifest(&entry_toml("x", REV))).unwrap();
        let msg = err_msg(Manifest::load_files(&[p1.clone(), p2]));
        assert!(msg.contains("duplicate entry name `x`"), "{msg}");
        // 同じファイルを 2 回指定しても重複にはしない
        let m = Manifest::load_files(&[p1.clone(), p1]).unwrap();
        assert_eq!(m.entries.len(), 1);
    }

    // K-4: 未知のキー (トップレベル、entry、source、discover)
    #[test]
    fn rejects_unknown_keys() {
        let base = manifest(&entry_toml("a", REV));
        let cases = [
            base.replace("version = 1", "version = 1\nextra = 1"),
            base.replace("license = \"ISC\"", "license = \"ISC\"\nfoo = 1"),
            base.replace("sparse = []", "sparse = []\nbranch = \"main\""),
            base.replace("root = \"tests\"", "root = \"tests\"\npattern = \"*.sv\""),
        ];
        for text in cases {
            let msg = err_msg(Manifest::parse_str(&text, "t"));
            assert!(msg.contains("unknown field"), "{msg}");
        }
    }

    #[test]
    fn rejects_unknown_discover_kind() {
        let text =
            manifest(&entry_toml("a", REV)).replace("kind = \"sv-tests\"", "kind = \"glob\"");
        let msg = err_msg(Manifest::parse_str(&text, "t"));
        assert!(msg.contains("unknown variant"), "{msg}");
    }

    #[test]
    fn rejects_missing_required_fields() {
        let text = manifest(&entry_toml("a", REV)).replace("license = \"ISC\"\n", "");
        let msg = err_msg(Manifest::parse_str(&text, "t"));
        assert!(msg.contains("license"), "{msg}");
        let msg = err_msg(Manifest::parse_str("", "t"));
        assert!(msg.contains("version"), "{msg}");
    }

    #[test]
    fn rejects_unsupported_version() {
        let msg = err_msg(Manifest::parse_str("version = 2\n", "t"));
        assert!(msg.contains("unsupported manifest version 2"), "{msg}");
    }

    #[test]
    fn rejects_bad_names() {
        for name in ["", ".hidden", "a/b", "..", "a b", "a\\b"] {
            let msg = err_msg(Manifest::parse_str(&manifest(&entry_toml(name, REV)), "t"));
            assert!(msg.contains("name must be"), "{name:?}: {msg}");
        }
    }

    #[test]
    fn rejects_unsafe_paths_and_urls() {
        let base = manifest(&entry_toml("a", REV));
        let cases = [
            base.replace("sparse = []", "sparse = [\"../x\"]"),
            base.replace("sparse = []", "sparse = [\"/abs\"]"),
            base.replace("submodules = []", "submodules = [\"--init\"]"),
            base.replace("root = \"tests\"", "root = \"../up\""),
            base.replace("root = \"tests\"", "root = \"\""),
            base.replace(
                "git = \"https://example.invalid/repo\"",
                "git = \"--upload-pack=x\"",
            ),
            base.replace("git = \"https://example.invalid/repo\"", "git = \"\""),
        ];
        for text in cases {
            assert!(Manifest::parse_str(&text, "t").is_err(), "{text}");
        }
    }

    // マネージャ決定 (STATUS.md「corpus file://」): file:// はテスト用のフラグのときだけ
    #[test]
    fn file_urls_require_test_flag() {
        let base = manifest(&entry_toml("a", REV));
        for url in [
            "file:///tmp/repo.git",
            "FILE:///tmp/repo.git",
            "file://host/x",
        ] {
            let text = base.replace("https://example.invalid/repo", url);
            let msg = err_msg(Manifest::parse_str(&text, "t"));
            assert!(msg.contains("--allow-file-protocol"), "{url}: {msg}");
            let allow = ManifestOptions {
                allow_file_urls: true,
            };
            let m = Manifest::parse_str_with(&text, "t", allow).unwrap();
            assert_eq!(m.entries[0].source.git, url);
        }
    }

    // https 以外 (取得側の git でも禁じているプロトコル) は検証で弾く
    #[test]
    fn non_https_urls_are_rejected() {
        let base = manifest(&entry_toml("a", REV));
        let allow = ManifestOptions {
            allow_file_urls: true,
        };
        for url in [
            "http://example.invalid/repo",
            "ssh://git@example.invalid/repo",
            "git://example.invalid/repo",
            "ftp://example.invalid/repo",
            "ftps://example.invalid/repo",
            "git@example.invalid:repo.git",
            "ext::sh -c x",
            "/abs/path/repo.git",
            "../relative/repo.git",
            "https://",
            "https://a\u{7}b",
        ] {
            let text = base.replace("https://example.invalid/repo", url);
            assert!(
                Manifest::parse_str_with(&text, "t", allow).is_err(),
                "{url}"
            );
        }
        // https のスキームは小文字だけ (T13 追加項目)
        for url in ["HTTPS://example.invalid/r", "Https://example.invalid/r"] {
            let text = base.replace("https://example.invalid/repo", url);
            let msg = err_msg(Manifest::parse_str_with(&text, "t", allow));
            assert!(msg.contains("must be an https:// URL"), "{url}: {msg}");
        }
        let text = base.replace("https://example.invalid/repo", "https://example.invalid/r");
        assert!(Manifest::parse_str(&text, "t").is_ok());
    }

    #[test]
    fn load_files_with_passes_options() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.toml");
        let text =
            manifest(&entry_toml("x", REV)).replace("https://example.invalid/repo", "file:///r");
        std::fs::write(&p, text).unwrap();
        assert!(Manifest::load_files(std::slice::from_ref(&p)).is_err());
        let allow = ManifestOptions {
            allow_file_urls: true,
        };
        assert_eq!(
            Manifest::load_files_with(&[p], allow)
                .unwrap()
                .entries
                .len(),
            1
        );
    }

    #[test]
    fn error_mentions_origin() {
        let msg = err_msg(Manifest::parse_str("version = 2\n", "my/manifest.toml"));
        assert!(msg.contains("my/manifest.toml"), "{msg}");
    }

    /// リポジトリの `corpus/manifest.toml` が解析でき、sv-tests を固定コミットで指していること。
    #[test]
    fn repository_manifest_is_valid() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/manifest.toml");
        let m = Manifest::load_file(&p).unwrap();
        let e = m.get("sv-tests").expect("sv-tests entry");
        assert_eq!(e.source.rev, REV);
        assert_eq!(e.source.git, "https://github.com/chipsalliance/sv-tests");
        assert_eq!(e.discover.kind, DiscoverKind::SvTests);
        assert_eq!(e.discover.root, PathBuf::from("tests"));
        assert!(default_manifest_path().is_some());
    }
}
