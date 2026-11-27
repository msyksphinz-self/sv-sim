//! コーパスのキャッシュの配置と排他ロック (plan.md §7.2)。
//!
//! - キャッシュの場所は `$SVSIM_CORPUS_CACHE`。未設定なら `$HOME/.cache/svsim/corpus`。
//! - 配置は `<cache>/<name>/<rev>/`。完了印は `<cache>/<name>/<rev>/.svsim-complete`。
//! - 取得中の一時ディレクトリは `<cache>/<name>/.tmp-<rev>-<pid>`。
//! - 排他は `<cache>/<name>/.lock` を `create_new` で作って行う。中身は PID と作成時刻 (R-18)。

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::Error;

/// キャッシュの場所を指定する環境変数。
pub const CACHE_ENV: &str = "SVSIM_CORPUS_CACHE";

/// 取得が完了したことを示す印のファイル名 (チェックアウトの直下に置く)。
pub const COMPLETE_MARKER: &str = ".svsim-complete";

/// ロックファイルの名前 (`<cache>/<name>/` の直下に置く)。
pub const LOCK_FILE: &str = ".lock";

/// 環境変数からキャッシュのルートを決める。
///
/// `$SVSIM_CORPUS_CACHE` が空でなければそれを、なければ `$HOME/.cache/svsim/corpus` を返す。
pub fn cache_root_from_env() -> Result<PathBuf, Error> {
    resolve_cache_root(
        std::env::var_os(CACHE_ENV).map(PathBuf::from),
        std::env::var_os("HOME").map(PathBuf::from),
    )
}

/// [`cache_root_from_env`] の本体 (環境変数を引数で受け取る。テスト用に分けてある)。
pub fn resolve_cache_root(
    cache_env: Option<PathBuf>,
    home: Option<PathBuf>,
) -> Result<PathBuf, Error> {
    if let Some(p) = cache_env.filter(|p| !p.as_os_str().is_empty()) {
        return Ok(p);
    }
    match home.filter(|p| !p.as_os_str().is_empty()) {
        Some(h) => Ok(h.join(".cache").join("svsim").join("corpus")),
        None => Err(Error::Other(format!(
            "cannot determine the corpus cache directory: set ${CACHE_ENV} or $HOME"
        ))),
    }
}

/// キャッシュの中の、あるエントリのある rev の配置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheLayout {
    /// `<cache>/<name>/`。
    pub entry_dir: PathBuf,
    /// `<cache>/<name>/<rev>/` (チェックアウトの最終位置)。
    pub checkout_dir: PathBuf,
    /// `<cache>/<name>/.lock`。
    pub lock_path: PathBuf,
    rev: String,
}

impl CacheLayout {
    /// 配置を計算する。`name` と `rev` はマニフェストの検証を通ったもの (パス区切りを含まない) を渡す。
    pub fn new(cache_root: &Path, name: &str, rev: &str) -> CacheLayout {
        let entry_dir = cache_root.join(name);
        CacheLayout {
            checkout_dir: entry_dir.join(rev),
            lock_path: entry_dir.join(LOCK_FILE),
            entry_dir,
            rev: rev.to_string(),
        }
    }

    /// 完了印のパス。
    pub fn marker_path(&self) -> PathBuf {
        self.checkout_dir.join(COMPLETE_MARKER)
    }

    /// 取得が完了しているか (完了印があるか)。
    pub fn is_complete(&self) -> bool {
        self.marker_path().is_file()
    }

    /// このプロセスが使う一時ディレクトリ `<cache>/<name>/.tmp-<rev>-<pid>`。
    pub fn tmp_dir(&self) -> PathBuf {
        self.entry_dir
            .join(format!(".tmp-{}-{}", self.rev, std::process::id()))
    }
}

/// ロックファイルの中身。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockInfo {
    /// ロックを取ったプロセスの PID。
    pub pid: u32,
    /// 作成時刻 (UNIX 時刻の秒)。
    pub created: u64,
}

impl LockInfo {
    fn current() -> LockInfo {
        LockInfo {
            pid: std::process::id(),
            created: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        }
    }

    /// ロックファイルに書く形式 (`pid=<pid>\ncreated=<secs>\n`)。
    pub fn to_text(&self) -> String {
        format!("pid={}\ncreated={}\n", self.pid, self.created)
    }

    /// ロックファイルの中身を解析する。形式が違えば `None`。
    pub fn parse(text: &str) -> Option<LockInfo> {
        let mut pid = None;
        let mut created = None;
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("pid=") {
                pid = v.trim().parse().ok();
            } else if let Some(v) = line.strip_prefix("created=") {
                created = v.trim().parse().ok();
            }
        }
        Some(LockInfo {
            pid: pid?,
            created: created?,
        })
    }
}

/// プロセスが存在するか。
///
/// Linux では `/proc/<pid>` の有無で判定する。それ以外の OS では判定できないので、
/// 安全側に倒して「存在する」とみなす (その場合は `--force-unlock` で取り除く)。
pub fn process_alive(pid: u32) -> bool {
    if cfg!(target_os = "linux") && Path::new("/proc/self").exists() {
        Path::new("/proc").join(pid.to_string()).exists()
    } else {
        true
    }
}

/// 取得したロック。破棄すると (このプロセスが作った) ロックファイルを消す。
#[derive(Debug)]
pub struct LockGuard {
    path: PathBuf,
    info: LockInfo,
}

impl LockGuard {
    /// ロックファイルのパス。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 書き込んだ中身。
    pub fn info(&self) -> &LockInfo {
        &self.info
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        // 自分の書いたロックのときだけ消す (--force-unlock 等で他者のものに替わっていたら触らない)
        if let Ok(text) = fs::read_to_string(&self.path)
            && LockInfo::parse(&text).as_ref() == Some(&self.info)
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// ロックの取り方。
#[derive(Debug, Clone, Copy, Default)]
pub struct LockOptions {
    /// 既存のロックを、持ち主が生きていても取り除く (`--force-unlock`)。
    pub force_unlock: bool,
    /// 持ち主が存在しないかを判定する関数 (テストで差し替える)。`None` なら [`process_alive`]。
    pub alive: Option<fn(u32) -> bool>,
}

/// `lock_path` のロックを取る (R-18)。
///
/// - ロックが無ければ `create_new` で作り、PID と作成時刻を書く。
/// - ロックがあり、書かれた PID のプロセスが存在しなければ、古いロックとして取り除いて取り直す。
/// - `force_unlock` なら、持ち主に関係なく取り除いて取り直す。
/// - それ以外は待たずに [`Error::Locked`] を返す。
pub fn acquire_lock(lock_path: &Path, opts: LockOptions) -> Result<LockGuard, Error> {
    let alive = opts.alive.unwrap_or(process_alive);
    // 古いロックの除去と作成の競合に備えて、数回だけ試す
    for _ in 0..4 {
        match try_create(lock_path) {
            Ok(guard) => return Ok(guard),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
            Err(e) => return Err(Error::io(lock_path, e)),
        }
        let text = match fs::read_to_string(lock_path) {
            Ok(t) => t,
            // 読む前に消えた: もう一度作ってみる
            Err(e) if e.kind() == ErrorKind::NotFound => continue,
            Err(e) => return Err(Error::io(lock_path, e)),
        };
        let info = LockInfo::parse(&text);
        let stale = match &info {
            Some(i) => !alive(i.pid),
            // 中身が読めない (書き込みの途中か、異常終了した): 持ち主が分からないので取り除かない
            None => false,
        };
        if !(stale || opts.force_unlock) {
            return Err(Error::Locked {
                path: lock_path.to_path_buf(),
                holder: info,
            });
        }
        remove_if_unchanged(lock_path, &text)?;
    }
    Err(Error::Locked {
        path: lock_path.to_path_buf(),
        holder: fs::read_to_string(lock_path)
            .ok()
            .and_then(|t| LockInfo::parse(&t)),
    })
}

fn try_create(lock_path: &Path) -> std::io::Result<LockGuard> {
    let info = LockInfo::current();
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(lock_path)?;
    let res = f
        .write_all(info.to_text().as_bytes())
        .and_then(|_| f.sync_all());
    if let Err(e) = res {
        drop(f);
        let _ = fs::remove_file(lock_path);
        return Err(e);
    }
    Ok(LockGuard {
        path: lock_path.to_path_buf(),
        info,
    })
}

/// ロックファイルの中身が `expected` のままなら取り除く。
///
/// 古いロックを 2 つのプロセスが同時に見つけた場合に、相手が取り直した新しいロックを消さないよう、
/// いったん自分専用の名前に `rename` してから中身を確かめる。違っていたら元に戻す。
fn remove_if_unchanged(lock_path: &Path, expected: &str) -> Result<(), Error> {
    let aside = lock_path.with_file_name(format!(
        "{}.stale-{}",
        lock_path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        std::process::id()
    ));
    match fs::rename(lock_path, &aside) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(Error::io(lock_path, e)),
    }
    let now = fs::read_to_string(&aside).unwrap_or_default();
    if now != expected {
        // 別のプロセスの新しいロックだった。元の名前に戻す (既に別のロックがあれば戻さない)
        let _ = fs::hard_link(&aside, lock_path);
    }
    let _ = fs::remove_file(&aside);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dead(_: u32) -> bool {
        false
    }
    fn alive(_: u32) -> bool {
        true
    }

    #[test]
    fn cache_root_prefers_env() {
        let r = resolve_cache_root(Some("/c".into()), Some("/h".into())).unwrap();
        assert_eq!(r, PathBuf::from("/c"));
        let r = resolve_cache_root(Some("".into()), Some("/h".into())).unwrap();
        assert_eq!(r, PathBuf::from("/h/.cache/svsim/corpus"));
        let r = resolve_cache_root(None, Some("/h".into())).unwrap();
        assert_eq!(r, PathBuf::from("/h/.cache/svsim/corpus"));
        assert!(resolve_cache_root(None, None).is_err());
    }

    #[test]
    fn layout_paths() {
        let l = CacheLayout::new(Path::new("/c"), "sv-tests", "abc");
        assert_eq!(l.entry_dir, PathBuf::from("/c/sv-tests"));
        assert_eq!(l.checkout_dir, PathBuf::from("/c/sv-tests/abc"));
        assert_eq!(l.lock_path, PathBuf::from("/c/sv-tests/.lock"));
        assert_eq!(
            l.marker_path(),
            PathBuf::from("/c/sv-tests/abc/.svsim-complete")
        );
        assert_eq!(
            l.tmp_dir(),
            PathBuf::from(format!("/c/sv-tests/.tmp-abc-{}", std::process::id()))
        );
    }

    #[test]
    fn lock_info_roundtrip() {
        let i = LockInfo {
            pid: 42,
            created: 1_700_000_000,
        };
        assert_eq!(LockInfo::parse(&i.to_text()), Some(i));
        assert_eq!(LockInfo::parse(""), None);
        assert_eq!(LockInfo::parse("pid=x\ncreated=1\n"), None);
        assert_eq!(LockInfo::parse("pid=1\n"), None);
    }

    #[test]
    fn current_process_is_alive() {
        assert!(process_alive(std::process::id()));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn nonexistent_pid_is_not_alive() {
        // Linux の PID の上限 (pid_max) は最大 2^22 なので、これより大きい PID は存在しない
        assert!(!process_alive(u32::MAX - 1));
    }

    #[test]
    fn lock_is_exclusive_and_released_on_drop() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(".lock");
        let g = acquire_lock(&p, LockOptions::default()).unwrap();
        let text = fs::read_to_string(&p).unwrap();
        let info = LockInfo::parse(&text).unwrap();
        assert_eq!(info.pid, std::process::id());
        assert_eq!(g.info(), &info);
        // 生きているプロセス (自分) のロックではエラー (K-5)
        let e = acquire_lock(&p, LockOptions::default()).unwrap_err();
        assert!(matches!(e, Error::Locked { .. }), "{e}");
        assert!(e.to_string().contains("--force-unlock"), "{e}");
        drop(g);
        assert!(!p.exists());
        let _g = acquire_lock(&p, LockOptions::default()).unwrap();
    }

    // K-5: 存在しない PID のロックは自動で取り除く
    #[test]
    fn stale_lock_is_removed() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(".lock");
        fs::write(&p, "pid=4294967294\ncreated=1\n").unwrap();
        // 実際の process_alive で判定する (Linux 以外では生きているとみなすのでエラー)
        let r = acquire_lock(&p, LockOptions::default());
        if cfg!(target_os = "linux") {
            let g = r.unwrap();
            assert_eq!(g.info().pid, std::process::id());
        } else {
            assert!(r.is_err());
        }
    }

    #[test]
    fn stale_lock_with_injected_liveness() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(".lock");
        fs::write(&p, "pid=123\ncreated=1\n").unwrap();
        let e = acquire_lock(
            &p,
            LockOptions {
                alive: Some(alive),
                ..Default::default()
            },
        )
        .unwrap_err();
        match e {
            Error::Locked { holder, .. } => assert_eq!(holder.unwrap().pid, 123),
            other => panic!("{other}"),
        }
        let g = acquire_lock(
            &p,
            LockOptions {
                alive: Some(dead),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(g.info().pid, std::process::id());
        // 退避用のファイルが残っていない
        let names: Vec<_> = fs::read_dir(d.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from(".lock")]);
    }

    #[test]
    fn force_unlock_removes_live_lock() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(".lock");
        // 生きているプロセス (自分) のロック。作成時刻を変えて、新しく取るロックと区別する
        let live = format!("pid={}\ncreated=1\n", std::process::id());
        fs::write(&p, &live).unwrap();
        assert!(acquire_lock(&p, LockOptions::default()).is_err());
        let g = acquire_lock(
            &p,
            LockOptions {
                force_unlock: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_ne!(fs::read_to_string(&p).unwrap(), live);
        drop(g);
        assert!(!p.exists());
    }

    #[test]
    fn unparsable_lock_is_not_removed_automatically() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(".lock");
        fs::write(&p, "garbage").unwrap();
        let e = acquire_lock(
            &p,
            LockOptions {
                alive: Some(dead),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(matches!(e, Error::Locked { holder: None, .. }));
        assert_eq!(fs::read_to_string(&p).unwrap(), "garbage");
        let _g = acquire_lock(
            &p,
            LockOptions {
                force_unlock: true,
                ..Default::default()
            },
        )
        .unwrap();
    }

    #[test]
    fn guard_does_not_remove_foreign_lock() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(".lock");
        let g = acquire_lock(&p, LockOptions::default()).unwrap();
        fs::write(&p, "pid=1\ncreated=2\n").unwrap();
        drop(g);
        assert_eq!(fs::read_to_string(&p).unwrap(), "pid=1\ncreated=2\n");
    }

    #[test]
    fn remove_if_unchanged_restores_changed_lock() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(".lock");
        fs::write(&p, "pid=9\ncreated=9\n").unwrap();
        remove_if_unchanged(&p, "pid=1\ncreated=1\n").unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "pid=9\ncreated=9\n");
        remove_if_unchanged(&p, "pid=9\ncreated=9\n").unwrap();
        assert!(!p.exists());
    }
}
