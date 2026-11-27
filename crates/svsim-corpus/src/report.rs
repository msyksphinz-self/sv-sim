//! 段階評価の結果の出力 (plan.md §7.4、タスク M0-T13)。
//!
//! - `<out>/results.jsonl`: 1 行に 1 ケース ([`ResultLine`])。`id` のバイト順。
//!   所要時間 (`duration_us`) は各行の最後のフィールドに置く。
//! - `<out>/summary.json`: 段階 × 結果の件数と判定の件数 ([`Summary`])。
//!
//! 所要時間以外は決定的にする (同じ入力なら、所要時間を除いてバイト単位で同じ)。
//! 既定の `<out>` は `target/corpus-results/<entry>/` ([`default_out_dir`])。

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::Error;
use crate::stage::{CaseResult, OutcomeKind, Stage, StageRecord, Verdict};

/// `results.jsonl` のファイル名。
pub const RESULTS_FILE: &str = "results.jsonl";

/// `summary.json` のファイル名。
pub const SUMMARY_FILE: &str = "summary.json";

/// `results.jsonl` の 1 行。
#[derive(Debug, Serialize)]
pub struct ResultLine<'a> {
    /// ケースの id。
    pub id: &'a str,
    /// 失敗するのが正しいケースか。
    pub should_fail: bool,
    /// 評価する最後の段階。
    pub target: Stage,
    /// 段階ごとの結果。
    pub stages: &'a [StageRecord],
    /// 判定。
    pub verdict: Verdict,
    /// 所要時間 (マイクロ秒)。決定的でない唯一のフィールド。
    pub duration_us: u64,
}

impl<'a> ResultLine<'a> {
    /// [`CaseResult`] から作る。
    pub fn new(r: &'a CaseResult) -> ResultLine<'a> {
        ResultLine {
            id: &r.id,
            should_fail: r.expect.should_fail,
            target: r.expect.target,
            stages: &r.stages,
            verdict: r.verdict,
            duration_us: u64::try_from(r.duration.as_micros()).unwrap_or(u64::MAX),
        }
    }
}

/// `summary.json` の中身。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Summary {
    /// エントリ名。
    pub entry: String,
    /// 取得したコミット。
    pub rev: String,
    /// ケース数。
    pub total: usize,
    /// `should_fail` のケース数。
    pub should_fail: usize,
    /// 段階 × 結果の種類の件数 (その段階で、その結果になったケースの数)。全組を 0 も含めて出す。
    pub stages: BTreeMap<Stage, BTreeMap<OutcomeKind, usize>>,
    /// 判定ごとの件数。全種類を 0 も含めて出す。
    pub verdicts: BTreeMap<Verdict, usize>,
}

impl Summary {
    /// 結果を集計する。
    pub fn new(entry: &str, rev: &str, results: &[CaseResult]) -> Summary {
        let mut stages: BTreeMap<Stage, BTreeMap<OutcomeKind, usize>> = Stage::ALL
            .into_iter()
            .map(|s| (s, OutcomeKind::ALL.into_iter().map(|k| (k, 0)).collect()))
            .collect();
        let mut verdicts: BTreeMap<Verdict, usize> =
            Verdict::ALL.into_iter().map(|v| (v, 0)).collect();
        for r in results {
            for s in &r.stages {
                *stages
                    .entry(s.stage)
                    .or_default()
                    .entry(s.outcome.kind())
                    .or_default() += 1;
            }
            *verdicts.entry(r.verdict).or_default() += 1;
        }
        Summary {
            entry: entry.to_string(),
            rev: rev.to_string(),
            total: results.len(),
            should_fail: results.iter().filter(|r| r.expect.should_fail).count(),
            stages,
            verdicts,
        }
    }

    /// 0 でない件数を、人間向けに 1 行ずつ返す (標準出力用)。
    pub fn lines(&self) -> Vec<String> {
        let mut out = vec![format!(
            "{}: {} cases ({} should_fail)",
            self.entry, self.total, self.should_fail
        )];
        for (stage, kinds) in &self.stages {
            let parts: Vec<String> = kinds
                .iter()
                .filter(|(_, n)| **n > 0)
                .map(|(k, n)| format!("{k:?} {n}"))
                .collect();
            if !parts.is_empty() {
                out.push(format!("  {stage}: {}", parts.join(", ")));
            }
        }
        let v: Vec<String> = self
            .verdicts
            .iter()
            .map(|(k, n)| format!("{k:?} {n}"))
            .collect();
        out.push(format!("  verdict: {}", v.join(", ")));
        out
    }
}

/// 既定の出力先 `<target>/corpus-results/<entry>/`。
///
/// `<target>` は、既定のマニフェスト (`<repo>/corpus/manifest.toml`) が見つかれば `<repo>/target`、
/// 見つからなければカレントディレクトリの `target`。
pub fn default_out_dir(default_manifest: Option<&Path>, entry: &str) -> PathBuf {
    let base = default_manifest
        .and_then(|m| m.parent())
        .and_then(|corpus| corpus.parent())
        .map_or_else(|| PathBuf::from("target"), |repo| repo.join("target"));
    base.join("corpus-results").join(entry)
}

/// `out` に `results.jsonl` と `summary.json` を書く (ディレクトリは作る)。
pub fn write_results(out: &Path, summary: &Summary, results: &[CaseResult]) -> Result<(), Error> {
    fs::create_dir_all(out).map_err(|e| Error::io(out, e))?;

    let p = out.join(RESULTS_FILE);
    let f = fs::File::create(&p).map_err(|e| Error::io(&p, e))?;
    let mut w = BufWriter::new(f);
    for r in results {
        serde_json::to_writer(&mut w, &ResultLine::new(r))
            .map_err(|e| Error::Other(format!("{}: {e}", p.display())))?;
        w.write_all(b"\n").map_err(|e| Error::io(&p, e))?;
    }
    w.flush().map_err(|e| Error::io(&p, e))?;

    let p = out.join(SUMMARY_FILE);
    let mut text = serde_json::to_string_pretty(summary)
        .map_err(|e| Error::Other(format!("{}: {e}", p.display())))?;
    text.push('\n');
    fs::write(&p, text).map_err(|e| Error::io(&p, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discover::Expectation;
    use crate::stage::StageOutcome;
    use std::time::Duration;

    fn result(id: &str, should_fail: bool, us: u64) -> CaseResult {
        CaseResult {
            id: id.into(),
            expect: Expectation {
                should_fail,
                reason: None,
                target: Stage::Elaborate,
            },
            stages: vec![StageRecord {
                stage: Stage::Preprocess,
                outcome: StageOutcome::NotImplemented,
            }],
            verdict: Verdict::Undetermined,
            duration: Duration::from_micros(us),
        }
    }

    #[test]
    fn summary_counts_all_pairs() {
        let rs = vec![result("e/a", false, 1), result("e/b", true, 2)];
        let s = Summary::new("e", "r", &rs);
        assert_eq!(s.total, 2);
        assert_eq!(s.should_fail, 1);
        assert_eq!(s.stages.len(), 5);
        for (stage, kinds) in &s.stages {
            assert_eq!(kinds.len(), 5);
            for (k, n) in kinds {
                let want =
                    usize::from(*stage == Stage::Preprocess && *k == OutcomeKind::NotImplemented)
                        * 2;
                assert_eq!(*n, want, "{stage:?} {k:?}");
            }
        }
        assert_eq!(s.verdicts[&Verdict::Undetermined], 2);
        assert_eq!(s.verdicts[&Verdict::AsExpected], 0);
        let lines = s.lines();
        assert_eq!(lines[0], "e: 2 cases (1 should_fail)");
        assert_eq!(lines[1], "  Preprocess: NotImplemented 2");
        assert_eq!(
            lines[2],
            "  verdict: AsExpected 0, Unexpected 0, Undetermined 2"
        );
    }

    #[test]
    fn result_line_shape() {
        let r = result("e/a", true, 7);
        assert_eq!(
            serde_json::to_string(&ResultLine::new(&r)).unwrap(),
            r#"{"id":"e/a","should_fail":true,"target":"Elaborate","stages":[{"stage":"Preprocess","outcome":"NotImplemented"}],"verdict":"Undetermined","duration_us":7}"#
        );
    }

    // TC80 (5): 所要時間以外は決定的
    #[test]
    fn output_is_deterministic_except_duration() {
        let d = tempfile::tempdir().unwrap();
        let run = |dir: &Path, us: u64| {
            let rs = vec![result("e/a", false, us), result("e/b", true, us + 1)];
            write_results(dir, &Summary::new("e", "r", &rs), &rs).unwrap();
        };
        let a = d.path().join("a");
        let b = d.path().join("b/c");
        run(&a, 1);
        run(&b, 1000);
        let strip = |p: &Path| {
            fs::read_to_string(p.join(RESULTS_FILE))
                .unwrap()
                .lines()
                .map(|l| l.split(",\"duration_us\":").next().unwrap().to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(strip(&a), strip(&b));
        assert_eq!(strip(&a).len(), 2);
        assert_eq!(
            fs::read(a.join(SUMMARY_FILE)).unwrap(),
            fs::read(b.join(SUMMARY_FILE)).unwrap()
        );
        let v: serde_json::Value =
            serde_json::from_slice(&fs::read(a.join(SUMMARY_FILE)).unwrap()).unwrap();
        assert_eq!(v["stages"]["Preprocess"]["NotImplemented"], 2);
        assert_eq!(v["verdicts"]["Undetermined"], 2);
    }

    #[test]
    fn default_out_dir_uses_repo_target() {
        assert_eq!(
            default_out_dir(Some(Path::new("/r/corpus/manifest.toml")), "sv-tests"),
            PathBuf::from("/r/target/corpus-results/sv-tests")
        );
        assert_eq!(
            default_out_dir(None, "x"),
            PathBuf::from("target/corpus-results/x")
        );
    }
}
