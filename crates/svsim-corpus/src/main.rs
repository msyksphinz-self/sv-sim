//! `svsim-corpus` コマンド (`cargo corpus <subcmd>`)。plan.md §7.5。
//!
//! - `list`: マニフェストのエントリの一覧
//! - `fetch [NAME...] [--offline] [--force-unlock]`: 取得 (名前を省略すると全部)
//! - `discover NAME [--json]`: ケースを列挙し、件数とメタデータの統計を表示する (M0-T13)
//! - `run NAME [--out DIR]`: 段階評価を実行し、`results.jsonl` と `summary.json` を書く (M0-T13)
//!
//! `discover` と `run` はネットワークに出ない。取得していなければエラー (終了コード 1) にする。
//! `run` はケースが失敗しても 0 以外にしない (計測用のため。plan.md §7.5)。
//!
//! 終了コード (plan.md §12 Q-18): 0 = 成功、1 = 実行時のエラー (取得の失敗、マニフェストが
//! 見つからない・読めない・誤っている等)、2 = 使い方の誤り (clap の引数エラー、存在しないエントリ名)。

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use svsim_corpus::discover::{self, Discovery};
use svsim_corpus::fetch::{self, FetchOptions, FetchStatus};
use svsim_corpus::manifest::ManifestOptions;
use svsim_corpus::report::{self, Summary};
use svsim_corpus::stage::{Pipeline, RunEnv};
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
    /// Enumerate the test cases of a fetched corpus and show statistics.
    Discover {
        /// Entry name.
        name: String,
        /// Print the statistics and all cases as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Run the staged evaluation and write results.jsonl and summary.json.
    Run {
        /// Entry name.
        name: String,
        /// Output directory (default: target/corpus-results/<NAME>/).
        #[arg(long, value_name = "DIR")]
        out: Option<PathBuf>,
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
        Cmd::Discover { name, json } => {
            let (entry, checkout, disc) = discover_named(&m, &name)?;
            print_diagnostics(&disc);
            let stats = disc.stats();
            if json {
                // チェックアウトの絶対パスは標準出力の JSON に入れない (キャッシュの場所に依らない
                // 出力にするため)。場所は標準エラー出力に出す。
                eprintln!("svsim-corpus: checkout: {}", checkout.display());
                let text = discover_json(entry, &disc)?;
                println!("{text}");
            } else {
                println!(
                    "{}: {} cases ({} should_fail, {} warnings, {} errors)",
                    entry.name, stats.total, stats.should_fail, stats.warnings, stats.errors
                );
                println!("  by target:");
                for (s, n) in &stats.by_target {
                    println!("    {s}: {n}");
                }
                println!("  by :type:");
                for (t, n) in &stats.by_type {
                    println!("    {t}: {n}");
                }
                println!("  metadata keys:");
                for (k, n) in &stats.keys {
                    println!("    {k}: {n}");
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Run { name, out } => {
            let (entry, checkout, disc) = discover_named(&m, &name)?;
            print_diagnostics(&disc);
            // M0 には runner が無いので、全ケースが Preprocess: NotImplemented になる
            let pipeline = Pipeline::new();
            let env = RunEnv {
                checkout_dir: checkout,
            };
            let results = pipeline.run_all(&disc.cases, &env);
            let summary = Summary::new(&entry.name, &entry.source.rev, &results);
            let out = out.unwrap_or_else(|| {
                report::default_out_dir(manifest::default_manifest_path().as_deref(), &entry.name)
            });
            report::write_results(&out, &summary, &results)?;
            for line in summary.lines() {
                println!("{line}");
            }
            println!("results: {}", out.display());
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

/// 名前のエントリを、取得済みのチェックアウトから列挙する (ネットワークに出ない)。
fn discover_named<'m>(
    m: &'m Manifest,
    name: &str,
) -> Result<(&'m manifest::Entry, PathBuf, Discovery), Failure> {
    let entry = m
        .get(name)
        .ok_or_else(|| Failure::from(Error::UnknownEntry(name.to_string())))?;
    let root = cache::cache_root_from_env()?;
    let checkout = fetch::cached_checkout(entry, &root).ok_or_else(|| {
        Failure::Runtime(Error::NotFetched {
            name: entry.name.clone(),
            path: cache::CacheLayout::new(&root, &entry.name, &entry.source.rev).checkout_dir,
        })
    })?;
    let disc = discover::discover_entry(entry, &checkout)?;
    Ok((entry, checkout, disc))
}

/// `discover --json` の出力。チェックアウトの場所 (絶対パス) は含めない。
/// ケースのパスはチェックアウトからの相対パスなので、出力はキャッシュの場所に依らない。
fn discover_json(entry: &manifest::Entry, disc: &Discovery) -> Result<String, Failure> {
    let v = serde_json::json!({
        "entry": entry.name,
        "rev": entry.source.rev,
        "stats": disc.stats(),
        "warnings": disc.warnings,
        "errors": disc.errors,
        "cases": disc.cases,
    });
    serde_json::to_string_pretty(&v).map_err(|e| Failure::Runtime(Error::Other(e.to_string())))
}

/// 列挙の警告とエラーを標準エラー出力に出す。
fn print_diagnostics(d: &Discovery) {
    for w in &d.warnings {
        eprintln!("svsim-corpus: warning: {}: {}", w.path.display(), w.message);
    }
    for e in &d.errors {
        eprintln!("svsim-corpus: error: {}: {}", e.path.display(), e.message);
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
