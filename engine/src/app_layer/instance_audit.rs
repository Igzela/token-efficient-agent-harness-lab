use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

const REQUIRED_FILES: &[&str] = &[
    "AGENTS.md",
    "docs/harness/PROJECT_BOARD.md",
    "docs/harness/TASK_QUEUE.md",
    "docs/harness/QUALITY_GATES.md",
    "docs/harness/DECISION_RECORD.md",
    "docs/harness/RISK_REGISTER.md",
];

const OPTIONAL_RECOMMENDED_FILES: &[&str] = &[
    "docs/harness/FINAL_GATE.md",
    "docs/harness/EVIDENCE_INDEX.md",
];

static RE_PUSH_WITHOUT_APPROVAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)push(?:ing)?\s+(?:directly\s+)?to\s+`?(?:main|master)`?\s+without\s+approval")
        .unwrap()
});

static RE_FUTURE_PHASE_DONE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\|\s*(?:P5|CA-8|Stage 5)[^|]*\|[^|]*\|\s*(?:\*\*)?done").unwrap()
});

static RE_SLICE_HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^###\s+").unwrap());
static RE_STATUS_FIELD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\*\*Status\*\*\s*:").unwrap());
static RE_GOAL_FIELD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\*\*Goal\*\*\s*:").unwrap());
static RE_STATUS_GATED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\*\*Status\*\*\s*:\s*(ready-with-approval|blocked)").unwrap()
});
static RE_MALFORMED_TABLE_ROW: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b[A-Z0-9]+-[A-Z0-9]+\b\s*\|").unwrap());
static RE_CLOSEOUT_STATUS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\*\*Status\*\*\s*:\s*([^\n]+)").unwrap());
static RE_CLOSEOUT_TESTS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\*\*Test count\*\*\s*:\s*([^\n]+)").unwrap());
static RE_CLOSEOUT_SEALED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\*\*Sealed baseline candidate\*\*\s*:\s*([^\n]+)").unwrap());

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditCheck {
    pub check_id: String,
    pub status: String,
    pub message: String,
    pub evidence: Vec<String>,
}

#[allow(clippy::derivable_impls)]
impl Default for AuditCheck {
    fn default() -> Self {
        Self {
            check_id: String::new(),
            status: String::new(),
            message: String::new(),
            evidence: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceAuditReport {
    pub target_repo: String,
    pub verdict: String,
    pub checks: Vec<AuditCheck>,
    pub warnings: Vec<String>,
    pub blockers: Vec<String>,
    pub recommended_next_actions: Vec<String>,
}

#[allow(clippy::derivable_impls)]
impl Default for InstanceAuditReport {
    fn default() -> Self {
        Self {
            target_repo: String::new(),
            verdict: String::new(),
            checks: Vec::new(),
            warnings: Vec::new(),
            blockers: Vec::new(),
            recommended_next_actions: Vec::new(),
        }
    }
}

impl InstanceAuditReport {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

struct AuditState {
    target_repo: PathBuf,
    checks: Vec<AuditCheck>,
    warnings: Vec<String>,
    blockers: Vec<String>,
}

impl AuditState {
    fn new(target_repo: PathBuf) -> Self {
        Self {
            target_repo,
            checks: Vec::new(),
            warnings: Vec::new(),
            blockers: Vec::new(),
        }
    }

    fn add_check(&mut self, check_id: &str, status: &str, message: &str, evidence: Vec<String>) {
        self.checks.push(AuditCheck {
            check_id: check_id.to_string(),
            status: status.to_string(),
            message: message.to_string(),
            evidence,
        });
    }

    fn warn(&mut self, message: &str) {
        if !self.warnings.contains(&message.to_string()) {
            self.warnings.push(message.to_string());
        }
    }

    fn block(&mut self, message: &str) {
        if !self.blockers.contains(&message.to_string()) {
            self.blockers.push(message.to_string());
        }
    }
}

pub fn audit_instance(target_repo: &str) -> InstanceAuditReport {
    let expanded = if let Some(stripped) = target_repo.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            format!("{}/{}", home, stripped)
        } else {
            target_repo.to_string()
        }
    } else {
        target_repo.to_string()
    };
    let root = PathBuf::from(&expanded);
    let mut state = AuditState::new(root.clone());

    if !root.exists() || !root.is_dir() {
        state.block(&format!(
            "Target repository not found or not a directory: {}",
            root.display()
        ));
        state.add_check(
            "target_repo",
            "FAIL",
            "target repository is unavailable",
            vec![],
        );
        return finalize(state);
    }

    check_required_files(&mut state);
    check_optional_files(&mut state);
    check_agents_policy(&mut state);
    check_project_board(&mut state);
    check_task_queue(&mut state);
    check_quality_gates(&mut state);
    check_risk_register(&mut state);
    check_closeout_reports(&mut state);

    finalize(state)
}

pub fn format_report(report: &InstanceAuditReport) -> String {
    let mut lines = vec![
        "=".repeat(72),
        "Harness App MVP0 \u{2014} Read-Only Project Instance Audit".to_string(),
        "=".repeat(72),
        format!("Target repo: {}", report.target_repo),
        format!("Verdict: {}", report.verdict),
        String::new(),
        "Checks:".to_string(),
    ];
    for check in &report.checks {
        lines.push(format!(
            "- [{}] {}: {}",
            check.status, check.check_id, check.message
        ));
        for item in &check.evidence {
            lines.push(format!("  - {}", item));
        }
    }
    lines.push(String::new());
    lines.push("Warnings:".to_string());
    if report.warnings.is_empty() {
        lines.push("- None".to_string());
    } else {
        for w in &report.warnings {
            lines.push(format!("- {}", w));
        }
    }
    lines.push(String::new());
    lines.push("Blockers:".to_string());
    if report.blockers.is_empty() {
        lines.push("- None".to_string());
    } else {
        for b in &report.blockers {
            lines.push(format!("- {}", b));
        }
    }
    lines.push(String::new());
    lines.push("Recommended next actions:".to_string());
    for action in &report.recommended_next_actions {
        lines.push(format!("- {}", action));
    }
    lines.join("\n")
}

fn read_text(root: &Path, rel_path: &str) -> String {
    std::fs::read_to_string(root.join(rel_path)).unwrap_or_default()
}

fn file_exists(root: &Path, rel_path: &str) -> bool {
    root.join(rel_path).is_file()
}

fn contains_all(text: &str, terms: &[&str]) -> bool {
    let lowered = text.to_lowercase();
    terms.iter().all(|t| lowered.contains(&t.to_lowercase()))
}

fn contains_any(text: &str, terms: &[&str]) -> bool {
    let lowered = text.to_lowercase();
    terms.iter().any(|t| lowered.contains(&t.to_lowercase()))
}

fn guards_main_push(text: &str) -> bool {
    let lowered = text.to_lowercase();
    let phrases = [
        "pushing directly to `main`",
        "pushing directly to main",
        "push directly to main",
        "push to main",
        "not main/master directly",
        "not `main`/`master` directly",
        "not main or master directly",
        "not directly to main",
        "not directly to `main`",
    ];
    contains_any(&lowered, &phrases)
        && contains_any(
            &lowered,
            &["pause", "must not", "requires", "before", "approval", "not"],
        )
}

fn check_required_files(state: &mut AuditState) {
    let missing: Vec<String> = REQUIRED_FILES
        .iter()
        .filter(|p| !file_exists(&state.target_repo, p))
        .map(|p| p.to_string())
        .collect();
    if !missing.is_empty() {
        for path in &missing {
            state.block(&format!("Missing required harness control file: {}", path));
        }
        state.add_check(
            "required_files",
            "FAIL",
            "required harness control files are missing",
            missing,
        );
    } else {
        state.add_check(
            "required_files",
            "PASS",
            "all required harness control files are present",
            REQUIRED_FILES.iter().map(|s| s.to_string()).collect(),
        );
    }
}

fn check_optional_files(state: &mut AuditState) {
    let missing: Vec<String> = OPTIONAL_RECOMMENDED_FILES
        .iter()
        .filter(|p| !file_exists(&state.target_repo, p))
        .map(|p| p.to_string())
        .collect();
    if !missing.is_empty() {
        state.warn(&format!(
            "Missing optional recommended control files: {}",
            missing.join(", ")
        ));
        state.add_check(
            "optional_files",
            "WARN",
            "some optional recommended control files are missing",
            missing,
        );
    } else {
        state.add_check(
            "optional_files",
            "PASS",
            "optional recommended control files are present",
            OPTIONAL_RECOMMENDED_FILES
                .iter()
                .map(|s| s.to_string())
                .collect(),
        );
    }
}

fn check_agents_policy(state: &mut AuditState) {
    let text = read_text(&state.target_repo, "AGENTS.md");
    if text.is_empty() {
        state.add_check(
            "agents_policy",
            "FAIL",
            "AGENTS.md is missing or unreadable",
            vec![],
        );
        return;
    }
    let mut evidence: Vec<String> = Vec::new();

    if contains_all(&text, &["execution adapter"]) {
        evidence.push("agent is described as execution adapter".to_string());
    } else {
        state.warn("AGENTS.md does not clearly describe the agent as an execution adapter");
    }

    if contains_all(&text, &["not", "governance authority"])
        || contains_all(&text, &["not the governance authority"])
    {
        evidence.push("agent is not governance authority".to_string());
    } else {
        state.block("AGENTS.md does not clearly deny governance authority to the agent");
    }

    if contains_any(
        &text,
        &[
            "human authorisation",
            "human authorization",
            "explicit human",
            "requires human",
        ],
    ) {
        evidence.push("human authority is referenced".to_string());
    } else {
        state.block("AGENTS.md does not require human authority for governance decisions");
    }

    if RE_PUSH_WITHOUT_APPROVAL.is_match(&text) {
        state.block("AGENTS.md allows pushing main/master without approval");
    } else if guards_main_push(&text) {
        evidence.push("main/master push requires pause or approval".to_string());
    } else if contains_any(
        &text,
        &[
            "pushing directly to `main`",
            "pushing directly to main",
            "push directly to main",
            "push to main",
        ],
    ) {
        state.block("AGENTS.md mentions main/master push without an approval guard");
    } else {
        state.warn("AGENTS.md does not explicitly mention main/master push restrictions");
    }

    if contains_any(&text, &["provider", "llm provider", "real llm"]) {
        if contains_any(
            &text,
            &[
                "must not",
                "not allowed",
                "without approval",
                "requires approval",
                "do not connect",
            ],
        ) {
            evidence.push("provider integration is guarded".to_string());
        } else {
            state.warn("AGENTS.md mentions provider integration without a clear guard");
        }
    } else {
        state.warn("AGENTS.md does not mention provider integration boundaries");
    }

    if contains_any(
        &text,
        &[
            "fully automated",
            "default fully automated",
            "ordinary engineering work",
        ],
    ) {
        if contains_any(
            &text,
            &[
                "pause for explicit human confirmation",
                "must still pause",
                "before",
            ],
        ) {
            state.warn("AGENTS.md allows broad automation but includes pause conditions");
            evidence.push("broad automation has pause conditions".to_string());
        } else {
            state.block("AGENTS.md allows broad automation without pause conditions");
        }
    }

    if contains_any(
        &text,
        &["active yaml", "active state", "user/project state"],
    ) {
        if contains_any(&text, &["approval", "human", "must not", "pause"]) {
            evidence.push("active state mutation is guarded".to_string());
        } else {
            state.block("AGENTS.md does not guard active state mutation");
        }
    } else {
        state.warn("AGENTS.md does not explicitly guard active user/project state mutation");
    }

    let has_agents_blocker = state.blockers.iter().any(|b| b.contains("AGENTS.md"));
    let status = if has_agents_blocker {
        "FAIL"
    } else if !evidence.is_empty() {
        if state.warnings.iter().any(|w| w.contains("AGENTS.md")) {
            "PASS_WITH_NOTES"
        } else {
            "PASS"
        }
    } else {
        "WARN"
    };
    state.add_check(
        "agents_policy",
        status,
        "AGENTS.md execution adapter policy reviewed",
        evidence,
    );
}

fn check_project_board(state: &mut AuditState) {
    let text = read_text(&state.target_repo, "docs/harness/PROJECT_BOARD.md");
    if text.is_empty() {
        state.add_check(
            "project_board",
            "FAIL",
            "PROJECT_BOARD.md is missing or unreadable",
            vec![],
        );
        return;
    }
    let mut evidence: Vec<String> = Vec::new();

    if contains_all(&text, &["todo", "ready", "running", "review", "done"]) {
        evidence.push("task state vocabulary exists".to_string());
    } else {
        state.warn("PROJECT_BOARD.md does not expose the expected task state vocabulary");
    }

    if contains_any(&text, &["phase", "sealed baseline", "closeout"]) {
        evidence.push("phase/closeout status appears documented".to_string());
    } else {
        state.warn("PROJECT_BOARD.md does not clearly expose phase or closeout status");
    }

    let malformed_rows = find_malformed_markdown_table_rows(&text);
    if !malformed_rows.is_empty() {
        state.warn(&format!(
            "PROJECT_BOARD.md has structurally suspicious table rows: {}",
            malformed_rows[..malformed_rows.len().min(5)].join("; ")
        ));
    }

    if RE_FUTURE_PHASE_DONE.is_match(&text)
        && !contains_any(
            &text,
            &[
                "P5-000 remains blocked",
                "CA-8 has not started",
                "Stage 5 not started",
            ],
        )
    {
        state.block("Future phase appears marked done without clear closeout/blocking evidence");
    }

    if contains_any(
        &text,
        &[
            "ready-with-approval",
            "blocked",
            "pending_human",
            "pending GPT/human",
        ],
    ) {
        evidence.push("approval/blocking statuses are visible".to_string());
    } else {
        state.warn("PROJECT_BOARD.md does not show approval/blocking statuses");
    }

    let has_future_blocker = state.blockers.iter().any(|b| b.contains("Future phase"));
    let status = if has_future_blocker {
        "FAIL"
    } else if !malformed_rows.is_empty() {
        "PASS_WITH_NOTES"
    } else {
        "PASS"
    };
    state.add_check(
        "project_board",
        status,
        "project board sanity check complete",
        evidence,
    );
}

fn check_task_queue(state: &mut AuditState) {
    let text = read_text(&state.target_repo, "docs/harness/TASK_QUEUE.md");
    if text.is_empty() {
        state.add_check(
            "task_queue",
            "FAIL",
            "TASK_QUEUE.md is missing or unreadable",
            vec![],
        );
        return;
    }
    let mut evidence: Vec<String> = Vec::new();

    let slice_count = RE_SLICE_HEADING.find_iter(&text).count();
    if slice_count > 0 {
        evidence.push(format!("execution slices found: {}", slice_count));
    } else {
        state.block("TASK_QUEUE.md has no execution slices");
    }

    let status_count = RE_STATUS_FIELD.find_iter(&text).count();
    let goal_count = RE_GOAL_FIELD.find_iter(&text).count();
    if status_count == 0 || goal_count == 0 {
        state.warn("TASK_QUEUE.md slices may be missing Status/Goal fields");
    } else {
        evidence.push(format!(
            "Status fields: {}; Goal fields: {}",
            status_count, goal_count
        ));
    }

    if contains_any(
        &text,
        &["ready-with-approval", "blocked", "paused", "retired"],
    ) {
        evidence.push("non-executable statuses are present".to_string());
    } else {
        state.warn("TASK_QUEUE.md does not show blocked/approval status vocabulary");
    }

    if RE_STATUS_GATED.is_match(&text) {
        evidence.push("approval-gated or blocked slices detected".to_string());
    }

    let has_task_blocker = state.blockers.iter().any(|b| b.contains("TASK_QUEUE"));
    let status = if has_task_blocker {
        "FAIL"
    } else if !state.warnings.is_empty() {
        "PASS_WITH_NOTES"
    } else {
        "PASS"
    };
    state.add_check(
        "task_queue",
        status,
        "task queue sanity check complete",
        evidence,
    );
}

fn check_quality_gates(state: &mut AuditState) {
    let text = read_text(&state.target_repo, "docs/harness/QUALITY_GATES.md");
    if text.is_empty() {
        state.add_check(
            "quality_gates",
            "FAIL",
            "QUALITY_GATES.md is missing or unreadable",
            vec![],
        );
        return;
    }
    let mut evidence: Vec<String> = Vec::new();
    let checks: &[(&str, &[&str])] = &[
        (
            "unknown_error requires human review",
            &["unknown_error", "human review"],
        ),
        ("provider or LLM boundary present", &["provider"]),
        (
            "active state mutation requires approval",
            &["active", "human"],
        ),
        (
            "auto modification is forbidden or reviewed",
            &["auto", "modify"],
        ),
        (
            "read-only or evidence-only boundary present",
            &["read-only"],
        ),
    ];
    for (label, terms) in checks {
        if contains_all(&text, terms) {
            evidence.push(label.to_string());
        } else {
            state.warn(&format!("QUALITY_GATES.md may be missing: {}", label));
        }
    }
    let status = if state.warnings.iter().any(|w| w.contains("QUALITY_GATES")) {
        "PASS_WITH_NOTES"
    } else {
        "PASS"
    };
    state.add_check(
        "quality_gates",
        status,
        "quality gate sanity check complete",
        evidence,
    );
}

fn check_risk_register(state: &mut AuditState) {
    let text = read_text(&state.target_repo, "docs/harness/RISK_REGISTER.md");
    if text.is_empty() {
        state.add_check(
            "risk_register",
            "FAIL",
            "RISK_REGISTER.md is missing or unreadable",
            vec![],
        );
        return;
    }
    let mut evidence: Vec<String> = Vec::new();
    let required: &[(&str, &[&str])] = &[
        ("active risks exist", &["active"]),
        ("mitigated risks exist", &["mitigated"]),
        (
            "provider/LLM premature integration risk exists",
            &["provider"],
        ),
        ("scope drift risk exists", &["scope drift"]),
        ("mutation/active state risk exists", &["mutation"]),
    ];
    for (label, terms) in required {
        if contains_all(&text, terms) {
            evidence.push(label.to_string());
        } else {
            state.warn(&format!("RISK_REGISTER.md may be missing: {}", label));
        }
    }
    let status = if state.warnings.iter().any(|w| w.contains("RISK_REGISTER")) {
        "PASS_WITH_NOTES"
    } else {
        "PASS"
    };
    state.add_check(
        "risk_register",
        status,
        "risk register sanity check complete",
        evidence,
    );
}

fn check_closeout_reports(state: &mut AuditState) {
    let harness_dir = state.target_repo.join("docs").join("harness");
    if !harness_dir.is_dir() {
        state.add_check(
            "closeout_reports",
            "FAIL",
            "docs/harness directory is missing",
            vec![],
        );
        return;
    }
    let mut reports: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&harness_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with("CLOSEOUT_REPORT.md") {
                reports.push(entry.path());
            }
        }
    }
    reports.sort();
    if reports.is_empty() {
        state.warn("No closeout reports found under docs/harness");
        state.add_check(
            "closeout_reports",
            "WARN",
            "no closeout reports found",
            vec![],
        );
        return;
    }
    let mut evidence: Vec<String> = Vec::new();
    for report_path in &reports {
        let text = std::fs::read_to_string(report_path).unwrap_or_default();
        let rel = report_path
            .strip_prefix(&state.target_repo)
            .unwrap_or(report_path)
            .to_string_lossy()
            .replace('\\', "/");
        let mut parts = vec![rel];
        if let Some(caps) = RE_CLOSEOUT_STATUS.captures(&text) {
            parts.push(format!("status={}", caps[1].trim()));
        }
        if let Some(caps) = RE_CLOSEOUT_TESTS.captures(&text) {
            parts.push(format!("tests={}", caps[1].trim()));
        }
        if let Some(caps) = RE_CLOSEOUT_SEALED.captures(&text) {
            parts.push(format!("sealed_candidate={}", caps[1].trim()));
        }
        evidence.push(parts.join("; "));
    }
    state.add_check(
        "closeout_reports",
        "PASS",
        "closeout reports detected",
        evidence,
    );
}

fn find_malformed_markdown_table_rows(text: &str) -> Vec<String> {
    let mut suspicious = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        let stripped = line.trim();
        if !line.contains('|')
            || stripped.starts_with('|')
            || stripped.starts_with('-')
            || stripped.starts_with('#')
        {
            continue;
        }
        if RE_MALFORMED_TABLE_ROW.is_match(stripped) {
            let truncated = if stripped.len() > 80 {
                &stripped[..80]
            } else {
                stripped
            };
            suspicious.push(format!("line {}: {}", idx + 1, truncated));
        }
    }
    suspicious
}

fn finalize(state: AuditState) -> InstanceAuditReport {
    let verdict = if !state.blockers.is_empty() {
        "BLOCKED"
    } else if !state.warnings.is_empty() {
        "PASS_WITH_NOTES"
    } else {
        "PASS"
    };
    let recommended_next_actions = recommended_next_actions(verdict, &state);
    InstanceAuditReport {
        target_repo: state.target_repo.to_string_lossy().to_string(),
        verdict: verdict.to_string(),
        checks: state.checks,
        warnings: state.warnings,
        blockers: state.blockers,
        recommended_next_actions,
    }
}

fn recommended_next_actions(verdict: &str, state: &AuditState) -> Vec<String> {
    if verdict == "BLOCKED" {
        return vec![
            "Fix blockers before allowing execution slices to proceed.".to_string(),
            "Do not treat blocked or ready-with-approval work as executable.".to_string(),
            "Re-run the read-only instance audit after corrections.".to_string(),
        ];
    }
    let mut actions = vec![
        "Keep using the target repository as a controlled harness instance.".to_string(),
        "Do not allow the execution adapter to approve its own work.".to_string(),
        "Use human approval before active state mutation, provider integration, sandbox execution, or main-branch push.".to_string(),
    ];
    if !state.warnings.is_empty() {
        actions.insert(0, "Review warnings and convert high-friction manual controls into machine-readable indexes.".to_string());
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_harness_dir(root: &Path) {
        fs::create_dir_all(root.join("docs/harness")).unwrap();
        fs::write(root.join("AGENTS.md"), "# Agents\nThis agent is an execution adapter, not the governance authority. Requires human authorization. Pushing directly to main requires approval. Provider integration requires approval.\n").unwrap();
        fs::write(root.join("docs/harness/PROJECT_BOARD.md"), "# Board\n| ID | Name | Status |\n|---|---|---|\n| P1 | Task | done |\ntodo ready running review done\nphase sealed baseline\nready-with-approval blocked\n").unwrap();
        fs::write(root.join("docs/harness/TASK_QUEUE.md"), "# Queue\n### Slice 1\n**Status**: ready-with-approval\n**Goal**: test\nready-with-approval blocked\n").unwrap();
        fs::write(root.join("docs/harness/QUALITY_GATES.md"), "# Gates\nunknown_error human review\nprovider boundary\nactive human\nauto modify\nread-only\n").unwrap();
        fs::write(
            root.join("docs/harness/DECISION_RECORD.md"),
            "# Decisions\n",
        )
        .unwrap();
        fs::write(root.join("docs/harness/RISK_REGISTER.md"), "# Risks\nactive risks\nmitigated risks\nprovider risk\nscope drift risk\nmutation risk\n").unwrap();
    }

    #[test]
    fn test_audit_instance_pass() {
        let tmp = tempfile::tempdir().unwrap();
        setup_harness_dir(tmp.path());
        let report = audit_instance(tmp.path().to_str().unwrap());
        assert!(
            report.verdict == "PASS" || report.verdict == "PASS_WITH_NOTES",
            "expected PASS or PASS_WITH_NOTES, got {}",
            report.verdict
        );
        assert!(!report.checks.is_empty());
    }

    #[test]
    fn test_audit_instance_missing_repo() {
        let report = audit_instance("/nonexistent/repo/path/xyz123");
        assert_eq!(report.verdict, "BLOCKED");
        assert!(!report.blockers.is_empty());
    }

    #[test]
    fn test_audit_instance_missing_required_files() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("docs/harness")).unwrap();
        fs::write(tmp.path().join("AGENTS.md"), "# Agents\n").unwrap();
        let report = audit_instance(tmp.path().to_str().unwrap());
        assert_eq!(report.verdict, "BLOCKED");
        assert!(report
            .blockers
            .iter()
            .any(|b| b.contains("Missing required")));
    }

    #[test]
    fn test_format_report_contains_verdict() {
        let report = InstanceAuditReport {
            target_repo: "/tmp/test".to_string(),
            verdict: "PASS".to_string(),
            checks: vec![AuditCheck {
                check_id: "test_check".to_string(),
                status: "PASS".to_string(),
                message: "all good".to_string(),
                evidence: vec!["evidence1".to_string()],
            }],
            warnings: vec!["minor issue".to_string()],
            blockers: vec![],
            recommended_next_actions: vec!["keep going".to_string()],
        };
        let formatted = format_report(&report);
        assert!(formatted.contains("PASS"));
        assert!(formatted.contains("test_check"));
        assert!(formatted.contains("minor issue"));
        assert!(formatted.contains("keep going"));
        assert!(formatted.contains("evidence1"));
    }

    #[test]
    fn test_format_report_no_warnings_no_blockers() {
        let report = InstanceAuditReport {
            target_repo: "/tmp/test".to_string(),
            verdict: "PASS".to_string(),
            checks: vec![],
            warnings: vec![],
            blockers: vec![],
            recommended_next_actions: vec![],
        };
        let formatted = format_report(&report);
        assert!(formatted.contains("- None"));
    }

    #[test]
    fn test_report_to_json_roundtrip() {
        let report = InstanceAuditReport {
            target_repo: "/tmp/test".to_string(),
            verdict: "PASS".to_string(),
            checks: vec![AuditCheck {
                check_id: "c1".to_string(),
                status: "PASS".to_string(),
                message: "ok".to_string(),
                evidence: vec![],
            }],
            warnings: vec![],
            blockers: vec![],
            recommended_next_actions: vec![],
        };
        let json = report.to_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["verdict"].as_str().unwrap(), "PASS");
        assert_eq!(parsed["checks"][0]["check_id"].as_str().unwrap(), "c1");
    }

    #[test]
    fn test_audit_instance_with_closeout_reports() {
        let tmp = tempfile::tempdir().unwrap();
        setup_harness_dir(tmp.path());
        fs::write(tmp.path().join("docs/harness/PHASE1_CLOSEOUT_REPORT.md"), "# Closeout\n**Status**: STABLE\n**Test count**: 100\n**Sealed baseline candidate**: abc123\n").unwrap();
        let report = audit_instance(tmp.path().to_str().unwrap());
        let closeout = report
            .checks
            .iter()
            .find(|c| c.check_id == "closeout_reports");
        assert!(closeout.is_some());
        assert_eq!(closeout.unwrap().status, "PASS");
    }

    #[test]
    fn test_audit_instance_agents_policy_blocks_governance() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("docs/harness")).unwrap();
        fs::write(
            tmp.path().join("AGENTS.md"),
            "# Agents\nThis agent manages everything.\n",
        )
        .unwrap();
        for f in &REQUIRED_FILES[1..] {
            fs::write(tmp.path().join(f), "# placeholder\n").unwrap();
        }
        let report = audit_instance(tmp.path().to_str().unwrap());
        assert!(report
            .blockers
            .iter()
            .any(|b| b.contains("governance authority")));
    }

    #[test]
    fn test_contains_helpers() {
        assert!(contains_all("Hello World", &["hello", "world"]));
        assert!(!contains_all("Hello World", &["hello", "missing"]));
        assert!(contains_any("Hello World", &["missing", "world"]));
        assert!(!contains_any("Hello World", &["missing", "absent"]));
    }

    #[test]
    fn test_guards_main_push() {
        assert!(guards_main_push(
            "Pushing directly to main requires approval from the team lead."
        ));
        assert!(!guards_main_push("Push to main whenever you want."));
    }
}
