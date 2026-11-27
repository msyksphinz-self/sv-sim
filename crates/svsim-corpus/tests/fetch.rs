//! 取得・キャッシュ・ロックの結合テスト (plan.md §7.2、受け入れ基準 K-1, K-5)。
//! ネットワークには出ない。取得元は一時ディレクトリの bare リポジトリ (`file://`)。

mod git_fixture;

use std::fs;
use std::path::Path;

use git_fixture::{Fixture, commit_all, file_url, git, write};
use svsim_corpus::cache::{COMPLETE_MARKER, CacheLayout};
use svsim_corpus::fetch::{FetchOptions, FetchStatus, cached_checkout, fetch_entry};
use svsim_corpus::manifest::ManifestOptions;
use svsim_corpus::{Entry, Error, Manifest};

/// テスト用 (`file://` を許す) の設定でマニフェストを解析し、最初のエントリを返す。
fn parse_entry(text: &str) -> Entry {
    let opts = ManifestOptions {
        allow_file_urls: true,
    };
    Manifest::parse_str_with(text, "test", opts)
        .unwrap()
        .entries
        .remove(0)
}

fn entry(fx: &Fixture, rev: &str, extra_source: &str) -> Entry {
    parse_entry(&fx.manifest_toml("demo", rev, extra_source))
}

fn opts(fx: &Fixture) -> FetchOptions {
    let mut o = FetchOptions::new(fx.cache());
    o.allow_file_protocol = true;
    o
}

/// `<cache>/<name>/` の中身 (ファイル名の一覧、整列済み)。
fn entry_dir_names(fx: &Fixture) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(fx.cache().join("demo"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

fn head_of(dir: &Path) -> String {
    git(dir, &["rev-parse", "HEAD"])
}

// K-1: 先端でないコミットを浅く取得し、rev を検証して <cache>/<name>/<rev>/ に置く。2 回目は何もしない
#[test]
fn fetches_non_tip_commit_and_is_idempotent() {
    let fx = Fixture::new();
    let e = entry(&fx, &fx.rev1, "");
    let o = opts(&fx);

    assert_eq!(cached_checkout(&e, &fx.cache()), None);
    let out = fetch_entry(&e, &o).unwrap();
    assert_eq!(out.status, FetchStatus::Fetched);
    let dir = fx.cache().join("demo").join(&fx.rev1);
    assert_eq!(out.checkout_dir, dir);
    assert_eq!(head_of(&dir), fx.rev1);
    assert!(dir.join("tests/a.sv").is_file());
    assert!(
        !dir.join("tests/b.sv").exists(),
        "rev1 must not contain b.sv"
    );
    assert!(dir.join(COMPLETE_MARKER).is_file());
    assert_eq!(cached_checkout(&e, &fx.cache()), Some(dir.clone()));
    // 浅い取得 (履歴は 1 コミット)
    assert_eq!(git(&dir, &["rev-list", "--count", "HEAD"]), "1");
    // ロックと一時ディレクトリが残っていない
    assert_eq!(entry_dir_names(&fx), vec![fx.rev1.clone()]);

    // 2 回目: 取得元を消してもキャッシュから成功する (ネットワークに出ない)
    fs::remove_dir_all(&fx.bare).unwrap();
    let out2 = fetch_entry(&e, &o).unwrap();
    assert_eq!(out2.status, FetchStatus::Cached);
    assert_eq!(out2.checkout_dir, dir);
    // --offline でも成功する
    let mut off = o.clone();
    off.offline = true;
    assert_eq!(fetch_entry(&e, &off).unwrap().status, FetchStatus::Cached);
}

#[test]
fn fetches_tip_commit() {
    let fx = Fixture::new();
    let e = entry(&fx, &fx.rev2, "");
    let out = fetch_entry(&e, &opts(&fx)).unwrap();
    assert_eq!(out.status, FetchStatus::Fetched);
    assert!(out.checkout_dir.join("tests/b.sv").is_file());
    assert_eq!(head_of(&out.checkout_dir), fx.rev2);
}

#[test]
fn offline_without_cache_is_an_error() {
    let fx = Fixture::new();
    let e = entry(&fx, &fx.rev1, "");
    let mut o = opts(&fx);
    o.offline = true;
    let err = fetch_entry(&e, &o).unwrap_err();
    assert!(matches!(err, Error::NotCached { .. }), "{err}");
    assert!(err.to_string().contains("--offline"), "{err}");
    // 何も作らない
    assert!(!fx.cache().exists());
}

#[test]
fn unknown_rev_fails_and_leaves_nothing_behind() {
    let fx = Fixture::new();
    let e = entry(&fx, "0123456789abcdef0123456789abcdef01234567", "");
    let err = fetch_entry(&e, &opts(&fx)).unwrap_err();
    assert!(matches!(err, Error::Git { .. }), "{err}");
    assert!(err.to_string().contains("git fetch"), "{err}");
    // 一時ディレクトリもロックも残らない
    assert!(
        entry_dir_names(&fx).is_empty(),
        "{:?}",
        entry_dir_names(&fx)
    );
}

#[test]
fn unreachable_source_fails() {
    let fx = Fixture::new();
    let text = fx.manifest_toml("demo", &fx.rev1, "").replace(
        &file_url(&fx.bare),
        &file_url(&fx.tmp.path().join("missing.git")),
    );
    let e = parse_entry(&text);
    let err = fetch_entry(&e, &opts(&fx)).unwrap_err();
    assert!(matches!(err, Error::Git { .. }), "{err}");
    assert!(entry_dir_names(&fx).is_empty());
}

#[test]
fn missing_git_binary_is_reported() {
    let fx = Fixture::new();
    let e = entry(&fx, &fx.rev1, "");
    let mut o = opts(&fx);
    o.git = fx.tmp.path().join("no-such-git");
    let err = fetch_entry(&e, &o).unwrap_err();
    assert!(err.to_string().contains("cannot run"), "{err}");
}

#[test]
fn sparse_checkout_limits_files() {
    let fx = Fixture::new();
    let e = entry(&fx, &fx.rev2, r#"sparse = ["tests/", "LICENSE"]"#);
    let out = fetch_entry(&e, &opts(&fx)).unwrap();
    let d = &out.checkout_dir;
    assert!(d.join("tests/a.sv").is_file());
    assert!(d.join("tests/b.sv").is_file());
    assert!(d.join("LICENSE").is_file());
    assert!(!d.join("other").exists(), "other/ must be excluded");
    assert_eq!(head_of(d), fx.rev2);
}

#[test]
fn submodules_are_fetched_when_listed() {
    let fx = Fixture::new();
    // サブモジュールになるリポジトリ
    let sub_work = fx.tmp.path().join("sub-work");
    fs::create_dir_all(&sub_work).unwrap();
    git(&sub_work, &["init", "-q"]);
    write(&sub_work, "core.sv", "module core; endmodule\n");
    commit_all(&sub_work, "sub");
    git(
        fx.tmp.path(),
        &[
            "clone",
            "-q",
            "--bare",
            sub_work.to_str().unwrap(),
            "sub.git",
        ],
    );
    let sub_url = file_url(&fx.tmp.path().join("sub.git"));
    // 親リポジトリ
    let work = fx.tmp.path().join("parent-work");
    fs::create_dir_all(&work).unwrap();
    git(&work, &["init", "-q"]);
    write(&work, "tests/t.sv", "module t; endmodule\n");
    git(
        &work,
        &["submodule", "add", "-q", &sub_url, "third_party/sub"],
    );
    git(
        &work,
        &["submodule", "add", "-q", &sub_url, "third_party/unused"],
    );
    let rev = commit_all(&work, "parent");
    git(
        fx.tmp.path(),
        &[
            "clone",
            "-q",
            "--bare",
            work.to_str().unwrap(),
            "parent.git",
        ],
    );
    let text = fx
        .manifest_toml("demo", &rev, r#"submodules = ["third_party/sub"]"#)
        .replace(
            &file_url(&fx.bare),
            &file_url(&fx.tmp.path().join("parent.git")),
        );
    let e = parse_entry(&text);
    let out = fetch_entry(&e, &opts(&fx)).unwrap();
    let d = &out.checkout_dir;
    assert!(d.join("tests/t.sv").is_file());
    assert!(d.join("third_party/sub/core.sv").is_file());
    // 指定していないサブモジュールは取得しない
    assert!(!d.join("third_party/unused/core.sv").exists());

    // サブモジュールを指定しなければ取得しない
    let text2 = text.replace(r#"submodules = ["third_party/sub"]"#, "");
    let mut e2 = parse_entry(&text2);
    e2.name = "demo2".into();
    let out2 = fetch_entry(&e2, &opts(&fx)).unwrap();
    assert!(!out2.checkout_dir.join("third_party/sub/core.sv").exists());
}

// K-5: 生きているプロセスのロックがあればエラー (待たない)
#[test]
fn live_lock_blocks_fetch_and_force_unlock_overrides() {
    let fx = Fixture::new();
    let e = entry(&fx, &fx.rev1, "");
    let layout = CacheLayout::new(&fx.cache(), "demo", &fx.rev1);
    fs::create_dir_all(&layout.entry_dir).unwrap();
    // 自分自身の PID (確実に生きている) で、作成時刻を古くしたロック
    let live = format!("pid={}\ncreated_unix=1\n", std::process::id());
    fs::write(&layout.lock_path, &live).unwrap();

    let err = fetch_entry(&e, &opts(&fx)).unwrap_err();
    assert!(matches!(err, Error::Locked { .. }), "{err}");
    assert!(err.to_string().contains("--force-unlock"), "{err}");
    assert_eq!(fs::read_to_string(&layout.lock_path).unwrap(), live);
    assert!(!layout.checkout_dir.exists());

    let mut o = opts(&fx);
    o.force_unlock = true;
    let out = fetch_entry(&e, &o).unwrap();
    assert_eq!(out.status, FetchStatus::Fetched);
    assert!(!layout.lock_path.exists());
}

// K-5: 存在しない PID のロックは自動で取り除かれる
#[cfg(target_os = "linux")]
#[test]
fn stale_lock_is_removed_automatically() {
    let fx = Fixture::new();
    let e = entry(&fx, &fx.rev1, "");
    let layout = CacheLayout::new(&fx.cache(), "demo", &fx.rev1);
    fs::create_dir_all(&layout.entry_dir).unwrap();
    // Linux の pid_max は最大 2^22 なので、この PID のプロセスは存在しない
    fs::write(&layout.lock_path, "pid=4294967294\ncreated_unix=1\n").unwrap();
    let out = fetch_entry(&e, &opts(&fx)).unwrap();
    assert_eq!(out.status, FetchStatus::Fetched);
    assert_eq!(entry_dir_names(&fx), vec![fx.rev1.clone()]);
}

// 異常終了したプロセスの一時ディレクトリと、完了印の無い最終位置は作り直す
#[cfg(target_os = "linux")]
#[test]
fn leftovers_from_crashed_fetch_are_cleaned() {
    let fx = Fixture::new();
    let e = entry(&fx, &fx.rev1, "");
    let layout = CacheLayout::new(&fx.cache(), "demo", &fx.rev1);
    let dead_tmp = layout
        .entry_dir
        .join(format!(".tmp-{}-4294967294", fx.rev1));
    fs::create_dir_all(&dead_tmp).unwrap();
    fs::write(dead_tmp.join("junk"), "x").unwrap();
    fs::create_dir_all(&layout.checkout_dir).unwrap();
    fs::write(layout.checkout_dir.join("partial"), "x").unwrap();

    let out = fetch_entry(&e, &opts(&fx)).unwrap();
    assert_eq!(out.status, FetchStatus::Fetched);
    assert!(!out.checkout_dir.join("partial").exists());
    assert!(out.checkout_dir.join("tests/a.sv").is_file());
    assert_eq!(entry_dir_names(&fx), vec![fx.rev1.clone()]);
}

// 古い形式 (`created=`) のロックも読める。持ち主が生きていればエラー
#[test]
fn legacy_lock_format_is_understood() {
    let fx = Fixture::new();
    let e = entry(&fx, &fx.rev1, "");
    let layout = CacheLayout::new(&fx.cache(), "demo", &fx.rev1);
    fs::create_dir_all(&layout.entry_dir).unwrap();
    fs::write(
        &layout.lock_path,
        format!("pid={}\ncreated=5\n", std::process::id()),
    )
    .unwrap();
    let err = fetch_entry(&e, &opts(&fx)).unwrap_err();
    match &err {
        Error::Locked {
            holder: Some(h), ..
        } => {
            assert_eq!(h.pid, std::process::id());
            assert_eq!(h.created_unix, 5);
        }
        other => panic!("{other}"),
    }
    assert!(err.to_string().contains("unix time 5"), "{err}");
}

// 解析できないロックは自動では取り除かない (Q-20)
#[test]
fn unparsable_lock_blocks_fetch() {
    let fx = Fixture::new();
    let e = entry(&fx, &fx.rev1, "");
    let layout = CacheLayout::new(&fx.cache(), "demo", &fx.rev1);
    fs::create_dir_all(&layout.entry_dir).unwrap();
    fs::write(&layout.lock_path, "pid=?\n").unwrap();
    let err = fetch_entry(&e, &opts(&fx)).unwrap_err();
    assert!(matches!(err, Error::Locked { holder: None, .. }), "{err}");
    assert_eq!(fs::read_to_string(&layout.lock_path).unwrap(), "pid=?\n");
}

// git は file:// を、テスト用のフラグが無ければ使わない (protocol.file.allow=never)
#[test]
fn file_protocol_is_refused_without_flag() {
    let fx = Fixture::new();
    let e = entry(&fx, &fx.rev1, "");
    let mut o = opts(&fx);
    o.allow_file_protocol = false;
    let err = fetch_entry(&e, &o).unwrap_err();
    assert!(matches!(err, Error::Git { .. }), "{err}");
    assert!(err.to_string().contains("not allowed"), "{err}");
    assert!(entry_dir_names(&fx).is_empty());
}

// 親の環境の GIT_CONFIG_PARAMETERS / GIT_ALLOW_PROTOCOL で file:// を許しても効かない
#[test]
fn parent_git_config_env_is_not_inherited() {
    let fx = Fixture::new();
    // 子プロセスに渡す環境を変えるため、取得を別のプロセス (このテスト自身) で行う
    let exe = std::env::current_exe().unwrap();
    let out = std::process::Command::new(exe)
        .args([
            "--exact",
            "parent_env_child",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("SVSIM_TEST_CHILD_URL", file_url(&fx.bare))
        .env("SVSIM_TEST_CHILD_REV", &fx.rev1)
        .env("SVSIM_TEST_CHILD_CACHE", fx.cache())
        .env("GIT_CONFIG_PARAMETERS", "'protocol.file.allow'='always'")
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "protocol.allow")
        .env("GIT_CONFIG_VALUE_0", "always")
        .env("GIT_ALLOW_PROTOCOL", "file")
        .output()
        .unwrap();
    let text =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{text}");
    assert!(text.contains("CHILD-RESULT: refused"), "{text}");
}

/// [`parent_env_child`] から子プロセスとして呼ばれる。単独では何もしない。
#[test]
#[ignore = "helper for parent_git_config_env_is_not_inherited"]
fn parent_env_child() {
    let (Some(url), Some(rev), Some(cache)) = (
        std::env::var_os("SVSIM_TEST_CHILD_URL"),
        std::env::var("SVSIM_TEST_CHILD_REV").ok(),
        std::env::var_os("SVSIM_TEST_CHILD_CACHE"),
    ) else {
        return;
    };
    let text = format!(
        "version = 1\n[[entry]]\nname = \"demo\"\nlicense = \"ISC\"\n[entry.source]\ngit = \"{}\"\nrev = \"{rev}\"\n[entry.discover]\nkind = \"sv-tests\"\nroot = \"tests\"\n",
        url.to_string_lossy()
    );
    let e = parse_entry(&text);
    // allow_file_protocol は false のまま
    let o = FetchOptions::new(cache.into());
    match fetch_entry(&e, &o) {
        Err(Error::Git { message, .. }) if message.contains("not allowed") => {
            println!("CHILD-RESULT: refused")
        }
        other => println!("CHILD-RESULT: unexpected {other:?}"),
    }
}

// 異なる rev は別の場所に置かれ、互いに影響しない
#[test]
fn different_revs_coexist() {
    let fx = Fixture::new();
    let o = opts(&fx);
    let a = fetch_entry(&entry(&fx, &fx.rev1, ""), &o).unwrap();
    let b = fetch_entry(&entry(&fx, &fx.rev2, ""), &o).unwrap();
    assert_ne!(a.checkout_dir, b.checkout_dir);
    assert_eq!(head_of(&a.checkout_dir), fx.rev1);
    assert_eq!(head_of(&b.checkout_dir), fx.rev2);
    let mut names = vec![fx.rev1.clone(), fx.rev2.clone()];
    names.sort();
    assert_eq!(entry_dir_names(&fx), names);
}

/// 実ネットワークで sv-tests を取得する (手動確認用。`cargo test -- --ignored`)。
/// `SVSIM_CORPUS_CACHE` が設定されていなければ何もしない (ホームを汚さないため)。
#[test]
#[ignore = "network access"]
fn fetch_real_sv_tests() {
    let Some(cache) = std::env::var_os("SVSIM_CORPUS_CACHE") else {
        eprintln!("SVSIM_CORPUS_CACHE is not set; skipping");
        return;
    };
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/manifest.toml");
    let m = Manifest::load_file(&p).unwrap();
    let e = m.get("sv-tests").unwrap();
    let out = fetch_entry(e, &FetchOptions::new(cache.into())).unwrap();
    assert_eq!(head_of(&out.checkout_dir), e.source.rev);
}
