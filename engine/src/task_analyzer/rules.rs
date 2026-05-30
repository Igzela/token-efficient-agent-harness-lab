use std::collections::HashMap;
use std::sync::LazyLock;

pub(super) static NEGATED_RISK_PHRASES: &[&str] = &[
    "no target repo writes",
    "no target repository writes",
    "do not write target repo",
    "do not write target repository",
    "target repo remains read-only",
    "target repository remains read-only",
    "without target repo writes",
    "without target repository writes",
    "no source changes",
    "does not modify target repo",
    "does not modify target repository",
    "no target repository mutation",
    "no target repo mutation",
    "without any provider calls",
    "without provider calls",
    "without model calls",
    "without any model calls",
    "no provider calls",
    "no model calls",
    "do not call providers",
    "do not call any providers",
    "no api key",
    "no credentials",
    "without any sandbox execution",
    "without sandbox execution",
    "without executing commands",
    "no sandbox execution",
    "no sandbox",
    "do not run sandbox",
    "no container",
    "no worker",
    "no autonomous workers",
    "read-only validation",
    "audit only",
    "review only",
];

pub(super) static PHRASE_FLAGS: LazyLock<HashMap<&'static str, Vec<&'static str>>> =
    LazyLock::new(|| {
        let mut m: HashMap<&'static str, Vec<&'static str>> = HashMap::new();
        for phrase in &[
            "no target repo writes",
            "no target repository writes",
            "do not write target repo",
            "do not write target repository",
            "target repo remains read-only",
            "target repository remains read-only",
            "without target repo writes",
            "without target repository writes",
            "no source changes",
            "does not modify target repo",
            "does not modify target repository",
            "no target repository mutation",
            "no target repo mutation",
            "read-only validation",
            "audit only",
            "review only",
        ] {
            m.insert(phrase, vec!["target_write"]);
        }
        for phrase in &[
            "without any provider calls",
            "without provider calls",
            "without model calls",
            "without any model calls",
            "no provider calls",
            "no model calls",
            "do not call providers",
            "do not call any providers",
        ] {
            m.insert(phrase, vec!["provider_call"]);
        }
        m.insert("no api key", vec!["secret_handling"]);
        m.insert("no credentials", vec!["secret_handling"]);
        for phrase in &[
            "without any sandbox execution",
            "without sandbox execution",
            "without executing commands",
            "no sandbox execution",
            "no sandbox",
            "do not run sandbox",
            "no container",
            "no worker",
            "no autonomous workers",
        ] {
            m.insert(phrase, vec!["sandbox_execution"]);
        }
        m
    });

pub(super) static DOMAIN_KEYWORDS: &[(&str, &[&str])] = &[
    (
        "code",
        &[
            "function",
            "class",
            "module",
            "implement",
            "refactor",
            "debug",
            "bug",
            "fix",
            "endpoint",
            "method",
            "variable",
            "auth.py",
            "test_auth",
        ],
    ),
    (
        "docs",
        &[
            "document",
            "documentation",
            "readme",
            "docs",
            "guide",
            "tutorial",
            "write docs",
            "update docs",
        ],
    ),
    (
        "config",
        &[
            "config",
            "configuration",
            "settings",
            "yaml",
            "toml",
            "env",
            "environment",
            ".env",
            "ci/cd",
            "pipeline config",
        ],
    ),
    (
        "infra",
        &[
            "infrastructure",
            "deploy",
            "deployment",
            "docker",
            "kubernetes",
            "k8s",
            "terraform",
            "aws",
            "cloud",
            "server",
            "container",
        ],
    ),
    (
        "math",
        &[
            "calculate",
            "compute",
            "formula",
            "equation",
            "algorithm",
            "batch size",
            "mathematical",
            "optimal batch",
        ],
    ),
    (
        "architecture",
        &[
            "architecture",
            "system design",
            "microservice",
            "architectural",
            "high-level design",
            "component design",
        ],
    ),
    (
        "repo_ops",
        &[
            "commit",
            "push",
            "merge",
            "branch",
            "pull request",
            "pr",
            "git",
            "repository",
            "repo",
            "clone",
            "fork",
        ],
    ),
    (
        "governance",
        &[
            "governance",
            "compliance",
            "audit",
            "policy",
            "security audit",
            "vulnerability",
            "security review",
            "security",
        ],
    ),
];

pub(super) static INTENT_KEYWORDS: &[(&str, &[&str])] = &[
    (
        "generate",
        &[
            "generate",
            "create",
            "build",
            "implement",
            "add",
            "new",
            "scaffold",
            "produce",
        ],
    ),
    (
        "review",
        &["review", "look at", "analyze for issues", "examine"],
    ),
    (
        "debug",
        &[
            "debug",
            "fix",
            "bug",
            "error",
            "failing",
            "broken",
            "troubleshoot",
            "diagnose",
        ],
    ),
    (
        "summarize",
        &[
            "summarize",
            "summary",
            "overview",
            "tldr",
            "short version",
            "condensed",
        ],
    ),
    (
        "audit",
        &[
            "audit",
            "compliance",
            "vulnerability",
            "scan",
            "penetration",
            "security audit",
            "security review",
        ],
    ),
    (
        "plan",
        &[
            "plan",
            "strategy",
            "roadmap",
            "approach",
            "design plan",
            "implementation plan",
        ],
    ),
    (
        "refactor",
        &[
            "refactor",
            "restructure",
            "reorganize",
            "clean up",
            "improve code",
            "code quality",
        ],
    ),
    (
        "compare",
        &[
            "compare",
            "contrast",
            "versus",
            "vs",
            "difference",
            "trade-off",
        ],
    ),
    (
        "explain",
        &[
            "explain",
            "describe",
            "how does",
            "what is",
            "clarify",
            "elaborate",
        ],
    ),
    (
        "classify",
        &["classify", "categorize", "sort", "group", "label", "tag"],
    ),
];

pub(super) static RISK_KEYWORDS: &[(&str, &[&str])] = &[
    (
        "target_write",
        &[
            "write",
            "modify",
            "edit",
            "commit",
            "push",
            "merge",
            "delete",
            "remove",
            "create file",
            "fix and commit",
        ],
    ),
    (
        "provider_call",
        &[
            "call openai",
            "call anthropic",
            "openai api",
            "anthropic api",
            "provider call",
            "model call",
            "llm call",
            "gpt api",
            "claude api",
        ],
    ),
    (
        "sandbox_execution",
        &["sandbox", "container", "docker run", "shell command"],
    ),
    (
        "deployment",
        &[
            "deploy",
            "release",
            "publish",
            "production",
            "staging",
            "ship",
        ],
    ),
    (
        "secret_handling",
        &[
            "secret",
            "api key",
            "credential",
            "password",
            "rotate key",
            "rotate the api",
        ],
    ),
    (
        "destructive_operation",
        &[
            "delete",
            "drop table",
            "destroy",
            "wipe",
            "purge",
            "truncate",
            "rm -rf",
        ],
    ),
    (
        "long_context",
        &[
            "500-file",
            "large codebase",
            "entire repo",
            "all files",
            "full codebase",
            "massive",
            "huge",
        ],
    ),
    (
        "high_uncertainty",
        &[
            "unclear",
            "ambiguous",
            "not sure",
            "maybe",
            "might",
            "possibly",
            "make it better",
        ],
    ),
];

pub(super) static BUDGET_BASE: &[(&str, i64)] = &[
    ("code", 3000),
    ("docs", 2000),
    ("config", 1500),
    ("infra", 2500),
    ("math", 2000),
    ("architecture", 3500),
    ("repo_ops", 1500),
    ("governance", 2000),
    ("other", 2000),
];

pub(super) static INTENT_MULTIPLIER: &[(&str, f64)] = &[
    ("generate", 1.5),
    ("review", 1.0),
    ("debug", 1.3),
    ("summarize", 0.7),
    ("audit", 1.2),
    ("plan", 1.4),
    ("refactor", 1.3),
    ("compare", 1.1),
    ("explain", 0.9),
    ("classify", 0.8),
];

pub(super) static NEGATION_PREFIXES: &[&str] = &[
    "without any ",
    "without ",
    "no ",
    "do not ",
    "don't ",
    "never ",
    "cannot ",
    "can't ",
    "must not ",
    "shall not ",
];
