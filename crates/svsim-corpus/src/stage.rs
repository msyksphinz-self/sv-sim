//! 段階評価の枠組み (plan.md §7.4、タスク M0-T13)。
//!
//! - テストケースごとに、[`Stage::Preprocess`] から [`Expectation::target`] まで順に
//!   [`StageRunner`] を実行し、[`StageOutcome::Pass`] 以外が出たらそこで止める。
//! - runner が登録されていない段階は [`StageOutcome::NotImplemented`] として記録して止める。
//!   M0 では runner が 1 つも無いので、全ケースが `Preprocess: NotImplemented` になる。
//! - 生の結果と、期待どおりかの判定 ([`Verdict`]、plan.md §12 Q-21) の両方を記録する。
//!
//! # 判定の規則 ([`judge`])
//!
//! - `target` までのどこかで `Fail` → `should_fail` なら [`Verdict::AsExpected`]、そうでなければ [`Verdict::Unexpected`]。
//! - `target` まで全部 `Pass` → `should_fail` なら `Unexpected`、そうでなければ `AsExpected`。
//! - それ以外 (`NotImplemented` / `Skipped` / `Timeout` で止まった) → [`Verdict::Undetermined`]。
//!
//! この規則は M1 で runner が入ったときに見直す (plan.md §7.4)。

use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::discover::{Expectation, TestCase};

/// 評価の段階。順序は `Preprocess < Parse < Elaborate < Simulate < Compare`。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize)]
pub enum Stage {
    /// プリプロセス (IEEE 1800-2023 §22)。
    Preprocess,
    /// 構文解析。
    Parse,
    /// エラボレーション (§3.12、§23)。
    Elaborate,
    /// シミュレーション (§4)。
    Simulate,
    /// 期待値との比較 (参照シミュレータの出力などと)。
    Compare,
}

impl Stage {
    /// 全段階 (順序どおり)。
    pub const ALL: [Stage; 5] = [
        Stage::Preprocess,
        Stage::Parse,
        Stage::Elaborate,
        Stage::Simulate,
        Stage::Compare,
    ];

    /// 表示用の名前 (`Preprocess` 等。JSON の表記と同じ)。
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Preprocess => "Preprocess",
            Stage::Parse => "Parse",
            Stage::Elaborate => "Elaborate",
            Stage::Simulate => "Simulate",
            Stage::Compare => "Compare",
        }
    }
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 1 段階の結果。JSON では `{"outcome": "<種類>", ...}` の形にする。
#[derive(Clone, PartialEq, Eq, Debug, Serialize)]
#[serde(tag = "outcome")]
pub enum StageOutcome {
    /// 成功。
    Pass,
    /// 失敗。
    Fail {
        /// 失敗の分類 (診断のコードなど。集計に使う)。
        category: String,
        /// 内容。
        message: String,
    },
    /// その段階の runner がまだ無い。
    NotImplemented,
    /// 実行しなかった。
    Skipped {
        /// 理由。
        reason: String,
    },
    /// 時間切れ。
    Timeout,
}

impl StageOutcome {
    /// 結果の種類。
    pub fn kind(&self) -> OutcomeKind {
        match self {
            StageOutcome::Pass => OutcomeKind::Pass,
            StageOutcome::Fail { .. } => OutcomeKind::Fail,
            StageOutcome::NotImplemented => OutcomeKind::NotImplemented,
            StageOutcome::Skipped { .. } => OutcomeKind::Skipped,
            StageOutcome::Timeout => OutcomeKind::Timeout,
        }
    }
}

/// [`StageOutcome`] の種類 (集計用)。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize)]
pub enum OutcomeKind {
    /// [`StageOutcome::Pass`]。
    Pass,
    /// [`StageOutcome::Fail`]。
    Fail,
    /// [`StageOutcome::NotImplemented`]。
    NotImplemented,
    /// [`StageOutcome::Skipped`]。
    Skipped,
    /// [`StageOutcome::Timeout`]。
    Timeout,
}

impl OutcomeKind {
    /// 全種類 (順序どおり)。
    pub const ALL: [OutcomeKind; 5] = [
        OutcomeKind::Pass,
        OutcomeKind::Fail,
        OutcomeKind::NotImplemented,
        OutcomeKind::Skipped,
        OutcomeKind::Timeout,
    ];
}

/// 期待どおりかの判定 (plan.md §12 Q-21)。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize)]
pub enum Verdict {
    /// 期待どおり。
    AsExpected,
    /// 期待と違う。
    Unexpected,
    /// 判定できない (runner が無い、飛ばした、時間切れ)。
    Undetermined,
}

impl Verdict {
    /// 全種類 (順序どおり)。
    pub const ALL: [Verdict; 3] = [
        Verdict::AsExpected,
        Verdict::Unexpected,
        Verdict::Undetermined,
    ];
}

/// runner に渡す実行環境。
#[derive(Clone, Debug)]
pub struct RunEnv {
    /// 取得したリポジトリのチェックアウトの場所。[`TestCase`] のパスはここからの相対。
    pub checkout_dir: PathBuf,
}

/// 1 段階を実行するもの。M1 で Preprocess と Parse の runner を登録する。
pub trait StageRunner: Send + Sync {
    /// 担当する段階。
    fn stage(&self) -> Stage;
    /// ケースを実行する。
    fn run(&self, case: &TestCase, env: &RunEnv) -> StageOutcome;
}

/// 段階と結果の組 (JSON では `{"stage": "...", "outcome": "...", ...}`)。
#[derive(Clone, PartialEq, Eq, Debug, Serialize)]
pub struct StageRecord {
    /// 段階。
    pub stage: Stage,
    /// 結果。
    #[serde(flatten)]
    pub outcome: StageOutcome,
}

/// 1 ケースの評価結果。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CaseResult {
    /// ケースの id ([`TestCase::id`])。
    pub id: String,
    /// 期待。
    pub expect: Expectation,
    /// 実行した段階の結果 (順序どおり。最後の 1 つ以外は `Pass`)。
    pub stages: Vec<StageRecord>,
    /// 判定。
    pub verdict: Verdict,
    /// 所要時間 (決定的でない唯一の値)。
    pub duration: Duration,
}

/// 段階ごとの runner の並び。
#[derive(Default)]
pub struct Pipeline {
    runners: Vec<Box<dyn StageRunner>>,
}

impl fmt::Debug for Pipeline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pipeline")
            .field(
                "stages",
                &self.runners.iter().map(|r| r.stage()).collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl Pipeline {
    /// runner の無い (M0 の) パイプライン。
    pub fn new() -> Pipeline {
        Pipeline::default()
    }

    /// runner を登録する。同じ段階の runner が既にあれば置き換える。
    pub fn register(&mut self, runner: Box<dyn StageRunner>) {
        let s = runner.stage();
        self.runners.retain(|r| r.stage() != s);
        self.runners.push(runner);
    }

    fn runner(&self, stage: Stage) -> Option<&dyn StageRunner> {
        self.runners
            .iter()
            .find(|r| r.stage() == stage)
            .map(|r| r.as_ref())
    }

    /// 1 ケースを評価する。
    pub fn run_case(&self, case: &TestCase, env: &RunEnv) -> CaseResult {
        let start = Instant::now();
        let mut stages = Vec::new();
        for stage in Stage::ALL
            .into_iter()
            .take_while(|s| *s <= case.expect.target)
        {
            let outcome = match self.runner(stage) {
                Some(r) => r.run(case, env),
                None => StageOutcome::NotImplemented,
            };
            let pass = outcome == StageOutcome::Pass;
            stages.push(StageRecord { stage, outcome });
            if !pass {
                break;
            }
        }
        let verdict = judge(&case.expect, &stages);
        CaseResult {
            id: case.id.clone(),
            expect: case.expect.clone(),
            stages,
            verdict,
            duration: start.elapsed(),
        }
    }

    /// 全ケースを順に評価する (逐次。`--jobs` は M1)。
    pub fn run_all(&self, cases: &[TestCase], env: &RunEnv) -> Vec<CaseResult> {
        cases.iter().map(|c| self.run_case(c, env)).collect()
    }
}

/// 判定の規則 (モジュールの doc を参照)。
pub fn judge(expect: &Expectation, stages: &[StageRecord]) -> Verdict {
    let failed = stages.iter().any(|r| r.outcome.kind() == OutcomeKind::Fail);
    let reached = stages
        .last()
        .is_some_and(|r| r.stage == expect.target && r.outcome == StageOutcome::Pass)
        && stages.iter().all(|r| r.outcome == StageOutcome::Pass);
    match (failed, reached, expect.should_fail) {
        (true, _, true) | (false, true, false) => Verdict::AsExpected,
        (true, _, false) | (false, true, true) => Verdict::Unexpected,
        (false, false, _) => Verdict::Undetermined,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    fn case(target: Stage, should_fail: bool) -> TestCase {
        TestCase {
            id: "t/a.sv".into(),
            files: vec![PathBuf::from("tests/a.sv")],
            incdirs: vec![PathBuf::from("tests")],
            defines: vec![],
            top: None,
            expect: Expectation {
                should_fail,
                reason: None,
                target,
            },
            meta: BTreeMap::new(),
        }
    }

    fn env() -> RunEnv {
        RunEnv {
            checkout_dir: PathBuf::from("/nonexistent"),
        }
    }

    /// 決まった結果を返し、呼ばれた段階を記録する runner。
    struct Fixed {
        stage: Stage,
        outcome: StageOutcome,
        calls: &'static Mutex<Vec<Stage>>,
    }

    impl StageRunner for Fixed {
        fn stage(&self) -> Stage {
            self.stage
        }
        fn run(&self, _: &TestCase, _: &RunEnv) -> StageOutcome {
            self.calls.lock().unwrap().push(self.stage);
            self.outcome.clone()
        }
    }

    fn fail() -> StageOutcome {
        StageOutcome::Fail {
            category: "c".into(),
            message: "m".into(),
        }
    }

    fn pipeline(calls: &'static Mutex<Vec<Stage>>, outcomes: &[(Stage, StageOutcome)]) -> Pipeline {
        let mut p = Pipeline::new();
        for (stage, outcome) in outcomes {
            p.register(Box::new(Fixed {
                stage: *stage,
                outcome: outcome.clone(),
                calls,
            }));
        }
        p
    }

    fn leak() -> &'static Mutex<Vec<Stage>> {
        Box::leak(Box::new(Mutex::new(Vec::new())))
    }

    fn stages_of(r: &CaseResult) -> Vec<(Stage, OutcomeKind)> {
        r.stages
            .iter()
            .map(|s| (s.stage, s.outcome.kind()))
            .collect()
    }

    // TC77: Stage の順序
    #[test]
    fn stage_order() {
        assert!(Stage::Preprocess < Stage::Parse);
        assert!(Stage::Parse < Stage::Elaborate);
        assert!(Stage::Elaborate < Stage::Simulate);
        assert!(Stage::Simulate < Stage::Compare);
        let mut v = Stage::ALL.to_vec();
        v.sort();
        assert_eq!(v, Stage::ALL);
    }

    // TC77: runner が 0 個なら Preprocess: NotImplemented、Undetermined
    #[test]
    fn empty_pipeline_is_not_implemented() {
        let p = Pipeline::new();
        for target in Stage::ALL {
            for sf in [false, true] {
                let r = p.run_case(&case(target, sf), &env());
                assert_eq!(
                    stages_of(&r),
                    vec![(Stage::Preprocess, OutcomeKind::NotImplemented)]
                );
                assert_eq!(r.verdict, Verdict::Undetermined);
            }
        }
    }

    // TC77: Parse で Fail なら Elaborate 以降を実行しない
    #[test]
    fn stops_at_first_non_pass() {
        let calls = leak();
        let p = pipeline(
            calls,
            &[
                (Stage::Preprocess, StageOutcome::Pass),
                (Stage::Parse, fail()),
                (Stage::Elaborate, StageOutcome::Pass),
                (Stage::Simulate, StageOutcome::Pass),
            ],
        );
        let r = p.run_case(&case(Stage::Simulate, false), &env());
        assert_eq!(
            stages_of(&r),
            vec![
                (Stage::Preprocess, OutcomeKind::Pass),
                (Stage::Parse, OutcomeKind::Fail)
            ]
        );
        assert_eq!(
            *calls.lock().unwrap(),
            vec![Stage::Preprocess, Stage::Parse]
        );
        assert_eq!(r.verdict, Verdict::Unexpected);
    }

    // TC77: target = Parse なら Elaborate を実行しない
    #[test]
    fn stops_at_target() {
        let calls = leak();
        let p = pipeline(
            calls,
            &[
                (Stage::Preprocess, StageOutcome::Pass),
                (Stage::Parse, StageOutcome::Pass),
                (Stage::Elaborate, StageOutcome::Pass),
            ],
        );
        let r = p.run_case(&case(Stage::Parse, false), &env());
        assert_eq!(r.stages.len(), 2);
        assert_eq!(
            *calls.lock().unwrap(),
            vec![Stage::Preprocess, Stage::Parse]
        );
        assert_eq!(r.verdict, Verdict::AsExpected);
    }

    // TC77: runner の無い段階は NotImplemented で止まる
    #[test]
    fn missing_runner_stops_with_not_implemented() {
        let calls = leak();
        let p = pipeline(calls, &[(Stage::Preprocess, StageOutcome::Pass)]);
        let r = p.run_case(&case(Stage::Elaborate, false), &env());
        assert_eq!(
            stages_of(&r),
            vec![
                (Stage::Preprocess, OutcomeKind::Pass),
                (Stage::Parse, OutcomeKind::NotImplemented)
            ]
        );
        assert_eq!(r.verdict, Verdict::Undetermined);
    }

    // TC77 / Q-21: 判定の表 (target = Parse)
    #[test]
    fn verdict_table() {
        use StageOutcome as O;
        let skipped = O::Skipped { reason: "r".into() };
        type Row = (bool, Vec<(Stage, StageOutcome)>, Verdict);
        let rows: Vec<Row> = vec![
            (
                true,
                vec![(Stage::Preprocess, O::Pass), (Stage::Parse, fail())],
                Verdict::AsExpected,
            ),
            (true, vec![(Stage::Preprocess, fail())], Verdict::AsExpected),
            (
                true,
                vec![(Stage::Preprocess, O::Pass), (Stage::Parse, O::Pass)],
                Verdict::Unexpected,
            ),
            (
                false,
                vec![(Stage::Preprocess, O::Pass), (Stage::Parse, O::Pass)],
                Verdict::AsExpected,
            ),
            (
                false,
                vec![(Stage::Preprocess, O::Pass), (Stage::Parse, fail())],
                Verdict::Unexpected,
            ),
        ];
        let mut rows = rows;
        for sf in [true, false] {
            rows.push((sf, vec![], Verdict::Undetermined)); // Preprocess に runner なし
            for o in [skipped.clone(), O::Timeout] {
                rows.push((
                    sf,
                    vec![(Stage::Preprocess, O::Pass), (Stage::Parse, o)],
                    Verdict::Undetermined,
                ));
            }
            // Parse に runner なし (NotImplemented)
            rows.push((
                sf,
                vec![(Stage::Preprocess, O::Pass)],
                Verdict::Undetermined,
            ));
        }
        for (sf, outcomes, want) in rows {
            let p = pipeline(leak(), &outcomes);
            let r = p.run_case(&case(Stage::Parse, sf), &env());
            assert_eq!(r.verdict, want, "should_fail={sf} {:?}", r.stages);
        }
    }

    #[test]
    fn judge_empty_stages_is_undetermined() {
        let e = case(Stage::Parse, false).expect;
        assert_eq!(judge(&e, &[]), Verdict::Undetermined);
    }

    #[test]
    fn register_replaces_same_stage() {
        let calls = leak();
        let mut p = pipeline(calls, &[(Stage::Preprocess, fail())]);
        p.register(Box::new(Fixed {
            stage: Stage::Preprocess,
            outcome: StageOutcome::Pass,
            calls,
        }));
        let r = p.run_case(&case(Stage::Preprocess, false), &env());
        assert_eq!(r.verdict, Verdict::AsExpected);
        assert!(format!("{p:?}").contains("Preprocess"));
    }

    #[test]
    fn stage_record_json_shape() {
        let r = StageRecord {
            stage: Stage::Preprocess,
            outcome: StageOutcome::NotImplemented,
        };
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"stage":"Preprocess","outcome":"NotImplemented"}"#
        );
        let r = StageRecord {
            stage: Stage::Parse,
            outcome: fail(),
        };
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"stage":"Parse","outcome":"Fail","category":"c","message":"m"}"#
        );
    }
}
