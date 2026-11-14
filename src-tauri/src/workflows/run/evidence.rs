//! Typed, source-bound review policy. Legacy prose never supplies a passing gate.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidencePolicy {
    pub max_reviews: u16,
    pub max_repairs: u16,
    pub max_story_proposals: u16,
    pub require_red: bool,
    pub require_strict_clippy: bool,
    pub native_targets: Vec<String>,
    pub consumer_scope: Vec<String>,
    pub changed_tests: Vec<String>,
}

impl Default for EvidencePolicy {
    fn default() -> Self {
        Self {
            max_reviews: 2,
            max_repairs: 1,
            max_story_proposals: 16,
            require_red: false,
            require_strict_clippy: false,
            native_targets: vec![],
            consumer_scope: vec![],
            changed_tests: vec![],
        }
    }
}

impl EvidencePolicy {
    pub fn validate(&self) -> Result<(), String> {
        if self.max_reviews == 0
            // Boss Q8=A (2026-11-12): the per-run review cap is 32.
            || self.max_reviews > 32
            || self.max_repairs >= self.max_reviews
            || self.max_story_proposals > 100
            || self.consumer_scope.len() > 100
            || self.native_targets.len() > 16
            || self.changed_tests.len() > 100
            || self
                .consumer_scope
                .iter()
                .chain(&self.native_targets)
                .chain(&self.changed_tests)
                .any(|v| v.trim().is_empty())
        {
            return Err("invalid evidence policy bounds".into());
        }
        Ok(())
    }
}

/// PR subjects are evidence only. They cannot invoke a native story transition.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReviewSubject {
    Story {
        id: String,
        commit: String,
        tree: String,
        requirements: String,
    },
    PullRequest {
        repository: String,
        number: u64,
        base: String,
        head: String,
        tree: String,
        requirements: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TriageDecision {
    Fix,
    Story,
    Accept,
    Unresolved,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScoredFinding {
    pub id: String,
    pub risk: i16,
    pub value: i16,
    pub effort: i16,
    pub adds_mechanism: bool,
    pub deletes_only: bool,
    pub scope: Vec<String>,
    pub accepted_risk: Option<String>,
    pub proposal: Option<String>,
}

impl ScoredFinding {
    /// First match wins, including risk five before the mechanism rule.
    pub fn decision(&self) -> TriageDecision {
        use TriageDecision::{Accept, Fix, Story, Unresolved};
        if !(1..=5).contains(&self.risk)
            || !(1..=5).contains(&self.value)
            || !(1..=5).contains(&self.effort)
            || (self.adds_mechanism && self.deletes_only)
        {
            return Unresolved;
        }
        if self.risk == 5 {
            Fix
        } else if self.adds_mechanism {
            if self.value >= 3 { Story } else { Accept }
        } else if (self.deletes_only && self.effort <= 2)
            || ((3..=4).contains(&self.risk) && self.effort <= 3)
            || self.value - self.effort >= 2
        {
            Fix
        } else if self.value >= 3 {
            Story
        } else {
            Accept
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProofCheck {
    pub id: String,
    pub subject: ReviewSubject,
    pub target: String,
    pub native: bool,
    pub toolchain: String,
    pub argv: Vec<String>,
    pub config_digest: String,
    pub scope: Vec<String>,
    pub matched_tests: u64,
    pub exit_code: i32,
    pub log_digest: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RedEvidence {
    pub pre_fix_commit: String,
    pub test: String,
    pub catches: String,
    pub check: ProofCheck,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TestChange {
    pub test: String,
    pub old_expectation_wrong_because: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProofEvidence {
    pub reality_effect: String,
    pub reality_state: String,
    pub reality_assumptions: String,
    pub reality_radius: String,
    pub symptom: Option<String>,
    pub red: Option<RedEvidence>,
    pub test_changes: Vec<TestChange>,
    pub checks: Vec<ProofCheck>,
    pub omissions: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewEvidence {
    pub subject: ReviewSubject,
    pub findings: Vec<ScoredFinding>,
    /// IDs of FIX findings resolved by this review, retained across repair generations.
    pub resolved_fixes: Vec<String>,
    pub proof: ProofEvidence,
}

fn nonempty(value: &str) -> bool {
    !value.trim().is_empty()
}
fn git_digest(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

impl ReviewSubject {
    pub fn validate(&self) -> Result<(), String> {
        let (identity, commit, tree, requirements) = match self {
            Self::Story {
                id,
                commit,
                tree,
                requirements,
            } => (nonempty(id), commit, tree, requirements),
            Self::PullRequest {
                repository,
                number,
                base,
                head,
                tree,
                requirements,
            } => (
                nonempty(repository) && *number > 0 && git_digest(base),
                head,
                tree,
                requirements,
            ),
        };
        if !identity || !git_digest(commit) || !git_digest(tree) || !digest(requirements) {
            return Err("incomplete review subject".into());
        }
        Ok(())
    }
    fn commit(&self) -> &str {
        match self {
            Self::Story { commit, .. } => commit,
            Self::PullRequest { head, .. } => head,
        }
    }
}

impl ProofCheck {
    fn validate(&self) -> Result<(), String> {
        self.subject.validate()?;
        if !nonempty(&self.id)
            || !nonempty(&self.target)
            || !nonempty(&self.toolchain)
            || self.argv.is_empty()
            || self.argv.iter().any(|v| !nonempty(v))
            || !digest(&self.config_digest)
            || !digest(&self.log_digest)
            || (!self.scope.is_empty() && self.matched_tests == 0)
        {
            return Err("incomplete proof check or zero matched tests".into());
        }
        Ok(())
    }
}

impl ReviewEvidence {
    /// Validate a report against the host-derived current subject, never its own claim.
    pub fn validate(
        &self,
        current: &ReviewSubject,
        policy: &EvidencePolicy,
    ) -> Result<TriageDecision, String> {
        policy.validate()?;
        current.validate()?;
        if &self.subject != current {
            return Err("review subject or requirements moved".into());
        }
        if self.findings.len() > 100
            || self.proof.checks.len() > 100
            || self.proof.test_changes.len() > 100
            || self.proof.omissions.len() > 100
            || self.resolved_fixes.len() > 100
        {
            return Err("evidence exceeds bounded policy".into());
        }
        let mut ids = std::collections::HashSet::new();
        let mut proposals = 0;
        let mut fix = false;
        for finding in &self.findings {
            if !nonempty(&finding.id) || !ids.insert(&finding.id) {
                return Err("duplicate or empty finding id".into());
            }
            match finding.decision() {
                TriageDecision::Unresolved => return Err("malformed finding scores".into()),
                TriageDecision::Fix => fix = true,
                TriageDecision::Story => {
                    proposals += 1;
                    if finding.proposal.as_deref().is_none_or(|v| !nonempty(v))
                        || finding.scope.is_empty()
                        || finding
                            .scope
                            .iter()
                            .any(|s| !policy.consumer_scope.contains(s))
                    {
                        return Err("STORY needs a scope-limited proposal".into());
                    }
                }
                TriageDecision::Accept => {
                    if finding
                        .accepted_risk
                        .as_deref()
                        .is_none_or(|v| !nonempty(v))
                    {
                        return Err("ACCEPT requires named risk evidence".into());
                    }
                }
            }
        }
        if proposals > policy.max_story_proposals {
            return Err("story proposal budget exhausted".into());
        }
        // A FIX may route to repair without claiming final successful proof.
        if fix {
            return Ok(TriageDecision::Fix);
        }
        let proof = &self.proof;
        if [
            &proof.reality_effect,
            &proof.reality_state,
            &proof.reality_assumptions,
            &proof.reality_radius,
        ]
        .iter()
        .any(|v| !nonempty(v))
            || proof
                .test_changes
                .iter()
                .any(|v| !nonempty(&v.test) || !nonempty(&v.old_expectation_wrong_because))
            || proof.omissions.iter().any(|v| !nonempty(v))
        {
            return Err("REALITY or TEST-CHANGE evidence is incomplete".into());
        }
        if policy
            .changed_tests
            .iter()
            .any(|test| !proof.test_changes.iter().any(|change| &change.test == test))
        {
            return Err("TEST-CHANGE rationale missing for changed assertion".into());
        }
        if policy.require_red && proof.red.is_none() {
            return Err("RED evidence missing".into());
        }
        if let Some(red) = &proof.red {
            red.check.validate()?;
            let same_requirements = match (current, &red.check.subject) {
                (
                    ReviewSubject::Story {
                        id, requirements, ..
                    },
                    ReviewSubject::Story {
                        id: old_id,
                        requirements: old_requirements,
                        ..
                    },
                ) => id == old_id && requirements == old_requirements,
                (
                    ReviewSubject::PullRequest {
                        repository,
                        number,
                        requirements,
                        ..
                    },
                    ReviewSubject::PullRequest {
                        repository: old_repository,
                        number: old_number,
                        requirements: old_requirements,
                        ..
                    },
                ) => {
                    repository == old_repository
                        && number == old_number
                        && requirements == old_requirements
                }
                _ => false,
            };
            if !same_requirements
                || !nonempty(&red.pre_fix_commit)
                || red.pre_fix_commit == current.commit()
                || red.check.subject.commit() != red.pre_fix_commit
                || red.check.exit_code == 0
                || red.check.matched_tests == 0
                || !red.check.scope.contains(&red.test)
                || !nonempty(&red.catches)
                || proof.symptom.as_deref().is_none_or(|v| !nonempty(v))
            {
                return Err("RED pre-fix provenance or SYMPTOM is invalid".into());
            }
        }
        for check in &proof.checks {
            check.validate()?;
            if &check.subject != current || check.exit_code != 0 {
                return Err("final check failed or is stale".into());
            }
        }
        for scope in &policy.consumer_scope {
            if !proof
                .checks
                .iter()
                .any(|c| c.scope.contains(scope) && c.matched_tests > 0)
            {
                return Err(format!("final consumer scope missing: {scope}"));
            }
        }
        for target in &policy.native_targets {
            if !proof.checks.iter().any(|c| c.target == *target && c.native) {
                return Err(format!("native proof missing: {target}"));
            }
        }
        if policy.require_strict_clippy
            && !proof.checks.iter().any(|c| {
                c.argv.first().is_some_and(|s| s == "cargo")
                    && c.argv.get(1).is_some_and(|s| s == "clippy")
                    && c.argv.iter().any(|s| s == "--all-targets")
                    && c.argv
                        .ends_with(&["--".into(), "-D".into(), "warnings".into()])
                    && !c
                        .argv
                        .iter()
                        .any(|s| s == "--cap-lints" || s.starts_with("--cap-lints="))
            })
        {
            return Err("strict all-targets clippy missing".into());
        }
        Ok(TriageDecision::Accept)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triage_first_match_oracle_catches_rule_order_and_score_boundaries() {
        // Each tuple is an independently specified row/boundary of the approved seven-rule table.
        use TriageDecision::{Accept, Fix, Story, Unresolved};
        let cases = [
            (5, 1, 5, true, false, Fix),
            (5, 1, 5, false, false, Fix),
            (4, 3, 1, true, false, Story),
            (4, 2, 1, true, false, Accept),
            (1, 1, 2, false, true, Fix),
            (1, 1, 3, false, true, Accept),
            (3, 1, 3, false, false, Fix),
            (4, 1, 3, false, false, Fix),
            (3, 1, 4, false, false, Accept),
            (2, 1, 3, false, false, Accept),
            (1, 4, 2, false, false, Fix),
            (1, 3, 2, false, false, Story),
            (1, 3, 5, false, false, Story),
            (1, 2, 5, false, false, Accept),
            (0, 3, 1, false, false, Unresolved),
            (6, 3, 1, false, false, Unresolved),
            (1, 0, 1, false, false, Unresolved),
            (1, 6, 1, false, false, Unresolved),
            (1, 3, 0, false, false, Unresolved),
            (1, 3, 6, false, false, Unresolved),
            (5, 5, 1, true, true, Unresolved),
        ];
        let failures: Vec<_> = cases
            .into_iter()
            .filter_map(
                |(risk, value, effort, adds_mechanism, deletes_only, want)| {
                    let finding = ScoredFinding {
                        id: "f".into(),
                        risk,
                        value,
                        effort,
                        adds_mechanism,
                        deletes_only,
                        scope: vec![],
                        accepted_risk: None,
                        proposal: None,
                    };
                    (finding.decision() != want).then_some((finding, want))
                },
            )
            .collect();
        assert!(failures.is_empty(), "triage mismatches: {failures:?}");
    }

    fn subject() -> ReviewSubject {
        ReviewSubject::PullRequest {
            repository: "owner/repo".into(),
            number: 42,
            base: "2".repeat(40),
            head: "1".repeat(40),
            tree: "3".repeat(40),
            requirements: "a".repeat(64),
        }
    }
    fn check() -> ProofCheck {
        ProofCheck {
            id: "native".into(),
            subject: subject(),
            target: "macos".into(),
            native: true,
            toolchain: "rustc pinned".into(),
            argv: vec![
                "cargo".into(),
                "clippy".into(),
                "--all-targets".into(),
                "--".into(),
                "-D".into(),
                "warnings".into(),
            ],
            config_digest: "b".repeat(64),
            scope: vec!["consumer".into()],
            matched_tests: 1,
            exit_code: 0,
            log_digest: "c".repeat(64),
        }
    }
    fn evidence() -> ReviewEvidence {
        ReviewEvidence {
            subject: subject(),
            findings: vec![],
            resolved_fixes: vec![],
            proof: ProofEvidence {
                reality_effect: "observable consumer".into(),
                reality_state: "native producer".into(),
                reality_assumptions: "none".into(),
                reality_radius: "scope".into(),
                symptom: None,
                red: None,
                test_changes: vec![],
                checks: vec![check()],
                omissions: vec!["No full suite; only final consumer".into()],
            },
        }
    }
    fn strict_policy() -> EvidencePolicy {
        EvidencePolicy {
            require_strict_clippy: true,
            native_targets: vec!["macos".into()],
            consumer_scope: vec!["consumer".into()],
            ..Default::default()
        }
    }

    #[test]
    fn proof_replay_rejects_changed_requirements_zero_scope_linux_and_lint_weakening() {
        // catches: optimistic proof labels overriding exact source/requirements, platform or scope.
        let policy = strict_policy();
        let original = evidence();
        let encoded = serde_json::to_string(&original).unwrap();
        let replay: ReviewEvidence = serde_json::from_str(&encoded).unwrap();
        assert_eq!(
            replay.validate(&subject(), &policy),
            Ok(TriageDecision::Accept)
        );
        let mut changed_subject = subject();
        if let ReviewSubject::PullRequest { requirements, .. } = &mut changed_subject {
            *requirements = "d".repeat(64);
        }
        assert!(replay.validate(&changed_subject, &policy).is_err());
        for case in [
            "zero",
            "linux",
            "scope",
            "clippy",
            "failed",
            "test-change",
            "reality",
            "omission",
        ] {
            let mut broken = original.clone();
            let mut required = policy.clone();
            match case {
                "zero" => broken.proof.checks[0].matched_tests = 0,
                "linux" => {
                    broken.proof.checks[0].target = "linux".into();
                    broken.proof.checks[0].native = false;
                }
                "scope" => broken.proof.checks[0].scope = vec!["other".into()],
                "clippy" => broken.proof.checks[0]
                    .argv
                    .extend(["-A".into(), "warnings".into()]),
                "failed" => broken.proof.checks[0].exit_code = 1,
                "test-change" => required.changed_tests.push("changed assertion".into()),
                "reality" => broken.proof.reality_state.clear(),
                "omission" => broken.proof.omissions.push(String::new()),
                _ => unreachable!(),
            }
            assert!(
                broken.validate(&subject(), &required).is_err(),
                "{case} must not pass"
            );
        }
    }

    #[test]
    fn accepted_risks_and_story_proposals_are_explicit_scoped_and_bounded() {
        // catches: silently discarded risks or STORY expanding the current assignment.
        let policy = strict_policy();
        let mut report = evidence();
        report.findings.push(ScoredFinding {
            id: "risk".into(),
            risk: 1,
            value: 1,
            effort: 5,
            adds_mechanism: false,
            deletes_only: false,
            scope: vec![],
            accepted_risk: None,
            proposal: None,
        });
        assert!(report.validate(&subject(), &policy).is_err());
        report.findings[0].accepted_risk = Some("Known latency; not correctness".into());
        assert_eq!(
            report.validate(&subject(), &policy),
            Ok(TriageDecision::Accept)
        );
        let replay: ReviewEvidence =
            serde_json::from_value(serde_json::to_value(&report).unwrap()).unwrap();
        assert_eq!(replay, report);
        report.findings[0].value = 3;
        report.findings[0].proposal = Some("Separate bounded improvement".into());
        report.findings[0].scope = vec!["outside".into()];
        assert!(report.validate(&subject(), &policy).is_err());
        report.findings[0].scope = vec!["consumer".into()];
        assert!(report.validate(&subject(), &policy).is_ok());
        let mut no_proposals = policy;
        no_proposals.max_story_proposals = 0;
        assert!(report.validate(&subject(), &no_proposals).is_err());
    }

    #[test]
    fn red_requires_failed_named_test_on_pre_fix_subject_and_symptom() {
        // catches: a final GREEN or unrelated failure being relabelled as pre-fix RED.
        let mut policy = strict_policy();
        policy.require_red = true;
        let mut report = evidence();
        assert!(report.validate(&subject(), &policy).is_err());
        let mut red_check = check();
        if let ReviewSubject::PullRequest { head, .. } = &mut red_check.subject {
            *head = "4".repeat(40);
        }
        red_check.exit_code = 100;
        report.proof.symptom = Some("Old consumer returned wrong result".into());
        report.proof.red = Some(RedEvidence {
            pre_fix_commit: "4".repeat(40),
            test: "consumer".into(),
            catches: "wrong consumer result".into(),
            check: red_check,
        });
        assert!(report.validate(&subject(), &policy).is_ok());
        for case in [
            "green",
            "same-source",
            "zero",
            "wrong-test",
            "symptom",
            "wrong-subject",
        ] {
            let mut broken = report.clone();
            let red = broken.proof.red.as_mut().unwrap();
            match case {
                "green" => red.check.exit_code = 0,
                "same-source" => red.pre_fix_commit = "1".repeat(40),
                "zero" => red.check.matched_tests = 0,
                "wrong-test" => red.test = "unrelated".into(),
                "symptom" => broken.proof.symptom = None,
                "wrong-subject" => {
                    if let ReviewSubject::PullRequest { number, .. } = &mut red.check.subject {
                        *number = 99;
                    }
                }
                _ => unreachable!(),
            }
            assert!(broken.validate(&subject(), &policy).is_err(), "{case}");
        }
    }
}
