//! コーパスのキャッシュの配置と排他ロック (plan.md §7.2)。
//!
//! - キャッシュの場所は `$SVSIM_CORPUS_CACHE`。未設定なら `$HOME/.cache/svsim/corpus`。
//! - 配置は `<cache>/<name>/<rev>/`。完了印は `<cache>/<name>/<rev>/.svsim-complete`。
//! - 取得中の一時ディレクトリは `<cache>/<name>/.tmp-<rev>-<pid>`。
//! - 排他は `<cache>/<name>/.lock` で行う (R-18)。詳細は [`acquire_lock`]。
//!
//! # ロックファイルの形式 (plan.md §12 Q-20)
//!
//! 2 行のテキスト (各行は LF で終わる):
//!
//! ```text
//! pid=<ロックを取ったプロセスの PID (10 進)>
//! created_unix=<作成時刻。UNIX 時刻の秒 (10 進)>
//! ```
//!
//! 読むときは、古い形式の `created=<秒>` も `created_unix` として受け付ける。知らない行は無視する。
//! `pid` か作成時刻が無い、または 10 進の数として読めないロックは「解析できない」ものとして扱い、
//! 持ち主が生きているか判断できないので取り除かずにエラーにする (`--force-unlock` を案内する)。

use std::fs::{self, File, TryLockError};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
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

/// ロックファイルの中身 (形式はモジュールの doc を参照)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockInfo {
    /// ロックを取ったプロセスの PID。
    pub pid: u32,
    /// 作成時刻 (UNIX 時刻の秒)。
    pub created_unix: u64,
}

impl LockInfo {
    fn current() -> LockInfo {
        LockInfo {
            pid: std::process::id(),
            created_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        }
    }

    /// ロックファイルに書く形式 (`pid=<pid>\ncreated_unix=<secs>\n`)。
    pub fn to_text(&self) -> String {
        format!("pid={}\ncreated_unix={}\n", self.pid, self.created_unix)
    }

    /// ロックファイルの中身を解析する。解析できなければ `None`。
    ///
    /// 古い形式の `created=<secs>` も受け付ける (`created_unix=` があればそちらを使う)。
    pub fn parse(text: &str) -> Option<LockInfo> {
        fn dec<T: std::str::FromStr>(v: &str) -> Option<T> {
            let v = v.trim();
            // `+1` などを受け付けないよう、10 進の数字だけに限る
            if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            v.parse().ok()
        }
        let mut pid = None;
        let mut created_unix = None;
        let mut created_legacy = None;
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("pid=") {
                pid = Some(dec(v)?);
            } else if let Some(v) = line.strip_prefix("created_unix=") {
                created_unix = Some(dec(v)?);
            } else if let Some(v) = line.strip_prefix("created=") {
                created_legacy = Some(dec(v)?);
            }
        }
        Some(LockInfo {
            pid: pid?,
            created_unix: created_unix.or(created_legacy)?,
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

/// 取得したロック。破棄すると (このプロセスが作った) ロックファイルを消し、flock を外す。
#[derive(Debug)]
pub struct LockGuard {
    path: PathBuf,
    info: LockInfo,
    /// flock を持っている開いたファイル。閉じると flock が外れる。
    file: File,
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
        // flock を持ったまま消す。パスが今もこのファイルを指し、中身が自分の書いたものときだけ
        // (--force-unlock で別のロックに替わっていたら触らない)。flock は `file` を閉じたときに外れる。
        if same_file(&self.file, &self.path)
            && let Ok(text) = fs::read_to_string(&self.path)
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

/// `lock_path` のロックを取る (R-18)。待たない。
///
/// 排他の本体は flock ([`File::try_lock`]) で、ロックファイルの中身 (PID) の判定は補助に使う:
///
/// 1. `lock_path` を開き (無ければ作り)、flock を待たずに取る。
///    - 取れなければ、別のプロセスが持っている。`force_unlock` ならファイルを消して作り直し、
///      それ以外は [`Error::Locked`] を返す。
/// 2. 取れたら、`lock_path` が今もそのファイルを指しているか確かめる (取る前に別のプロセスが
///    消して作り直していたら、1 からやり直す)。
/// 3. 中身を確かめる (`force_unlock` なら省く)。
///    - 空: 作ったばかりか、書く前に異常終了した。取ってよい。
///    - 解析でき、その PID のプロセスが存在しない: 古いロック。上書きして取る。
///    - 解析でき、その PID のプロセスが存在する: flock を使わない版の持ち主かもしれないので
///      [`Error::Locked`] (PID の再利用で誤ることがあるが、その場合は `--force-unlock` で解く)。
///    - 解析できない: 判断できないので [`Error::Locked`] (Q-20)。
/// 4. 自分の PID と作成時刻を書く。
///
/// 古いロックは取り除かずに flock を持ったまま上書きするので、複数のプロセスが同時に
/// 古いロックを見つけても、取れるのは 1 つだけになる。
pub fn acquire_lock(lock_path: &Path, opts: LockOptions) -> Result<LockGuard, Error> {
    let alive = opts.alive.unwrap_or(process_alive);
    let io = |e| Error::io(lock_path, e);
    // 他のプロセスが消して作り直すのと競合したときに備えて、数回だけ試す
    for _ in 0..8 {
        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .map_err(io)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                if opts.force_unlock {
                    // 持ち主の flock は消したファイルに残るだけで、新しいファイルには効かない
                    drop(file);
                    match fs::remove_file(lock_path) {
                        Ok(()) => {}
                        Err(e) if e.kind() == ErrorKind::NotFound => {}
                        Err(e) => return Err(io(e)),
                    }
                    continue;
                }
                return Err(Error::Locked {
                    path: lock_path.to_path_buf(),
                    holder: read_holder(lock_path),
                });
            }
            Err(TryLockError::Error(e)) => return Err(io(e)),
        }
        if !same_file(&file, lock_path) {
            continue;
        }
        let mut text = String::new();
        file.read_to_string(&mut text).map_err(io)?;
        if !text.is_empty() && !opts.force_unlock {
            match LockInfo::parse(&text) {
                // 古いロック: 上書きする
                Some(i) if !alive(i.pid) => {}
                holder => {
                    return Err(Error::Locked {
                        path: lock_path.to_path_buf(),
                        holder,
                    });
                }
            }
        }
        let info = LockInfo::current();
        file.set_len(0)
            .and_then(|()| file.seek(SeekFrom::Start(0)).map(|_| ()))
            .and_then(|()| file.write_all(info.to_text().as_bytes()))
            .and_then(|()| file.sync_all())
            .map_err(io)?;
        return Ok(LockGuard {
            path: lock_path.to_path_buf(),
            info,
            file,
        });
    }
    Err(Error::Locked {
        path: lock_path.to_path_buf(),
        holder: read_holder(lock_path),
    })
}

fn read_holder(lock_path: &Path) -> Option<LockInfo> {
    fs::read_to_string(lock_path)
        .ok()
        .and_then(|t| LockInfo::parse(&t))
}

/// 開いているファイル `file` と、パス `path` が指すファイルが同じか。
#[cfg(unix)]
fn same_file(file: &File, path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (file.metadata(), fs::metadata(path)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

/// 開いているファイル `file` と、パス `path` が指すファイルが同じか。
///
/// Unix 以外では確かめる手段を使わず、パスが存在すれば同じとみなす
/// (Windows では開いているファイルは普通は消せないので、作り直しの競合は起きにくい)。
#[cfg(not(unix))]
fn same_file(_file: &File, path: &Path) -> bool {
    path.exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

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

    // Q-20: 形式は `pid=<10進>\ncreated_unix=<秒>\n`
    #[test]
    fn lock_info_format_and_roundtrip() {
        let i = LockInfo {
            pid: 42,
            created_unix: 1_700_000_000,
        };
        assert_eq!(i.to_text(), "pid=42\ncreated_unix=1700000000\n");
        assert_eq!(LockInfo::parse(&i.to_text()), Some(i));
    }

    #[test]
    fn lock_info_accepts_legacy_created() {
        let want = Some(LockInfo {
            pid: 7,
            created_unix: 9,
        });
        assert_eq!(LockInfo::parse("pid=7\ncreated=9\n"), want);
        // CRLF や前後の空白、知らない行は許す
        assert_eq!(LockInfo::parse("pid= 7 \r\nhost=x\r\ncreated=9\r\n"), want);
        // 両方あれば created_unix を使う
        assert_eq!(
            LockInfo::parse("pid=7\ncreated=1\ncreated_unix=9\n"),
            want.clone()
        );
        assert_eq!(LockInfo::parse("created_unix=9\npid=7"), want);
    }

    #[test]
    fn lock_info_rejects_unparsable() {
        for t in [
            "",
            "garbage",
            "pid=1\n",
            "created_unix=1\n",
            "pid=x\ncreated_unix=1\n",
            "pid=1\ncreated_unix=y\n",
            "pid=-1\ncreated_unix=1\n",
            "pid=+1\ncreated_unix=1\n",
            "pid=0x10\ncreated_unix=1\n",
            "pid=\ncreated_unix=1\n",
            "pid=4294967296\ncreated_unix=1\n",
        ] {
            assert_eq!(LockInfo::parse(t), None, "{t:?}");
        }
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
        assert!(text.starts_with("pid=") && text.contains("\ncreated_unix="));
        let info = LockInfo::parse(&text).unwrap();
        assert_eq!(info.pid, std::process::id());
        assert_eq!(g.info(), &info);
        // flock を持っているので、PID の判定に関係なくエラー (K-5)
        let e = acquire_lock(
            &p,
            LockOptions {
                alive: Some(dead),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(
            matches!(&e, Error::Locked { holder: Some(h), .. } if h == &info),
            "{e}"
        );
        assert!(e.to_string().contains("--force-unlock"), "{e}");
        drop(g);
        assert!(!p.exists());
        let _g = acquire_lock(&p, LockOptions::default()).unwrap();
    }

    // K-5: 存在しない PID のロックは自動で取り除く (上書きする)
    #[test]
    fn stale_lock_is_removed() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(".lock");
        fs::write(&p, "pid=4294967294\ncreated_unix=1\n").unwrap();
        // 実際の process_alive で判定する (Linux 以外では生きているとみなすのでエラー)
        let r = acquire_lock(&p, LockOptions::default());
        if cfg!(target_os = "linux") {
            let g = r.unwrap();
            assert_eq!(g.info().pid, std::process::id());
            assert_eq!(fs::read_to_string(&p).unwrap(), g.info().to_text());
        } else {
            assert!(r.is_err());
        }
    }

    #[test]
    fn stale_lock_with_injected_liveness() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(".lock");
        // 古い形式 (created=) でも PID を読んで判定する
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
            Error::Locked { holder, .. } => assert_eq!(
                holder,
                Some(LockInfo {
                    pid: 123,
                    created_unix: 1
                })
            ),
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
        // ロックファイル以外が残っていない
        let names: Vec<_> = fs::read_dir(d.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from(".lock")]);
    }

    // R-18: 古いロックを複数が同時に見つけても、取れるのは 1 つだけ。
    // flock は開いたファイルごとに効くので、同じプロセスのスレッドでも競合を再現できる。
    #[test]
    fn concurrent_stale_lock_takeover_has_single_winner() {
        const N: usize = 8;
        for _round in 0..20 {
            let d = tempfile::tempdir().unwrap();
            let p = Arc::new(d.path().join(".lock"));
            fs::write(&*p, "pid=4294967294\ncreated_unix=1\n").unwrap();
            let start = Arc::new(Barrier::new(N));
            let hold = Arc::new(Barrier::new(N));
            let handles: Vec<_> = (0..N)
                .map(|_| {
                    let (p, start, hold) = (p.clone(), start.clone(), hold.clone());
                    std::thread::spawn(move || {
                        start.wait();
                        // 全員が「持ち主は死んでいる」と判定する最悪の場合
                        let r = acquire_lock(
                            &p,
                            LockOptions {
                                alive: Some(dead),
                                ..Default::default()
                            },
                        );
                        // 全員が試し終わるまでロックを持ち続ける
                        hold.wait();
                        r.is_ok()
                    })
                })
                .collect();
            let wins = handles
                .into_iter()
                .map(|h| h.join().unwrap())
                .filter(|&ok| ok)
                .count();
            assert_eq!(wins, 1);
            assert!(!p.exists(), "lock must be removed by the winner");
        }
    }

    #[test]
    fn force_unlock_removes_live_lock() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(".lock");
        // 生きているプロセス (自分) のロック。作成時刻を変えて、新しく取るロックと区別する
        let live = format!("pid={}\ncreated_unix=1\n", std::process::id());
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

    // flock を持っている相手がいても --force-unlock なら取れる。相手の破棄は新しいロックを消さない
    #[test]
    fn force_unlock_overrides_flock_holder() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(".lock");
        let old = acquire_lock(&p, LockOptions::default()).unwrap();
        let new = acquire_lock(
            &p,
            LockOptions {
                force_unlock: true,
                ..Default::default()
            },
        )
        .unwrap();
        drop(old);
        assert!(p.exists());
        assert_eq!(fs::read_to_string(&p).unwrap(), new.info().to_text());
        // 新しいロックは flock で有効
        assert!(acquire_lock(&p, LockOptions::default()).is_err());
        drop(new);
        assert!(!p.exists());
    }

    // Q-20: 解析できないロックは取り除かずにエラー
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
        assert!(e.to_string().contains("--force-unlock"), "{e}");
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

    // 空のロック (作成直後に異常終了した等) は、flock が取れれば取ってよい
    #[test]
    fn empty_lock_without_holder_is_taken() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(".lock");
        fs::write(&p, "").unwrap();
        let g = acquire_lock(
            &p,
            LockOptions {
                alive: Some(alive),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), g.info().to_text());
    }

    #[test]
    fn guard_does_not_remove_foreign_lock() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(".lock");
        let g = acquire_lock(&p, LockOptions::default()).unwrap();
        fs::write(&p, "pid=1\ncreated_unix=2\n").unwrap();
        drop(g);
        assert_eq!(fs::read_to_string(&p).unwrap(), "pid=1\ncreated_unix=2\n");
    }

    // 取る前にファイルが作り直された場合 (flock を取ったファイルがもうパスに無い) は取り直す
    #[cfg(unix)]
    #[test]
    fn same_file_detects_replaced_path() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(".lock");
        fs::write(&p, "").unwrap();
        let f = File::open(&p).unwrap();
        assert!(same_file(&f, &p));
        fs::remove_file(&p).unwrap();
        assert!(!same_file(&f, &p));
        fs::write(&p, "").unwrap();
        assert!(!same_file(&f, &p));
    }
}
