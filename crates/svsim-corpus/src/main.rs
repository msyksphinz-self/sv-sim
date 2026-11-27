//! `svsim-corpus` コマンド (`cargo corpus <subcmd>`)。plan.md §7.5。
//!
//! - `list`: マニフェストのエントリの一覧
//! - `fetch [NAME...] [--offline] [--force-unlock]`: 取得 (名前を省略すると全部)
//!
//! `discover` と `run` はタスク M0-T13 で追加する。
//!
//! 終了コード: 0 = 成功、1 = 実行時のエラー (取得の失敗等)、2 = 使い方の誤り。

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use svsim_corpus::fetch::{self, FetchOptions, FetchStatus};
use svsim_corpus::{Error, Manifest, cache, manifest};

/// sv-sim test corpus manager.
#[derive(Debug, Parser)]
#[command(name = "svsim-corpus", version)]
struct Cli {
    /// Additional manifest file(s), read after the default `corpus/manifest.toml`.
    #[arg(long = "manifest", value_name = "PATH", global = true)]
    manifests: Vec<PathBuf>,

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
        /// Allow `file://` sources (for tests only).
        #[arg(long, hide = true)]
        allow_file_protocol: bool,
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

fn load_manifest(extra: &[PathBuf]) -> Result<Manifest, Failure> {
    let mut paths = Vec::new();
    if let Some(p) = manifest::default_manifest_path() {
        paths.push(p);
    }
    paths.extend(extra.iter().cloned());
    if paths.is_empty() {
        return Err(Failure::Usage(format!(
            "no manifest found ({} was not found; use --manifest)",
            manifest::DEFAULT_MANIFEST_REL
        )));
    }
    Ok(Manifest::load_files(&paths)?)
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
    let m = load_manifest(&cli.manifests)?;
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
            allow_file_protocol,
        } => {
            let entries = select(&m, &names)?;
            let mut opts = FetchOptions::new(cache::cache_root_from_env()?);
            opts.offline = offline;
            opts.force_unlock = force_unlock;
            opts.allow_file_protocol = allow_file_protocol;
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
