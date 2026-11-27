//! CLI の `list` / `fetch` のテスト (plan.md §7.5)。ネットワークには出ない。
//!
//! 既定のマニフェスト (`corpus/manifest.toml`) も読み込まれるので、`fetch` には必ず名前を指定する。

mod git_fixture;

use std::path::Path;
use std::process::{Command, Output};

use git_fixture::Fixture;

const SV_TESTS_REV: &str = "7274242bcf99a2f6f8282493d0200c92eb87d347";

fn corpus(cwd: &Path, cache: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_svsim-corpus"))
        .current_dir(cwd)
        .env("SVSIM_CORPUS_CACHE", cache)
        .args(args)
        .output()
        .expect("run svsim-corpus")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn write_manifest(fx: &Fixture, text: &str) -> String {
    let p = fx.tmp.path().join("extra.toml");
    std::fs::write(&p, text).unwrap();
    p.to_str().unwrap().to_string()
}

#[test]
fn list_shows_default_and_extra_entries() {
    let fx = Fixture::new();
    let m = write_manifest(&fx, &fx.manifest_toml("demo", &fx.rev1, ""));
    let o = corpus(fx.tmp.path(), &fx.cache(), &["list", "--manifest", &m]);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let out = stdout(&o);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 2, "{out}");
    assert!(
        lines[0].starts_with(&format!("sv-tests\t{SV_TESTS_REV}\tnot-cached\t")),
        "{out}"
    );
    assert_eq!(
        lines[1],
        format!("demo\t{}\tnot-cached\ttest corpus", fx.rev1)
    );
}

#[test]
fn fetch_then_offline_via_cli() {
    let fx = Fixture::new();
    let m = write_manifest(&fx, &fx.manifest_toml("demo", &fx.rev1, ""));
    let cwd = fx.tmp.path();
    let cache = fx.cache();

    // --offline でキャッシュに無ければ終了コード 1
    let o = corpus(
        cwd,
        &cache,
        &["fetch", "demo", "--offline", "--manifest", &m],
    );
    assert_eq!(o.status.code(), Some(1), "{}", stdout(&o));
    assert!(stderr(&o).contains("not in the cache"), "{}", stderr(&o));

    let o = corpus(
        cwd,
        &cache,
        &["--manifest", &m, "fetch", "demo", "--allow-file-protocol"],
    );
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let dir = cache.join("demo").join(&fx.rev1);
    assert_eq!(
        stdout(&o).trim_end(),
        format!("demo: fetched {} -> {}", fx.rev1, dir.display())
    );

    let o = corpus(
        cwd,
        &cache,
        &["fetch", "demo", "--offline", "--manifest", &m],
    );
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert!(stdout(&o).starts_with("demo: cached "), "{}", stdout(&o));

    let o = corpus(cwd, &cache, &["list", "--manifest", &m]);
    assert!(
        stdout(&o).contains(&format!("demo\t{}\tcached\t", fx.rev1)),
        "{}",
        stdout(&o)
    );
}

#[test]
fn unknown_entry_is_usage_error() {
    let fx = Fixture::new();
    let o = corpus(fx.tmp.path(), &fx.cache(), &["fetch", "no-such-entry"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("no-such-entry"), "{}", stderr(&o));
}

#[test]
fn missing_subcommand_is_usage_error() {
    let fx = Fixture::new();
    let o = corpus(fx.tmp.path(), &fx.cache(), &[]);
    assert_eq!(o.status.code(), Some(2));
    let o = corpus(fx.tmp.path(), &fx.cache(), &["frobnicate"]);
    assert_eq!(o.status.code(), Some(2));
}

// K-4: マニフェストの誤りは実行時のエラー (終了コード 1) として報告される
#[test]
fn bad_manifests_are_reported() {
    let fx = Fixture::new();
    let good = fx.manifest_toml("demo", &fx.rev1, "");
    let cases = [
        (good.replace(&fx.rev1, "main"), "commit id"),
        (
            good.replace("license = \"ISC\"", "license = \"ISC\"\nbogus = 1"),
            "unknown field",
        ),
        (
            good.replace("name = \"demo\"", "name = \"sv-tests\""),
            "duplicate entry name",
        ),
    ];
    for (text, needle) in cases {
        let m = write_manifest(&fx, &text);
        let o = corpus(fx.tmp.path(), &fx.cache(), &["list", "--manifest", &m]);
        assert_eq!(o.status.code(), Some(1), "{text}");
        assert!(stderr(&o).contains(needle), "{needle}: {}", stderr(&o));
    }
    let o = corpus(
        fx.tmp.path(),
        &fx.cache(),
        &["list", "--manifest", "/nonexistent/manifest.toml"],
    );
    assert_eq!(o.status.code(), Some(1));
}

// K-5: 生きているプロセスのロックではエラーになり、--force-unlock で取り直せる
#[test]
fn locked_entry_via_cli() {
    let fx = Fixture::new();
    let m = write_manifest(&fx, &fx.manifest_toml("demo", &fx.rev1, ""));
    let lock_dir = fx.cache().join("demo");
    std::fs::create_dir_all(&lock_dir).unwrap();
    // このテストのプロセス (生きている) の PID
    std::fs::write(
        lock_dir.join(".lock"),
        format!("pid={}\ncreated=1\n", std::process::id()),
    )
    .unwrap();
    let args = ["--manifest", &m, "fetch", "demo", "--allow-file-protocol"];
    let o = corpus(fx.tmp.path(), &fx.cache(), &args);
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("--force-unlock"), "{}", stderr(&o));
    let mut forced = args.to_vec();
    forced.push("--force-unlock");
    let o = corpus(fx.tmp.path(), &fx.cache(), &forced);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
}
