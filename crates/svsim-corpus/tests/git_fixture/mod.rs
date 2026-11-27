//! テスト用のローカル git リポジトリ (ネットワークに出ない。plan.md §8.3 T12)。
//!
//! ユーザの `~/.gitconfig` (署名の設定など) の影響を受けないよう、`GIT_CONFIG_GLOBAL=/dev/null` で実行する。

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// `dir` で git を実行し、標準出力 (末尾の改行を除く) を返す。失敗したら panic する。
pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .args([
            "-c",
            "user.name=svsim-test",
            "-c",
            "user.email=svsim-test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
            "-c",
            "protocol.file.allow=always",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {args:?} in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

/// `file://` の URL。
pub fn file_url(p: &Path) -> String {
    format!("file://{}", p.display())
}

/// 作業ツリーにファイルを書く (親ディレクトリも作る)。
pub fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

/// 全部を add してコミットし、コミット ID を返す。
pub fn commit_all(dir: &Path, msg: &str) -> String {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", msg]);
    git(dir, &["rev-parse", "HEAD"])
}

/// テスト用のリポジトリ一式。
pub struct Fixture {
    /// 一時ディレクトリ (破棄で消える)。
    pub tmp: tempfile::TempDir,
    /// bare リポジトリ (取得元)。
    pub bare: PathBuf,
    /// 1 つ目のコミット (先端ではない)。
    pub rev1: String,
    /// 2 つ目のコミット (先端)。
    pub rev2: String,
}

impl Fixture {
    /// 2 コミットのリポジトリを作り、bare に複製する。
    ///
    /// rev1: `tests/a.sv`, `LICENSE`, `other/x.txt`。rev2: rev1 + `tests/b.sv`。
    pub fn new() -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path().join("work");
        std::fs::create_dir_all(&work).unwrap();
        git(&work, &["init", "-q"]);
        write(&work, "tests/a.sv", "module a; endmodule\n");
        write(&work, "LICENSE", "ISC\n");
        write(&work, "other/x.txt", "x\n");
        let rev1 = commit_all(&work, "one");
        write(&work, "tests/b.sv", "module b; endmodule\n");
        let rev2 = commit_all(&work, "two");
        let bare = tmp.path().join("src.git");
        git(
            tmp.path(),
            &["clone", "-q", "--bare", work.to_str().unwrap(), "src.git"],
        );
        Fixture {
            tmp,
            bare,
            rev1,
            rev2,
        }
    }

    /// キャッシュ用のディレクトリ (一時ディレクトリの中)。
    pub fn cache(&self) -> PathBuf {
        self.tmp.path().join("cache")
    }

    /// このリポジトリを指すマニフェストの TOML。
    pub fn manifest_toml(&self, name: &str, rev: &str, extra_source: &str) -> String {
        format!(
            r#"version = 1

[[entry]]
name = "{name}"
description = "test corpus"
license = "ISC"
[entry.source]
git = "{url}"
rev = "{rev}"
{extra_source}
[entry.discover]
kind = "sv-tests"
root = "tests"
"#,
            url = file_url(&self.bare)
        )
    }
}
