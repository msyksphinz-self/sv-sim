//! CLI の `discover` / `run` のテスト (plan.md §7.3〜§7.5、TC74-2、TC76、TC77、TC80、TC81)。
//! ネットワークには出ない (一時ディレクトリの bare リポジトリを `file://` で取得する)。

mod git_fixture;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use git_fixture::{commit_all, file_url, git, write};

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

/// sv-tests 風の小さなリポジトリ。実行されると印を作るスクリプトも入れる (TC74-2)。
struct Repo {
    tmp: tempfile::TempDir,
    manifest: String,
    marker: PathBuf,
}

impl Repo {
    fn new() -> Repo {
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path().join("work");
        std::fs::create_dir_all(&work).unwrap();
        git(&work, &["init", "-q"]);
        let marker = tmp.path().join("EXECUTED");
        let script = format!("#!/bin/sh\ntouch '{}'\n", marker.display());
        write(&work, "run.sh", &script);
        write(&work, "conf/generators/x.py", &script);
        write(
            &work,
            "Makefile",
            &format!("all:\n\ttouch '{}'\n", marker.display()),
        );
        #[cfg(unix)]
        for f in ["run.sh", "conf/generators/x.py"] {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(work.join(f), std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        write(
            &work,
            "tests/a.sv",
            ":name: a\n:tags: 5.6\nmodule a; endmodule\n",
        );
        write(
            &work,
            "tests/sub/bad.SV",
            ":name: bad\r\n:type: simulation elaboration parsing\r\n:should_fail_because: x\r\n",
        );
        write(&work, "tests/B.sv", ":type: preprocessing\n:foo_bar: 1\n");
        write(&work, "tests/inc.svh", "");
        let rev = commit_all(&work, "one");
        git(
            tmp.path(),
            &["clone", "-q", "--bare", work.to_str().unwrap(), "src.git"],
        );
        let manifest = tmp.path().join("extra.toml");
        std::fs::write(
            &manifest,
            format!(
                "version = 1\n[[entry]]\nname = \"demo\"\nlicense = \"ISC\"\n[entry.source]\ngit = \"{}\"\nrev = \"{rev}\"\n[entry.discover]\nkind = \"sv-tests\"\nroot = \"tests\"\n",
                file_url(&tmp.path().join("src.git"))
            ),
        )
        .unwrap();
        Repo {
            manifest: manifest.to_str().unwrap().to_string(),
            tmp,
            marker,
        }
    }

    fn cache(&self) -> PathBuf {
        self.tmp.path().join("cache")
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_with_cache(&self.cache(), args)
    }

    fn run_with_cache(&self, cache: &Path, args: &[&str]) -> Output {
        let mut a: Vec<&str> = args.to_vec();
        a.extend(["--manifest", &self.manifest, "--allow-file-protocol"]);
        corpus(self.tmp.path(), cache, &a)
    }
}

#[test]
fn discover_and_run_end_to_end() {
    let r = Repo::new();

    // 取得前の discover / run は実行時のエラー (1) で、fetch を案内する
    for cmd in ["discover", "run"] {
        let o = r.run(&[cmd, "demo"]);
        assert_eq!(o.status.code(), Some(1), "{cmd}: {}", stderr(&o));
        assert!(
            stderr(&o).contains("svsim-corpus fetch demo"),
            "{}",
            stderr(&o)
        );
    }

    let o = r.run(&["fetch", "demo"]);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));

    // discover (人間向け)
    let o = r.run(&["discover", "demo"]);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let out = stdout(&o);
    assert!(
        out.contains("demo: 3 cases (1 should_fail, 1 warnings, 0 errors)"),
        "{out}"
    );
    assert!(out.contains("    Simulate: 1"), "{out}");
    // 未知のキーは警告 (標準エラー出力)
    assert!(stderr(&o).contains("foo_bar"), "{}", stderr(&o));

    // discover --json
    let o = r.run(&["discover", "demo", "--json"]);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["stats"]["total"], 3);
    assert_eq!(v["stats"]["should_fail"], 1);
    assert_eq!(v["stats"]["by_target"]["Preprocess"], 1);
    assert_eq!(v["stats"]["by_target"]["Elaborate"], 1);
    assert_eq!(v["stats"]["by_target"]["Simulate"], 1);
    let ids: Vec<&str> = v["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["demo/B.sv", "demo/a.sv", "demo/sub/bad.SV"]);
    let bad = &v["cases"][2];
    assert_eq!(bad["files"][0], "tests/sub/bad.SV");
    assert_eq!(bad["incdirs"][0], "tests/sub");
    assert_eq!(bad["expect"]["should_fail"], true);
    assert_eq!(bad["expect"]["reason"], "x");
    assert_eq!(bad["expect"]["target"], "Simulate");
    assert_eq!(bad["meta"]["type"], "simulation elaboration parsing");

    // discover --json の出力はキャッシュの場所に依らない (絶対パスの checkout を含めない)
    assert!(v.get("checkout").is_none(), "{v}");
    let cache2 = r.tmp.path().join("another-cache");
    let o = r.run_with_cache(&cache2, &["fetch", "demo"]);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let o2 = r.run_with_cache(&cache2, &["discover", "demo", "--json"]);
    assert_eq!(o2.status.code(), Some(0), "{}", stderr(&o2));
    let json1 = r.run(&["discover", "demo", "--json"]).stdout;
    assert_eq!(json1, o2.stdout);
    let json1 = String::from_utf8(json1).unwrap();
    for c in [r.cache(), cache2.clone()] {
        assert!(!json1.contains(c.to_str().unwrap()), "{json1}");
    }
    // 場所は標準エラー出力に出す
    assert!(
        stderr(&o2).contains(cache2.to_str().unwrap()),
        "{}",
        stderr(&o2)
    );

    // run を 2 回。所要時間以外は同じ
    let out1 = r.tmp.path().join("r1");
    let out2 = r.tmp.path().join("r2");
    for out in [&out1, &out2] {
        let o = r.run(&["run", "demo", "--out", out.to_str().unwrap()]);
        assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
        let s = stdout(&o);
        assert!(s.contains("demo: 3 cases (1 should_fail)"), "{s}");
        assert!(s.contains("Preprocess: NotImplemented 3"), "{s}");
        assert!(s.contains("Undetermined 3"), "{s}");
    }
    let strip = |dir: &Path| -> Vec<String> {
        std::fs::read_to_string(dir.join("results.jsonl"))
            .unwrap()
            .lines()
            .map(|l| {
                let mut v: serde_json::Value = serde_json::from_str(l).unwrap();
                assert!(v["duration_us"].is_u64());
                v.as_object_mut().unwrap().remove("duration_us");
                v.to_string()
            })
            .collect()
    };
    let l1 = strip(&out1);
    assert_eq!(l1, strip(&out2));
    assert_eq!(l1.len(), 3);
    for l in &l1 {
        let v: serde_json::Value = serde_json::from_str(l).unwrap();
        assert_eq!(
            v["stages"],
            serde_json::json!([{"stage": "Preprocess", "outcome": "NotImplemented"}])
        );
        assert_eq!(v["verdict"], "Undetermined");
    }
    let s1 = std::fs::read(out1.join("summary.json")).unwrap();
    assert_eq!(s1, std::fs::read(out2.join("summary.json")).unwrap());
    let s: serde_json::Value = serde_json::from_slice(&s1).unwrap();
    assert_eq!(s["total"], 3);
    assert_eq!(s["stages"]["Preprocess"]["NotImplemented"], 3);
    assert_eq!(s["stages"]["Parse"]["NotImplemented"], 0);
    assert_eq!(s["verdicts"]["Undetermined"], 3);

    // TC74-2: 取得したリポジトリのスクリプトは実行しない
    assert!(!r.marker.exists());
}

// TC81: 使い方の誤りは 2
#[test]
fn usage_errors() {
    let r = Repo::new();
    for args in [
        vec!["discover"],
        vec!["run"],
        vec!["discover", "no-such-entry"],
        vec!["run", "no-such-entry"],
        vec!["run", "demo", "--bogus"],
    ] {
        let o = r.run(&args);
        assert_eq!(o.status.code(), Some(2), "{args:?}: {}", stderr(&o));
    }
}
