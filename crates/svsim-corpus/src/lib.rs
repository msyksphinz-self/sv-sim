//! sv-sim のテストコーパスマネージャ: マニフェスト、取得とキャッシュ、テストケースの列挙、段階評価。
//!
//! 設計は `docs/milestones/M0/plan.md` §7 を参照。
//!
//! - [`manifest`] — `corpus/manifest.toml` の読み込みと検証 (§7.1、タスク M0-T12)
//! - [`cache`] — キャッシュの配置と排他ロック (§7.2、M0-T12)
//! - [`fetch`] — 外部コマンドの `git` による取得と検証 (§7.2、M0-T12)
//! - [`discover`] — テストケースの列挙 (sv-tests アダプタ。§7.3、M0-T13)
//! - [`stage`] — 段階評価の枠組み (§7.4、M0-T13)
//! - [`report`] — 結果の出力 (`results.jsonl`、`summary.json`。§7.4、M0-T13)

#![forbid(unsafe_code)]

pub mod cache;
pub mod discover;
pub mod fetch;
pub mod manifest;
pub mod report;
pub mod stage;

use std::fmt;
use std::path::{Path, PathBuf};

pub use cache::LockInfo;
pub use discover::{Expectation, TestCase};
pub use manifest::{Entry, Manifest};
pub use stage::{Pipeline, Stage, StageOutcome, StageRunner, Verdict};

/// コーパスマネージャのエラー。
#[derive(Debug)]
pub enum Error {
    /// ファイル操作の失敗。
    Io {
        /// 対象のパス。
        path: PathBuf,
        /// 元のエラー。
        source: std::io::Error,
    },
    /// マニフェストの誤り (構文、未知のキー、rev の形式、名前の重複など。K-4)。
    Manifest {
        /// どのマニフェストか (ファイル名など)。
        origin: String,
        /// 内容。
        message: String,
    },
    /// `git` の実行の失敗。
    Git {
        /// 実行したコマンド (表示用)。
        command: String,
        /// 標準エラー出力などの内容。
        message: String,
    },
    /// 取得したコミットが `rev` と一致しない。
    RevMismatch {
        /// マニフェストの `rev`。
        expected: String,
        /// `git rev-parse HEAD` の結果。
        actual: String,
    },
    /// 他のプロセスがロックを持っている (K-5)。
    Locked {
        /// ロックファイルのパス。
        path: PathBuf,
        /// ロックの中身 (読めた場合)。
        holder: Option<LockInfo>,
    },
    /// `--offline` で、キャッシュに無い。
    NotCached {
        /// エントリ名。
        name: String,
        /// 期待する置き場所。
        path: PathBuf,
    },
    /// まだ取得していない (`discover` / `run` の前に `fetch` が要る)。
    NotFetched {
        /// エントリ名。
        name: String,
        /// 期待する置き場所。
        path: PathBuf,
    },
    /// マニフェストに無いエントリ名が指定された。
    UnknownEntry(String),
    /// その他。
    Other(String),
}

impl Error {
    /// [`Error::Io`] を作る。
    pub fn io(path: &Path, source: std::io::Error) -> Error {
        Error::Io {
            path: path.to_path_buf(),
            source,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Error::Manifest { origin, message } => write!(f, "manifest {origin}: {message}"),
            Error::Git { command, message } => {
                write!(f, "`{command}` failed: {}", message.trim_end())
            }
            Error::RevMismatch { expected, actual } => write!(
                f,
                "fetched commit {actual} does not match the manifest rev {expected}"
            ),
            Error::Locked { path, holder } => {
                write!(f, "{} is locked", path.display())?;
                if let Some(h) = holder {
                    write!(
                        f,
                        " by pid {} (created at unix time {})",
                        h.pid, h.created_unix
                    )?;
                }
                write!(
                    f,
                    "; another fetch may be running. If not, rerun with --force-unlock"
                )
            }
            Error::NotCached { name, path } => write!(
                f,
                "`{name}` is not in the cache ({}) and --offline was given",
                path.display()
            ),
            Error::NotFetched { name, path } => write!(
                f,
                "`{name}` has not been fetched ({}); run `svsim-corpus fetch {name}` first",
                path.display()
            ),
            Error::UnknownEntry(name) => write!(f, "no manifest entry named `{name}`"),
            Error::Other(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
