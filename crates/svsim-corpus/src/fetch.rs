//! 外部コマンドの `git` によるコーパスの取得 (plan.md §7.2)。
//!
//! 手順:
//! 1. 完了印があれば何もしない (`--offline` でも成功する)。`--offline` でキャッシュに無ければエラー。
//! 2. `<cache>/<name>/.lock` を取る ([`crate::cache::acquire_lock`])。
//! 3. 一時ディレクトリ `<cache>/<name>/.tmp-<rev>-<pid>` で
//!    `git init` → (sparse の設定) → `git fetch --depth 1 <url> <rev>` → `git checkout --detach FETCH_HEAD`
//!    → (サブモジュールの取得)。
//! 4. `git rev-parse HEAD` が `rev` と一致することを確かめる。
//! 5. 完了印を一時ディレクトリに置いてから、最終位置 `<cache>/<name>/<rev>/` へ `rename` する。
//!    `rename` が失敗しても、最終位置に完了印があれば (別のプロセスが先に完了した) 成功扱いにする。
//!
//! 取得したファイルは信頼しない入力として扱い、読むだけにする (リポジトリ内のスクリプトは実行しない)。
//! 新規に `init` したリポジトリにはフックが無く、さらに `core.hooksPath=/dev/null` で無効にしてある。
//!
//! `git` は `-c protocol.allow=never -c protocol.https.allow=always` で実行し、https 以外の
//! プロトコルを使わせない。`file://` は [`FetchOptions::allow_file_protocol`] (テスト専用) のときだけ許す。
//! 呼び出し元の環境の `GIT_CONFIG_PARAMETERS` / `GIT_CONFIG_COUNT` / `GIT_ALLOW_PROTOCOL` は引き継がない。

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::Error;
use crate::cache::{self, COMPLETE_MARKER, CacheLayout, LockOptions};
use crate::manifest::Entry;

/// 取得の設定。
#[derive(Debug, Clone)]
pub struct FetchOptions {
    /// キャッシュのルート ([`crate::cache::cache_root_from_env`])。
    pub cache_root: PathBuf,
    /// ネットワークに出ない。キャッシュに無ければエラー。
    pub offline: bool,
    /// 既存のロックを持ち主に関係なく取り除く。
    pub force_unlock: bool,
    /// `file://` の取得を許す (`-c protocol.file.allow=always`)。テスト専用。
    ///
    /// マニフェストの検証でも `file://` を許す必要がある ([`crate::manifest::ManifestOptions`])。
    pub allow_file_protocol: bool,
    /// 呼び出す `git` コマンド。
    pub git: PathBuf,
}

impl FetchOptions {
    /// 既定の設定 (オンライン、強制解除なし、`file://` 不可、`git` は `PATH` から)。
    pub fn new(cache_root: PathBuf) -> FetchOptions {
        FetchOptions {
            cache_root,
            offline: false,
            force_unlock: false,
            allow_file_protocol: false,
            git: PathBuf::from("git"),
        }
    }
}

/// 取得の結果の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchStatus {
    /// 既にキャッシュにあった (ネットワークに出ていない)。
    Cached,
    /// 今回取得した。
    Fetched,
}

/// 取得の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchOutcome {
    /// 結果の種類。
    pub status: FetchStatus,
    /// チェックアウトの場所 (`<cache>/<name>/<rev>/`)。
    pub checkout_dir: PathBuf,
}

/// キャッシュに取得済みなら、チェックアウトの場所を返す (列挙などで使う)。
pub fn cached_checkout(entry: &Entry, cache_root: &Path) -> Option<PathBuf> {
    let l = CacheLayout::new(cache_root, &entry.name, &entry.source.rev);
    l.is_complete().then_some(l.checkout_dir)
}

/// エントリを取得する (冪等。2 回目以降はネットワークに出ない)。
pub fn fetch_entry(entry: &Entry, opts: &FetchOptions) -> Result<FetchOutcome, Error> {
    let layout = CacheLayout::new(&opts.cache_root, &entry.name, &entry.source.rev);
    let cached = || {
        Ok(FetchOutcome {
            status: FetchStatus::Cached,
            checkout_dir: layout.checkout_dir.clone(),
        })
    };
    if layout.is_complete() {
        return cached();
    }
    if opts.offline {
        return Err(Error::NotCached {
            name: entry.name.clone(),
            path: layout.checkout_dir.clone(),
        });
    }
    fs::create_dir_all(&layout.entry_dir).map_err(|e| Error::io(&layout.entry_dir, e))?;
    let _lock = cache::acquire_lock(
        &layout.lock_path,
        LockOptions {
            force_unlock: opts.force_unlock,
            alive: None,
        },
    )?;
    // ロックを待つ間に別のプロセスが完了させたかもしれない
    if layout.is_complete() {
        return cached();
    }
    clean_leftovers(&layout)?;
    if layout.checkout_dir.exists() {
        // 完了印の無い最終位置 (外部から壊された等) は作り直す
        fs::remove_dir_all(&layout.checkout_dir).map_err(|e| Error::io(&layout.checkout_dir, e))?;
    }
    let tmp = layout.tmp_dir();
    if tmp.exists() {
        fs::remove_dir_all(&tmp).map_err(|e| Error::io(&tmp, e))?;
    }
    fs::create_dir_all(&tmp).map_err(|e| Error::io(&tmp, e))?;

    let result = populate(entry, opts, &tmp).and_then(|()| finish(entry, &layout, &tmp));
    if result.is_err() || tmp.exists() {
        let _ = fs::remove_dir_all(&tmp);
    }
    result?;
    Ok(FetchOutcome {
        status: FetchStatus::Fetched,
        checkout_dir: layout.checkout_dir.clone(),
    })
}

/// 一時ディレクトリに取得して検証する (手順 3, 4)。
fn populate(entry: &Entry, opts: &FetchOptions, dir: &Path) -> Result<(), Error> {
    let git = Git::new(opts, dir);
    let src = &entry.source;
    git.run(["init", "-q"])?;
    if !src.sparse.is_empty() {
        // non-cone の sparse checkout。各パスはリポジトリのルートに固定する (先頭に `/`)。
        git.run(["config", "core.sparseCheckout", "true"])?;
        let mut patterns = String::new();
        for p in &src.sparse {
            let p = p.trim_start_matches("./");
            patterns.push('/');
            patterns.push_str(p);
            patterns.push('\n');
        }
        let info = dir.join(".git").join("info");
        fs::create_dir_all(&info).map_err(|e| Error::io(&info, e))?;
        let f = info.join("sparse-checkout");
        fs::write(&f, patterns).map_err(|e| Error::io(&f, e))?;
    }
    git.run([
        OsStr::new("fetch"),
        OsStr::new("-q"),
        OsStr::new("--depth"),
        OsStr::new("1"),
        OsStr::new("--no-tags"),
        OsStr::new(&src.git),
        OsStr::new(&src.rev),
    ])?;
    git.run(["checkout", "-q", "--detach", "FETCH_HEAD"])?;
    if !src.submodules.is_empty() {
        let mut args: Vec<&OsStr> = ["submodule", "update", "-q", "--init", "--depth", "1", "--"]
            .into_iter()
            .map(OsStr::new)
            .collect();
        args.extend(src.submodules.iter().map(OsStr::new));
        git.run(args)?;
    }
    let head = git.run(["rev-parse", "HEAD"])?;
    let head = head.trim();
    if head != src.rev {
        return Err(Error::RevMismatch {
            expected: src.rev.clone(),
            actual: head.to_string(),
        });
    }
    Ok(())
}

/// 完了印を置いて最終位置へ移す (手順 5)。
fn finish(entry: &Entry, layout: &CacheLayout, tmp: &Path) -> Result<(), Error> {
    let marker = tmp.join(COMPLETE_MARKER);
    let text = format!(
        "name={}\ngit={}\nrev={}\n",
        entry.name, entry.source.git, entry.source.rev
    );
    fs::write(&marker, text).map_err(|e| Error::io(&marker, e))?;
    match fs::rename(tmp, &layout.checkout_dir) {
        Ok(()) => Ok(()),
        // 別のプロセスが先に完了させた場合 (R-18)
        Err(_) if layout.is_complete() => Ok(()),
        Err(e) => Err(Error::io(&layout.checkout_dir, e)),
    }
}

/// ロックを持っている間に、異常終了したプロセスの残した一時ファイルを消す。
fn clean_leftovers(layout: &CacheLayout) -> Result<(), Error> {
    let rd = match fs::read_dir(&layout.entry_dir) {
        Ok(rd) => rd,
        Err(e) => return Err(Error::io(&layout.entry_dir, e)),
    };
    for ent in rd.flatten() {
        let name = ent.file_name();
        let Some(name) = name.to_str() else { continue };
        let pid = name
            .strip_prefix(".tmp-")
            .and_then(|rest| rest.rsplit('-').next());
        let Some(pid) = pid.and_then(|p| p.parse::<u32>().ok()) else {
            continue;
        };
        if pid == std::process::id() || !cache::process_alive(pid) {
            let p = ent.path();
            let _ = if p.is_dir() {
                fs::remove_dir_all(&p)
            } else {
                fs::remove_file(&p)
            };
        }
    }
    Ok(())
}

/// 環境を固定した `git` の呼び出し (R-18)。
struct Git<'a> {
    program: &'a Path,
    dir: &'a Path,
    allow_file: bool,
}

impl<'a> Git<'a> {
    fn new(opts: &'a FetchOptions, dir: &'a Path) -> Git<'a> {
        Git {
            program: &opts.git,
            dir,
            allow_file: opts.allow_file_protocol,
        }
    }

    fn command(&self) -> Command {
        let mut c = Command::new(self.program);
        c.current_dir(self.dir)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            // 呼び出し元 (別の git のフック等) の設定が漏れてこないようにする
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_OBJECT_DIRECTORY")
            .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_NAMESPACE")
            // 親から `-c` 相当の設定 (protocol.*.allow 等) を引き継がない
            .env_remove("GIT_CONFIG_PARAMETERS")
            .env_remove("GIT_CONFIG_COUNT")
            // GIT_ALLOW_PROTOCOL は protocol.*.allow の設定より優先されるので消す
            .env_remove("GIT_ALLOW_PROTOCOL")
            .args(["-c", "core.hooksPath=/dev/null"])
            .args(["-c", "advice.detachedHead=false"])
            .args(["-c", "init.defaultBranch=main"])
            // 使えるプロトコルは https だけ (ssh, git, http, ext, ローカルパス等は不可)。
            // サブモジュールの取得にも GIT_CONFIG_PARAMETERS 経由で引き継がれる
            .args(["-c", "protocol.allow=never"])
            .args(["-c", "protocol.https.allow=always"])
            .stdin(Stdio::null());
        // protocol.<name>.allow は protocol.allow より優先されるので、利用者の設定ファイル
        // (~/.gitconfig 等) で組み込みのプロトコルが許されていても、ここで明示的に禁じる
        for name in ["ssh", "git", "http", "ext"] {
            c.arg("-c").arg(format!("protocol.{name}.allow=never"));
        }
        // file:// はテスト (一時ディレクトリの bare リポジトリ) のときだけ
        let file = if self.allow_file { "always" } else { "never" };
        c.arg("-c").arg(format!("protocol.file.allow={file}"));
        c
    }

    /// 実行して標準出力を返す。失敗したら標準エラー出力を含むエラーにする。
    fn run<I, S>(&self, args: I) -> Result<String, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args: Vec<S> = args.into_iter().collect();
        let shown = std::iter::once("git".to_string())
            .chain(
                args.iter()
                    .map(|a| a.as_ref().to_string_lossy().into_owned()),
            )
            .collect::<Vec<_>>()
            .join(" ");
        let out = self
            .command()
            .args(&args)
            .output()
            .map_err(|e| Error::Git {
                command: shown.clone(),
                message: format!("cannot run {}: {e}", self.program.display()),
            })?;
        if !out.status.success() {
            return Err(Error::Git {
                command: shown,
                message: format!(
                    "{} ({})",
                    String::from_utf8_lossy(&out.stderr).trim_end(),
                    out.status
                ),
            });
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Manifest;
    use crate::manifest::ManifestOptions;

    const REV: &str = "0123456789abcdef0123456789abcdef01234567";

    fn entry() -> Entry {
        let text = format!(
            "version = 1\n[[entry]]\nname = \"demo\"\nlicense = \"MIT\"\n[entry.source]\ngit = \"file:///x\"\nrev = \"{REV}\"\n[entry.discover]\nkind = \"sv-tests\"\nroot = \"tests\"\n"
        );
        let opts = ManifestOptions {
            allow_file_urls: true,
        };
        Manifest::parse_str_with(&text, "t", opts)
            .unwrap()
            .entries
            .remove(0)
    }

    // R-18 (3): rename が失敗しても、最終位置に完了印があれば成功扱い
    #[test]
    fn finish_accepts_concurrently_completed_checkout() {
        let d = tempfile::tempdir().unwrap();
        let e = entry();
        let layout = CacheLayout::new(d.path(), "demo", REV);
        fs::create_dir_all(&layout.checkout_dir).unwrap();
        fs::write(layout.marker_path(), "done").unwrap();
        fs::write(layout.checkout_dir.join("winner"), "").unwrap();
        let tmp = layout.tmp_dir();
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("loser"), "").unwrap();
        finish(&e, &layout, &tmp).unwrap();
        assert!(layout.checkout_dir.join("winner").exists());
        assert!(!layout.checkout_dir.join("loser").exists());
    }

    #[test]
    fn finish_fails_when_target_is_incomplete() {
        let d = tempfile::tempdir().unwrap();
        let e = entry();
        let layout = CacheLayout::new(d.path(), "demo", REV);
        fs::create_dir_all(&layout.checkout_dir).unwrap();
        fs::write(layout.checkout_dir.join("partial"), "").unwrap();
        let tmp = layout.tmp_dir();
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("x"), "").unwrap();
        assert!(finish(&e, &layout, &tmp).is_err());
    }

    #[test]
    fn finish_moves_tmp_and_writes_marker() {
        let d = tempfile::tempdir().unwrap();
        let e = entry();
        let layout = CacheLayout::new(d.path(), "demo", REV);
        let tmp = layout.tmp_dir();
        fs::create_dir_all(&tmp).unwrap();
        finish(&e, &layout, &tmp).unwrap();
        assert!(!tmp.exists());
        assert!(layout.is_complete());
        let marker = fs::read_to_string(layout.marker_path()).unwrap();
        assert!(marker.contains(&format!("rev={REV}")), "{marker}");
    }

    /// `-c` で渡す設定の一覧。
    fn config_args(c: &Command) -> Vec<String> {
        let args: Vec<String> = c
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        args.windows(2)
            .filter(|w| w[0] == "-c")
            .map(|w| w[1].clone())
            .collect()
    }

    // R-18 / マネージャ決定: https だけを許し、file はテスト時だけ。親の -c 設定を引き継がない
    #[test]
    fn git_command_restricts_protocols() {
        let d = tempfile::tempdir().unwrap();
        let mut o = FetchOptions::new(d.path().to_path_buf());
        let c = Git::new(&o, d.path()).command();
        let cfg = config_args(&c);
        for want in [
            "protocol.allow=never",
            "protocol.https.allow=always",
            "protocol.file.allow=never",
            "protocol.ssh.allow=never",
            "protocol.git.allow=never",
            "protocol.http.allow=never",
            "protocol.ext.allow=never",
            "core.hooksPath=/dev/null",
        ] {
            assert!(cfg.iter().any(|a| a == want), "{want} not in {cfg:?}");
        }
        assert!(!cfg.iter().any(|a| a == "protocol.file.allow=always"));
        let removed: Vec<_> = c
            .get_envs()
            .filter(|(_, v)| v.is_none())
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect();
        for k in [
            "GIT_CONFIG_PARAMETERS",
            "GIT_CONFIG_COUNT",
            "GIT_ALLOW_PROTOCOL",
            "GIT_DIR",
        ] {
            assert!(removed.iter().any(|r| r == k), "{k} not removed");
        }

        o.allow_file_protocol = true;
        let cfg = config_args(&Git::new(&o, d.path()).command());
        assert!(cfg.iter().any(|a| a == "protocol.file.allow=always"));
        assert!(!cfg.iter().any(|a| a == "protocol.file.allow=never"));
    }

    #[test]
    fn offline_does_not_create_cache_dirs() {
        let d = tempfile::tempdir().unwrap();
        let mut o = FetchOptions::new(d.path().join("cache"));
        o.offline = true;
        assert!(matches!(
            fetch_entry(&entry(), &o),
            Err(Error::NotCached { .. })
        ));
        assert!(!d.path().join("cache").exists());
    }
}
