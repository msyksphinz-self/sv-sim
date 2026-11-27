//! `svsim-corpus` コマンド (`cargo corpus <subcmd>`)。plan.md §7.5。
//!
//! - `list`: マニフェストのエントリの一覧
//! - `fetch [NAME...] [--offline] [--force-unlock]`: 取得 (名前を省略すると全部)
//!
//! `discover` と `run` はタスク M0-T13 で追加する。
//!
//! 終了コード (plan.md §12 Q-18): 0 = 成功、1 = 実行時のエラー (取得の失敗、マニフェストが
//! 見つからない・読めない・誤っている等)、2 = 使い方の誤り (clap の引数エラー、存在しないエントリ名)。

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use svsim_corpus::fetch::{self, FetchOptions, FetchStatus};
use svsim_corpus::manifest::ManifestOptions;
use svsim_corpus::{Error, Manifest, cache, manifest};

/// sv-sim test corpus manager.
#[derive(Debug, Parser)]
#[command(name = "svsim-corpus", version)]
struct Cli {
    /// Additional manifest file(s), read after the default `corpus/manifest.toml`.
    #[arg(long = "manifest", value_name = "PATH", global = true)]
    manifests: Vec<PathBuf>,

    /// Allow `file://` sources in manifests and fetches (for tests only).
    #[arg(long, hide = true, global = true)]
    allow_file_protocol: bool,

    #[command(subcommand)]
    command: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// List manifest entries.
    List,
    /// Fetch corpora into the cache ($SVSIM_CORPUS_CACHE or ~/.cache/svsim/corpus).
    Fetch {
        /// Entry names (all entries if omitted).
        names: Vec<String>,
        /// Do not access the network; fail if an entry is not cached.
        #[arg(long)]
        offline: bool,
        /// Remove an existing lock even if its owner seems to be alive.
        #[arg(long)]
        force_unlock: bool,
    },
}

/// 実行時のエラー (終了コード 1) と使い方の誤り (終了コード 2) を分ける。
enum Failure {
    Runtime(Error),
    Usage(String),
}

impl From<Error> for Failure {
    fn from(e: Error) -> Failure {
        match e {
            Error::UnknownEntry(_) => Failure::Usage(e.to_string()),
            e => Failure::Runtime(e),
        }
    }
}

fn main() -> ExitCode {
    // clap は使い方の誤りで終了コード 2 を返す
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(Failure::Runtime(e)) => {
            eprintln!("svsim-corpus: error: {e}");
            ExitCode::from(1)
        }
        Err(Failure::Usage(msg)) => {
            eprintln!("svsim-corpus: error: {msg}");
            ExitCode::from(2)
        }
    }
}

fn load_manifest(extra: &[PathBuf], opts: ManifestOptions) -> Result<Manifest, Failure> {
    load_manifest_from(manifest::default_manifest_path(), extra, opts)
}

/// 既定のマニフェスト (見つかった場合) と `--manifest` の指定を読み込む。
///
/// どちらも無ければ実行時のエラー (終了コード 1。Q-18: 環境やファイルの問題)。
fn load_manifest_from(
    default: Option<PathBuf>,
    extra: &[PathBuf],
    opts: ManifestOptions,
) -> Result<Manifest, Failure> {
    let mut paths = Vec::new();
    paths.extend(default);
    paths.extend(extra.iter().cloned());
    if paths.is_empty() {
        return Err(Failure::Runtime(Error::Other(format!(
            "no manifest found ({} was not found; use --manifest)",
            manifest::DEFAULT_MANIFEST_REL
        ))));
    }
    Ok(Manifest::load_files_with(&paths, opts)?)
}

/// 名前で選ぶ。省略時は全部。知らない名前は使い方の誤り。
fn select<'m>(m: &'m Manifest, names: &[String]) -> Result<Vec<&'m manifest::Entry>, Failure> {
    if names.is_empty() {
        return Ok(m.entries.iter().collect());
    }
    names
        .iter()
        .map(|n| {
            m.get(n)
                .ok_or_else(|| Error::UnknownEntry(n.clone()).into())
        })
        .collect()
}

fn run(cli: Cli) -> Result<ExitCode, Failure> {
    let mopts = ManifestOptions {
        allow_file_urls: cli.allow_file_protocol,
    };
    let m = load_manifest(&cli.manifests, mopts)?;
    match cli.command {
        Cmd::List => {
            let root = cache::cache_root_from_env().ok();
            for e in &m.entries {
                let status = match &root {
                    Some(r) if fetch::cached_checkout(e, r).is_some() => "cached",
                    Some(_) => "not-cached",
                    None => "unknown",
                };
                println!(
                    "{}\t{}\t{}\t{}",
                    e.name, e.source.rev, status, e.description
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Fetch {
            names,
            offline,
            force_unlock,
        } => {
            let entries = select(&m, &names)?;
            let mut opts = FetchOptions::new(cache::cache_root_from_env()?);
            opts.offline = offline;
            opts.force_unlock = force_unlock;
            opts.allow_file_protocol = cli.allow_file_protocol;
            let mut failed = 0usize;
            for e in entries {
                match fetch::fetch_entry(e, &opts) {
                    Ok(o) => {
                        let what = match o.status {
                            FetchStatus::Cached => "cached",
                            FetchStatus::Fetched => "fetched",
                        };
                        println!(
                            "{}: {what} {} -> {}",
                            e.name,
                            e.source.rev,
                            o.checkout_dir.display()
                        );
                    }
                    Err(err) => {
                        failed += 1;
                        eprintln!("svsim-corpus: error: {}: {err}", e.name);
                    }
                }
            }
            Ok(if failed == 0 {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Q-18: マニフェストが見つからないのは実行時のエラー (終了コード 1)
    #[test]
    fn missing_manifest_is_runtime_error() {
        match load_manifest_from(None, &[], ManifestOptions::default()) {
            Err(Failure::Runtime(e)) => assert!(e.to_string().contains("no manifest found")),
            Err(Failure::Usage(m)) => panic!("expected runtime error, got usage error: {m}"),
            Ok(_) => panic!("expected an error"),
        }
    }

    // Q-18: 読めないマニフェストも実行時のエラー
    #[test]
    fn unreadable_manifest_is_runtime_error() {
        let r = load_manifest_from(
            None,
            &[PathBuf::from("/nonexistent/manifest.toml")],
            ManifestOptions::default(),
        );
        assert!(matches!(r, Err(Failure::Runtime(Error::Io { .. }))));
    }

    // Q-18: 存在しないエントリ名は使い方の誤り (終了コード 2)
    #[test]
    fn unknown_entry_is_usage_error() {
        let m = Manifest::default();
        assert!(matches!(
            select(&m, &["nope".to_string()]),
            Err(Failure::Usage(_))
        ));
    }
}
