use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::ops::Deref;
use std::sync::Arc;

use crate::node_executor::{NodeExecutionInput, NodeExecutionOutput, NodeExecutor};
use crate::provider::redaction::{contains_sensitive_patterns, redact_sensitive_patterns};
use crate::storage::local_product_store::{LocalProductStore, ToolExecutionGate};
use crate::workflow::tool_registry::{HookResult, HookType};

const MAX_ENRICHMENT_BYTES: usize = 8 * 1024;
const MAX_APPROVAL_REASON_BYTES: usize = 512;
const DEFAULT_TOOL_PROFILE: &str = "default";
const PRODUCT_CALL_BUDGET_EXHAUSTED: &str = "product apply call or retry budget exhausted";

pub(crate) fn managed_tool_binding_sha256(
    workspace_id: &str,
    operation: &str,
    attempt: u64,
    node_metadata: &Value,
) -> Result<String, String> {
    let required_string = |field: &str| {
        node_metadata
            .get(field)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| format!("managed tool invocation is missing {field}"))
    };
    let inputs = match operation {
        "verify" => json!({
            "command": required_string("command")?,
        }),
        "product_verify" => json!({
            "command": required_string("command")?,
            "pre_patch_sha256": required_string("pre_patch_sha256")?,
        }),
        "repair" => json!({
            "prompt": required_string("prompt")?,
            "verification_command": required_string("verification_command")?,
            "failure_summary_sha256": required_string("failure_summary_sha256")?,
        }),
        _ => return Err("managed tool invocation has an invalid operation".to_string()),
    };
    let executor_timeout_ms = node_metadata
        .get("executor_timeout_ms")
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .ok_or_else(|| "managed tool invocation is missing executor_timeout_ms".to_string())?;
    let binding_value = json!({
        "schema_version": "managed_supervised_patch_binding.v1",
        "workspace_id": workspace_id,
        "operation": operation,
        "attempt": attempt,
        "profile_id": node_metadata.get("profile_id"),
        "executor": node_metadata.get("executor"),
        "inputs": inputs,
        "workspace_path": node_metadata.get("workspace_path"),
        "workspace_root": node_metadata.get("workspace_root"),
        "executor_timeout_ms": executor_timeout_ms,
    });
    let encoded = serde_json::to_vec(&binding_value).map_err(|error| error.to_string())?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

fn bounded_approval_reason(reason: &str) -> String {
    let mut reason = redact_sensitive_patterns(reason);
    if reason.len() > MAX_APPROVAL_REASON_BYTES {
        let mut boundary = MAX_APPROVAL_REASON_BYTES;
        while boundary > 0 && !reason.is_char_boundary(boundary) {
            boundary -= 1;
        }
        reason.truncate(boundary);
    }
    reason
}

#[derive(Clone, Debug)]
pub enum ToolPolicyKind {
    Command,
    Cli { tool_name: String },
}

enum ToolPolicyStore<'a> {
    Shared(Arc<LocalProductStore>),
    Borrowed(&'a LocalProductStore),
}

impl Deref for ToolPolicyStore<'_> {
    type Target = LocalProductStore;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Shared(store) => store,
            Self::Borrowed(store) => store,
        }
    }
}

pub struct ToolPolicyNodeExecutor<'a> {
    inner: Arc<dyn NodeExecutor>,
    store: ToolPolicyStore<'a>,
    kind: ToolPolicyKind,
    #[cfg(test)]
    before_authorization_claim: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl ToolPolicyNodeExecutor<'static> {
    pub fn command(inner: Arc<dyn NodeExecutor>, store: Arc<LocalProductStore>) -> Self {
        Self {
            inner,
            store: ToolPolicyStore::Shared(store),
            kind: ToolPolicyKind::Command,
            #[cfg(test)]
            before_authorization_claim: None,
        }
    }

    pub fn cli(
        inner: Arc<dyn NodeExecutor>,
        store: Arc<LocalProductStore>,
        tool_name: impl Into<String>,
    ) -> Self {
        Self {
            inner,
            store: ToolPolicyStore::Shared(store),
            kind: ToolPolicyKind::Cli {
                tool_name: tool_name.into(),
            },
            #[cfg(test)]
            before_authorization_claim: None,
        }
    }
}

impl<'a> ToolPolicyNodeExecutor<'a> {
    pub(crate) fn command_borrowed(
        inner: Arc<dyn NodeExecutor>,
        store: &'a LocalProductStore,
    ) -> Self {
        Self {
            inner,
            store: ToolPolicyStore::Borrowed(store),
            kind: ToolPolicyKind::Command,
            #[cfg(test)]
            before_authorization_claim: None,
        }
    }

    #[cfg(test)]
    fn with_before_authorization_claim_for_test(
        mut self,
        callback: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        self.before_authorization_claim = Some(Arc::new(callback));
        self
    }

    fn tool_name(&self, input: &NodeExecutionInput) -> Result<String, String> {
        match &self.kind {
            ToolPolicyKind::Command => {
                let command = input
                    .node_metadata
                    .get("command")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "command metadata must contain a string command".to_string())?;
                let first = command
                    .split_whitespace()
                    .next()
                    .ok_or_else(|| "empty command".to_string())?;
                if first.contains('/') {
                    return Err(
                        "command executable must be a bare allowlisted tool name".to_string()
                    );
                }
                let tool = first;
                if tool.is_empty() {
                    Err("empty command tool name".to_string())
                } else {
                    Ok(tool.to_string())
                }
            }
            ToolPolicyKind::Cli { tool_name } => Ok(tool_name.clone()),
        }
    }

    fn policy_executor_type_name(&self) -> &str {
        match &self.kind {
            ToolPolicyKind::Command => self.inner.executor_type_name(),
            ToolPolicyKind::Cli { tool_name } => tool_name,
        }
    }

    fn bound_managed_workspace(
        &self,
        input: &NodeExecutionInput,
    ) -> Result<Option<String>, String> {
        let managed = input.node_metadata.get("managed_supervised_patch");
        let workspace = if let Some(managed) = managed {
            let binding = managed.as_object().ok_or_else(|| {
                "managed supervised-patch workspace binding must be an object".to_string()
            })?;
            let workspace_id = binding
                .get("workspace_id")
                .and_then(Value::as_str)
                .filter(|value| {
                    !value.is_empty()
                        && value.len() <= 128
                        && value
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
                })
                .ok_or_else(|| {
                    "managed supervised-patch workspace binding has an invalid workspace_id"
                        .to_string()
                })?;
            let operation = binding
                .get("operation")
                .and_then(Value::as_str)
                .filter(|value| match &self.kind {
                    ToolPolicyKind::Command => matches!(*value, "verify" | "product_verify"),
                    ToolPolicyKind::Cli { .. } => matches!(*value, "repair" | "product_apply"),
                })
                .ok_or_else(|| {
                    "managed tool operation does not match its executor kind".to_string()
                })?;
            let attempt = binding
                .get("attempt")
                .and_then(Value::as_u64)
                .filter(|value| {
                    if operation == "product_apply" {
                        *value == 1
                    } else {
                        let max_attempt = if operation == "product_verify" { 8 } else { 5 };
                        (1..=max_attempt).contains(value)
                    }
                })
                .ok_or_else(|| {
                    "managed supervised-patch workspace binding has an invalid attempt".to_string()
                })?;
            let binding_sha256 = binding
                .get("binding_sha256")
                .and_then(Value::as_str)
                .filter(|value| {
                    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
                })
                .ok_or_else(|| {
                    "managed supervised-patch workspace binding has an invalid digest".to_string()
                })?;
            if binding.get("schema_version").and_then(Value::as_str)
                != Some("managed_supervised_patch.v1")
                || binding.get("content_excluded").and_then(Value::as_bool) != Some(true)
            {
                return Err("managed supervised-patch workspace binding changed".to_string());
            }
            let product_apply = operation == "product_apply";
            let current_binding_sha256 = if product_apply {
                let task_id = input
                    .node_metadata
                    .get("product_task_id")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| "product apply task identity is missing".to_string())?;
                let prompt = input
                    .node_metadata
                    .get("prompt")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| "product apply prompt is missing".to_string())?;
                let expected_objective_fingerprint = input
                    .node_metadata
                    .get("objective_fingerprint")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "product apply objective fingerprint is missing".to_string())?;
                let tool_name = match &self.kind {
                    ToolPolicyKind::Cli { tool_name } => tool_name.as_str(),
                    ToolPolicyKind::Command => "",
                };
                if binding.get("product_task_id").and_then(Value::as_str) != Some(task_id)
                    || binding.get("executor_class").and_then(Value::as_str)
                        != Some("managed_coding")
                    || input
                        .node_metadata
                        .get("executor_class")
                        .and_then(Value::as_str)
                        != Some("managed_coding")
                    || input.task_type != tool_name
                    || input.node_id != format!("{}-apply", input.workflow_id)
                    || crate::product_golden_path::fingerprint_objective(prompt)
                        != expected_objective_fingerprint
                {
                    return Err("product apply authority binding changed".to_string());
                }
                if input
                    .node_metadata
                    .get("product_apply_binding_schema_version")
                    .and_then(Value::as_str)
                    == Some("product_apply_binding.v2")
                {
                    let budget = input
                        .node_metadata
                        .get("product_budget")
                        .and_then(Value::as_object)
                        .ok_or_else(|| "product apply budget authority is missing".to_string())?;
                    let execution_attempt = input
                        .node_metadata
                        .get("execution_attempt")
                        .and_then(Value::as_u64)
                        .filter(|attempt| *attempt > 0)
                        .ok_or_else(|| {
                            "product apply scheduler attempt authority is missing".to_string()
                        })?;
                    budget
                        .get("total_tokens")
                        .and_then(Value::as_u64)
                        .filter(|limit| *limit > 0)
                        .ok_or_else(|| "product apply token budget is missing".to_string())?;
                    let total_calls = budget
                        .get("total_calls")
                        .and_then(Value::as_u64)
                        .filter(|limit| *limit > 0)
                        .ok_or_else(|| "product apply call budget is missing".to_string())?;
                    let max_retries = budget
                        .get("max_retries")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| "product apply retry budget is missing".to_string())?;
                    if execution_attempt > total_calls
                        || execution_attempt > max_retries.saturating_add(1)
                    {
                        return Err(PRODUCT_CALL_BUDGET_EXHAUSTED.to_string());
                    }
                }
                crate::product_golden_path::product_apply_binding_sha256(
                    workspace_id,
                    &input.node_metadata,
                )?
            } else {
                if input.node_id != format!("supervised-{operation}-{attempt}") {
                    return Err("managed supervised-patch workspace binding changed".to_string());
                }
                managed_tool_binding_sha256(workspace_id, operation, attempt, &input.node_metadata)?
            };
            if current_binding_sha256 != binding_sha256 {
                return Err(
                    "managed supervised-patch invocation no longer matches its binding".to_string(),
                );
            }
            if !product_apply {
                let identity = serde_json::to_vec(&json!({
                    "schema_version": "managed_supervised_patch_identity.v1",
                    "workspace_id": workspace_id,
                    "operation": operation,
                    "attempt": attempt,
                }))
                .map_err(|error| error.to_string())?;
                let expected_run_id =
                    format!("managed-run-{}", hex::encode(Sha256::digest(identity)));
                if input.run_id != expected_run_id {
                    return Err("managed supervised-patch run identity changed".to_string());
                }
            }
            let workspace = self
                .store
                .get_supervised_patch_workspace(workspace_id)?
                .ok_or_else(|| {
                    "managed supervised-patch workspace binding is missing".to_string()
                })?;
            if workspace.get("workspace_id").and_then(Value::as_str) != Some(workspace_id) {
                return Err("managed supervised-patch workspace identity changed".to_string());
            }
            if product_apply
                && workspace.get("run_id").and_then(Value::as_str) != Some(input.run_id.as_str())
            {
                return Err("product apply run-to-workspace binding changed".to_string());
            }
            if product_apply
                && workspace.get("source_revision").and_then(Value::as_str)
                    != input
                        .node_metadata
                        .get("source_revision")
                        .and_then(Value::as_str)
            {
                return Err("product apply source revision binding changed".to_string());
            }
            Some(workspace)
        } else {
            self.store
                .get_supervised_patch_workspace_for_run(&input.run_id)?
        };
        let Some(workspace) = workspace else {
            return Ok(None);
        };
        if matches!(
            workspace.get("status").and_then(Value::as_str),
            Some("rejected" | "quarantined" | "cleaned") | None
        ) {
            return Err("managed supervised-patch workspace is not executable".to_string());
        }
        let requested_path = input
            .node_metadata
            .get("workspace_path")
            .and_then(Value::as_str)
            .ok_or_else(|| "managed supervised-patch workspace path is missing".to_string())?;
        let stored_path = workspace
            .get("workspace_path")
            .and_then(Value::as_str)
            .ok_or_else(|| "stored supervised-patch workspace path is missing".to_string())?;
        let stored_canonical = workspace
            .get("workspace_canonical_path")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                "stored supervised-patch canonical workspace path is missing".to_string()
            })?;
        let current_canonical = std::fs::canonicalize(requested_path)
            .map_err(|error| format!("managed workspace is unavailable: {error}"))?;
        if requested_path != stored_path
            || current_canonical.to_string_lossy() != stored_canonical
            || input
                .node_metadata
                .get("workspace_root")
                .and_then(Value::as_str)
                != Some(stored_path)
        {
            return Err("managed supervised-patch canonical workspace binding changed".to_string());
        }
        Ok(Some(stored_path.to_string()))
    }

    fn invocation_value(
        &self,
        input: &NodeExecutionInput,
        tool_name: &str,
    ) -> Result<Value, String> {
        let invocation = match &self.kind {
            ToolPolicyKind::Command => input.node_metadata.get("command"),
            ToolPolicyKind::Cli { .. } => input
                .node_metadata
                .get("prompt")
                .or_else(|| input.node_metadata.get("command")),
        };
        let invocation = invocation
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                "tool invocation metadata must contain a non-empty string".to_string()
            })?;
        Ok(json!({
            "run_id": input.run_id,
            "node_id": input.node_id,
            "workflow_id": input.workflow_id,
            "task_type": input.task_type,
            "tool_name": tool_name,
            "invocation": invocation,
            "executor": input.node_metadata.get("executor"),
            "model": input.node_metadata.get("model"),
            "workspace_path": input.node_metadata.get("workspace_path"),
        }))
    }

    fn fail(&self, domain: &str, message: impl Into<String>) -> NodeExecutionOutput {
        NodeExecutionOutput {
            status: "failed".to_string(),
            executor_type: self.policy_executor_type_name().to_string(),
            output: None,
            error_domain: Some(domain.to_string()),
            error_message: Some(message.into()),
            input_tokens: None,
            output_tokens: None,
            estimated_cost: None,
            latency_ms: Some(0),
            process_outcome: None,
            resolved_model: None,
        }
    }

    fn fail_preserving_usage(
        &self,
        mut output: NodeExecutionOutput,
        domain: &str,
        message: impl Into<String>,
    ) -> NodeExecutionOutput {
        output.status = "failed".to_string();
        output.error_domain = Some(domain.to_string());
        output.error_message = Some(message.into());
        output
    }

    fn audit(
        &self,
        input: &NodeExecutionInput,
        action: &str,
        details: &Value,
    ) -> Result<(), String> {
        self.store
            .append_audit("tool-policy", action, &input.run_id, details)
            .map(|_| ())
    }
}

impl NodeExecutor for ToolPolicyNodeExecutor<'_> {
    fn executor_type_name(&self) -> &str {
        self.policy_executor_type_name()
    }

    fn execute_node(&self, input: &NodeExecutionInput) -> NodeExecutionOutput {
        if let ToolPolicyKind::Cli { tool_name } = &self.kind {
            if input.node_metadata.get("executor").and_then(Value::as_str)
                != Some(tool_name.as_str())
            {
                return self.fail(
                    "tool_policy_executor_mismatch",
                    "CLI executor metadata does not match the policy-bound tool identity",
                );
            }
            let requested_workspace = input
                .node_metadata
                .get("workspace_path")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty());
            let bound_workspace = match self.bound_managed_workspace(input) {
                Ok(value) => value,
                Err(error) if error == PRODUCT_CALL_BUDGET_EXHAUSTED => {
                    return self.fail("product_call_budget_exhausted", error)
                }
                Err(error) => return self.fail("cli_workspace_binding_error", error),
            };
            if !matches!(
                (requested_workspace, bound_workspace.as_deref()),
                (Some(requested), Some(bound)) if requested == bound
            ) {
                return self.fail(
                    "cli_workspace_not_bound",
                    "CLI execution requires the exact app-owned workspace bound to this run",
                );
            }
        }
        let api_owned_managed_command = input
            .node_metadata
            .pointer("/managed_supervised_patch/operation")
            .and_then(Value::as_str)
            .is_some_and(|operation| matches!(operation, "verify" | "product_verify"));
        if matches!(self.kind, ToolPolicyKind::Command) && api_owned_managed_command {
            let requested_workspace = input
                .node_metadata
                .get("workspace_path")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty());
            let bound_workspace = match self.bound_managed_workspace(input) {
                Ok(value) => value,
                Err(error) => return self.fail("command_workspace_binding_error", error),
            };
            if !matches!(
                (requested_workspace, bound_workspace.as_deref()),
                (Some(requested), Some(bound)) if requested == bound
            ) {
                return self.fail(
                    "command_workspace_not_bound",
                    "managed command execution requires its exact app-owned workspace",
                );
            }
        }
        let tool_name = match self.tool_name(input) {
            Ok(value) => value,
            Err(error) => return self.fail("tool_policy_invalid_tool", error),
        };
        let profile_id = input
            .node_metadata
            .get("profile_id")
            .and_then(Value::as_str)
            .unwrap_or(DEFAULT_TOOL_PROFILE);
        let policy_before = match self
            .store
            .current_tool_execution_policy_snapshot(profile_id, &tool_name)
        {
            Ok(value) => value,
            Err(error) => return self.fail("tool_policy_store_error", error),
        };
        if !policy_before.tool_allowed {
            let details = json!({
                "node_id": input.node_id,
                "tool_name": tool_name,
                "profile_id": profile_id,
                "allowlist_configured": policy_before.allowlist_configured,
                "policy_sha256": policy_before.sha256,
                "decision": "blocked",
                "reason_code": "not_in_authoritative_allowlist",
            });
            if let Err(error) = self.audit(input, "tool_execution.blocked", &details) {
                return self.fail("tool_policy_audit_error", error);
            }
            return self.fail(
                "tool_not_allowed",
                format!("tool {tool_name} is not allowed for profile {profile_id}"),
            );
        }

        let invocation_value = match self.invocation_value(input, &tool_name) {
            Ok(value) => value,
            Err(error) => return self.fail("tool_policy_invalid_invocation", error),
        };
        let invocation_json = match serde_json::to_vec(&invocation_value) {
            Ok(value) => value,
            Err(error) => return self.fail("tool_policy_hash_error", error.to_string()),
        };
        let invocation_sha256 = hex::encode(Sha256::digest(&invocation_json));
        let hook_context = json!({
            "schema_version": "tool_hook_context.v1",
            "run_id": input.run_id,
            "node_id": input.node_id,
            "workflow_id": input.workflow_id,
            "task_type": input.task_type,
            "tool_name": tool_name,
            "profile_id": profile_id,
            "invocation_sha256": invocation_sha256,
            "invocation_bytes": invocation_json.len(),
            "content_excluded": true,
        });
        let pre = match policy_before.evaluate_hooks(
            &HookType::PreExecution,
            &tool_name,
            &hook_context,
        ) {
            Ok(value) => value,
            Err(error) => return self.fail("tool_hook_evaluation_failed", error),
        };
        let hook_ids = pre.matched_hook_ids;
        let mut enriched_input = input.clone();
        let mut enrichment_sha256: Option<String> = None;
        let mut approval_reason: Option<String> = None;
        match pre.result {
            HookResult::Allow => {}
            HookResult::Block(reason) => {
                let details = json!({
                    "node_id": input.node_id,
                    "tool_name": tool_name,
                    "profile_id": profile_id,
                    "decision": "blocked",
                    "hook_ids": hook_ids,
                    "reason_sha256": hex::encode(Sha256::digest(reason.as_bytes())),
                });
                if let Err(error) = self.audit(input, "tool_execution.pre_hook_blocked", &details) {
                    return self.fail("tool_policy_audit_error", error);
                }
                return self.fail("tool_hook_blocked", "pre-execution hook blocked tool");
            }
            HookResult::RequestApproval(reason) => {
                approval_reason = Some(reason);
            }
            HookResult::Enrich(context) => {
                let raw = match serde_json::to_string(&context) {
                    Ok(value) => value,
                    Err(error) => {
                        return self.fail("tool_hook_enrichment_invalid", error.to_string())
                    }
                };
                if raw.len() > MAX_ENRICHMENT_BYTES {
                    return self.fail(
                        "tool_hook_enrichment_oversized",
                        format!("hook enrichment exceeds {MAX_ENRICHMENT_BYTES} bytes"),
                    );
                }
                let redacted = if contains_sensitive_patterns(&raw) {
                    redact_sensitive_patterns(&raw)
                } else {
                    raw
                };
                let bounded: Value = match serde_json::from_str(&redacted) {
                    Ok(value) => value,
                    Err(error) => {
                        return self.fail("tool_hook_enrichment_invalid", error.to_string())
                    }
                };
                let digest = hex::encode(Sha256::digest(redacted.as_bytes()));
                enrichment_sha256 = Some(digest);
                if let Some(object) = enriched_input.node_metadata.as_object_mut() {
                    object.insert("tool_policy_enrichment".to_string(), bounded);
                } else {
                    return self.fail(
                        "tool_hook_enrichment_invalid",
                        "node metadata must be an object",
                    );
                }
            }
        }

        let capability_requires_approval = policy_before.capability_requires_approval();
        if capability_requires_approval && approval_reason.is_none() {
            approval_reason = Some("tool capability requires approval".to_string());
        }
        let capability_registered = policy_before.capability_registered();
        let policy_sha256 = policy_before.sha256.clone();
        let approval_reason = approval_reason.as_deref().map(bounded_approval_reason);
        let approval_reason_sha256 = approval_reason
            .as_ref()
            .map(|reason| hex::encode(Sha256::digest(reason.as_bytes())));
        let action_binding = json!({
            "invocation_sha256": invocation_sha256,
            "hook_ids": hook_ids,
            "enrichment_sha256": enrichment_sha256,
            "capability_requires_approval": capability_requires_approval,
            "approval_required": approval_reason.is_some(),
            "approval_reason_sha256": approval_reason_sha256,
            "policy_sha256": policy_sha256,
        });
        let action_json = match serde_json::to_vec(&action_binding) {
            Ok(value) => value,
            Err(error) => return self.fail("tool_policy_hash_error", error.to_string()),
        };
        let action_sha256 = hex::encode(Sha256::digest(&action_json));

        #[cfg(test)]
        if let Some(callback) = &self.before_authorization_claim {
            callback();
        }

        let existing_authorization = match self
            .store
            .inspect_tool_execution_authorization(&input.run_id, &input.node_id)
        {
            Ok(value) => value.is_some(),
            Err(error) => return self.fail("tool_policy_store_error", error),
        };
        let mut approval_consumed = false;
        let mut implicit_receipt_claimed = false;
        if approval_reason.is_some() || existing_authorization {
            let reason = approval_reason
                .as_deref()
                .unwrap_or("existing tool authorization requires exact current-policy rebinding");
            match self.store.gate_tool_execution(
                &input.run_id,
                &input.node_id,
                &tool_name,
                profile_id,
                &policy_sha256,
                &action_sha256,
                reason,
            ) {
                Ok(ToolExecutionGate::AwaitingApproval { approval_id }) => {
                    return NodeExecutionOutput {
                        status: "awaiting_approval".to_string(),
                        executor_type: self.policy_executor_type_name().to_string(),
                        output: Some(
                            json!({
                                "approval_id": approval_id,
                                "tool_name": tool_name,
                                "action_sha256": action_sha256,
                                "content_excluded": true,
                            })
                            .to_string(),
                        ),
                        error_domain: Some("tool_approval_required".to_string()),
                        error_message: Some(
                            "tool execution requires operator approval".to_string(),
                        ),
                        input_tokens: None,
                        output_tokens: None,
                        estimated_cost: None,
                        latency_ms: Some(0),
                        process_outcome: None,
                        resolved_model: None,
                    };
                }
                Ok(ToolExecutionGate::Authorized) => {
                    approval_consumed = true;
                }
                Ok(ToolExecutionGate::Rejected) => {
                    return self.fail("tool_approval_rejected", "tool execution was rejected")
                }
                Ok(ToolExecutionGate::ConsumedOutcomeUnknown) => {
                    return self.fail(
                        "tool_execution_outcome_unknown",
                        "tool authorization was already consumed; refusing duplicate execution",
                    )
                }
                Err(error) => return self.fail("tool_approval_binding_error", error),
            }
        } else {
            match self.store.claim_tool_execution_without_approval(
                &input.run_id,
                &input.node_id,
                &tool_name,
                profile_id,
                &policy_sha256,
                &action_sha256,
            ) {
                Ok(ToolExecutionGate::Authorized) => {
                    implicit_receipt_claimed = true;
                }
                Ok(ToolExecutionGate::ConsumedOutcomeUnknown) => {
                    return self.fail(
                        "tool_execution_outcome_unknown",
                        "tool execution receipt was already consumed; refusing duplicate execution",
                    )
                }
                Ok(ToolExecutionGate::AwaitingApproval { .. })
                | Ok(ToolExecutionGate::Rejected) => {
                    return self.fail(
                        "tool_execution_receipt_invalid",
                        "implicit tool execution receipt entered an invalid state",
                    )
                }
                Err(error) => return self.fail("tool_execution_receipt_error", error),
            }
        }

        let pre_details = json!({
            "node_id": input.node_id,
            "tool_name": tool_name,
            "profile_id": profile_id,
            "decision": "allowed",
            "invocation_sha256": invocation_sha256,
            "action_sha256": action_sha256,
            "policy_sha256": policy_sha256,
            "hook_ids": hook_ids,
            "enrichment_sha256": enrichment_sha256,
            "capability_registered": capability_registered,
            "approval_consumed": approval_consumed,
            "implicit_receipt_claimed": implicit_receipt_claimed,
        });
        if let Err(error) = self.audit(input, "tool_execution.pre_policy_passed", &pre_details) {
            return self.fail(
                "tool_execution_outcome_unknown",
                format!("tool execution receipt was consumed before audit failed: {error}"),
            );
        }

        let mut output = self.inner.execute_node(&enriched_input);
        if output.status == "failed" {
            let inner_domain = output.error_domain.as_deref().unwrap_or("unknown");
            output.error_message = Some(format!(
                "tool execution began and failed with domain {inner_domain}; effect outcome is unknown"
            ));
            output.error_domain = Some("tool_effect_outcome_unknown".to_string());
        }
        let post_context = json!({
            "schema_version": "tool_hook_context.v1",
            "run_id": input.run_id,
            "node_id": input.node_id,
            "workflow_id": input.workflow_id,
            "task_type": input.task_type,
            "tool_name": tool_name,
            "profile_id": profile_id,
            "invocation_sha256": invocation_sha256,
            "action_sha256": action_sha256,
            "execution_status": output.status,
            "executor_type": output.executor_type,
            "input_tokens": output.input_tokens,
            "output_tokens": output.output_tokens,
            "estimated_cost": output.estimated_cost,
            "latency_ms": output.latency_ms,
            "content_excluded": true,
        });
        let post =
            match policy_before.evaluate_hooks(&HookType::PostExecution, &tool_name, &post_context)
            {
                Ok(value) => value,
                Err(error) => {
                    return self.fail_preserving_usage(
                        output,
                        "tool_effect_outcome_unknown",
                        format!("post-execution hook evaluation failed: {error}"),
                    )
                }
            };
        let post_hook_ids = post.matched_hook_ids;
        let (post_decision, post_enrichment_sha256) = match post.result {
            HookResult::Allow => ("allowed", None),
            HookResult::Block(reason) => {
                let reason_hash = hex::encode(Sha256::digest(reason.as_bytes()));
                if let Err(error) = self.audit(
                    input,
                    "tool_execution.post_hook_blocked",
                    &json!({
                        "node_id": input.node_id,
                        "tool_name": tool_name,
                        "hook_ids": post_hook_ids,
                        "reason_sha256": reason_hash,
                        "authoritative_usage_preserved": true,
                        "tool_effect_outcome": "unknown",
                    }),
                ) {
                    return self.fail_preserving_usage(
                        output,
                        "tool_effect_outcome_unknown",
                        format!(
                            "post-execution block audit failed after tool invocation; tool effect outcome is unknown: {error}"
                        ),
                    );
                }
                return self.fail_preserving_usage(
                    output,
                    "tool_effect_rejected_after_execution",
                    "post-execution hook rejected the result",
                );
            }
            HookResult::RequestApproval(_) => {
                return self.fail_preserving_usage(
                    output,
                    "tool_effect_outcome_unknown",
                    "post-execution approval requests are unsupported",
                )
            }
            HookResult::Enrich(context) => {
                let serialized = match serde_json::to_vec(&context) {
                    Ok(value) => value,
                    Err(error) => {
                        return self.fail_preserving_usage(
                            output,
                            "tool_effect_outcome_unknown",
                            error.to_string(),
                        )
                    }
                };
                if serialized.len() > MAX_ENRICHMENT_BYTES {
                    return self.fail_preserving_usage(
                        output,
                        "tool_effect_outcome_unknown",
                        format!("hook enrichment exceeds {MAX_ENRICHMENT_BYTES} bytes"),
                    );
                }
                (
                    "enriched_audit_only",
                    Some(hex::encode(Sha256::digest(&serialized))),
                )
            }
        };
        if let Err(error) = self.audit(
            input,
            "tool_execution.post_policy_completed",
            &json!({
                "node_id": input.node_id,
                "tool_name": tool_name,
                "profile_id": profile_id,
                "decision": post_decision,
                "hook_ids": post_hook_ids,
                "enrichment_sha256": post_enrichment_sha256,
                "authoritative_usage_preserved": true,
            }),
        ) {
            return self.fail_preserving_usage(
                output,
                "tool_effect_outcome_unknown",
                format!("post-execution audit failed: {error}"),
            );
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingExecutor {
        calls: Arc<AtomicUsize>,
    }

    impl NodeExecutor for CountingExecutor {
        fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
            self.calls.fetch_add(1, Ordering::SeqCst);
            NodeExecutionOutput {
                status: "completed".to_string(),
                executor_type: "command".to_string(),
                output: Some("fixture completed".to_string()),
                error_domain: None,
                error_message: None,
                input_tokens: Some(11),
                output_tokens: Some(7),
                estimated_cost: Some(0.01),
                latency_ms: Some(3),
                process_outcome: None,
                resolved_model: None,
            }
        }

        fn executor_type_name(&self) -> &str {
            "command"
        }
    }

    fn setup_command_run(store: &LocalProductStore) -> (String, String) {
        let plan = store
            .create_workflow_plan("tool policy test", "test", "actor", |ids, _| {
                Ok(json!({
                    "schema_version": "read_only_plan.v1",
                    "plan_id": ids.plan_id,
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "analysis": {"analysis_id": "tool-policy", "task_domain": "test"},
                    "graph": {
                        "schema_version": "workflow_graph.v1",
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "status": "decomposed",
                        "created_at": "2026-07-14T00:00:00Z",
                        "updated_at": "2026-07-14T00:00:00Z",
                        "nodes": [{
                            "schema_version": "workflow_node.v1",
                            "node_id": "node-tool",
                            "workflow_id": ids.workflow_id,
                            "task_type": "command",
                            "status": "pending",
                            "profile_id": "locked",
                            "command": "echo approved"
                        }],
                        "edges": []
                    },
                    "boundaries": {"execution_authority": "bounded_trusted_local"}
                }))
            })
            .expect("create tool plan");
        let run = store
            .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
            .expect("create tool run");
        (
            run["run_id"].as_str().unwrap().to_string(),
            plan["workflow_id"].as_str().unwrap().to_string(),
        )
    }

    #[test]
    fn managed_cli_accepts_exact_product_apply_workspace_binding() {
        let target = tempfile::tempdir().expect("target");
        let workspace = tempfile::tempdir().expect("workspace");
        let target_path = target.path().to_string_lossy().to_string();
        let workspace_path = workspace.path().to_string_lossy().to_string();
        let store = Arc::new(LocalProductStore::new(":memory:").expect("store"));
        store
            .import_supervised_patch_workspace(&json!({
                "schema_version": "supervised_patch_workspace.v1",
                "workspace_id": "product-apply-workspace",
                "run_id": "product-run",
                "target_id": "target",
                "target_repo_path": target_path,
                "target_repo_canonical_path": target_path,
                "workspace_path": workspace_path,
                "workspace_canonical_path": workspace_path,
                "source_revision": "source-revision",
                "status": "requested",
                "metadata_only": true,
                "execution_authority": "disabled",
            }))
            .expect("workspace import");

        let task_id = "product-task";
        let prompt = "Create the bounded product change";
        let objective_fingerprint = crate::product_golden_path::fingerprint_objective(prompt);
        let mut metadata = json!({
            "executor": "codex_cli",
            "executor_class": "managed_coding",
            "prompt": prompt,
            "workspace_path": workspace_path,
            "workspace_root": workspace_path,
            "workspace_id": "product-apply-workspace",
            "source_revision": "source-revision",
            "product_task_id": task_id,
            "objective_fingerprint": objective_fingerprint,
            "intake_contract_sha256": "cd".repeat(32),
            "allowed_paths": ["docs/managed.md"],
            "output_intent": "draft_pr",
            "product_apply_binding_schema_version": "product_apply_binding.v2",
            "product_budget": {
                "total_tokens": 150000,
                "total_calls": 1,
                "max_retries": 0
            },
            "execution_attempt": 1,
        });
        let binding_sha256 = crate::product_golden_path::product_apply_binding_sha256(
            "product-apply-workspace",
            &metadata,
        )
        .expect("binding");
        metadata["managed_supervised_patch"] = json!({
            "schema_version": "managed_supervised_patch.v1",
            "workspace_id": "product-apply-workspace",
            "operation": "product_apply",
            "attempt": 1,
            "binding_sha256": binding_sha256,
            "content_excluded": true,
            "product_task_id": task_id,
            "executor_class": "managed_coding",
        });
        let calls = Arc::new(AtomicUsize::new(0));
        let executor = ToolPolicyNodeExecutor::cli(
            Arc::new(CountingExecutor {
                calls: calls.clone(),
            }),
            store,
            "codex_cli",
        );
        let input = NodeExecutionInput {
            node_id: "wf-product-apply".to_string(),
            task_type: "codex_cli".to_string(),
            run_id: "product-run".to_string(),
            workflow_id: "wf-product".to_string(),
            node_metadata: metadata,
        };

        assert_eq!(
            executor
                .bound_managed_workspace(&input)
                .expect("exact product apply binding"),
            Some(workspace_path)
        );

        let mut changed_prompt = input.node_metadata.clone();
        changed_prompt["prompt"] = json!("Different product objective");
        let error = executor
            .bound_managed_workspace(&NodeExecutionInput {
                node_metadata: changed_prompt,
                ..input.clone()
            })
            .expect_err("changed prompt must fail closed");
        assert!(error.contains("authority binding changed"));

        let mut changed_paths = input.node_metadata.clone();
        changed_paths["allowed_paths"] = json!(["docs/other.md"]);
        let error = executor
            .bound_managed_workspace(&NodeExecutionInput {
                node_metadata: changed_paths,
                ..input.clone()
            })
            .expect_err("changed path scope must fail closed");
        assert!(error.contains("no longer matches its binding"));

        let mut changed_budget = input.node_metadata.clone();
        changed_budget["product_budget"]["total_tokens"] = json!(300_000);
        let error = executor
            .bound_managed_workspace(&NodeExecutionInput {
                node_metadata: changed_budget,
                ..input.clone()
            })
            .expect_err("changed token budget must fail closed");
        assert!(error.contains("no longer matches its binding"));

        let mut missing_token_ceiling = input.node_metadata.clone();
        missing_token_ceiling["product_budget"]["total_tokens"] = Value::Null;
        let error = executor
            .bound_managed_workspace(&NodeExecutionInput {
                node_metadata: missing_token_ceiling,
                ..input.clone()
            })
            .expect_err("missing effective token ceiling must fail closed");
        assert!(error.contains("token budget is missing"));

        let mut second_call = input.node_metadata.clone();
        second_call["execution_attempt"] = json!(2);
        let error = executor
            .bound_managed_workspace(&NodeExecutionInput {
                node_metadata: second_call.clone(),
                ..input.clone()
            })
            .expect_err("call and retry budget must prevent a second CLI call");
        assert!(error.contains("call or retry budget exhausted"));
        let denied = executor.execute_node(&NodeExecutionInput {
            node_metadata: second_call,
            ..input.clone()
        });
        assert_eq!(
            denied.error_domain.as_deref(),
            Some("product_call_budget_exhausted")
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        let mut changed_workspace = input.node_metadata.clone();
        changed_workspace["workspace_id"] = json!("different-workspace");
        let error = executor
            .bound_managed_workspace(&NodeExecutionInput {
                node_metadata: changed_workspace,
                ..input.clone()
            })
            .expect_err("changed workspace identity must fail closed");
        assert!(error.contains("workspace identity changed"));

        let error = executor
            .bound_managed_workspace(&NodeExecutionInput {
                run_id: "different-run".to_string(),
                ..input
            })
            .expect_err("changed run must fail closed");
        assert!(error.contains("run-to-workspace binding changed"));
    }

    #[test]
    fn approval_required_tool_executes_once_after_bound_resolution() {
        let store = Arc::new(LocalProductStore::new(":memory:").expect("store"));
        let (run_id, workflow_id) = setup_command_run(&store);
        store
            .set_tool_allowlist("locked", &["echo".to_string()])
            .expect("allowlist");
        store
            .register_tool_capability("echo", "fixture", None, None, true, "medium")
            .expect("capability");
        let calls = Arc::new(AtomicUsize::new(0));
        let executor = ToolPolicyNodeExecutor::command(
            Arc::new(CountingExecutor {
                calls: calls.clone(),
            }),
            store.clone(),
        );

        let first = store
            .tick_with_executor(&run_id, "scheduler", 0, &executor)
            .expect("first tick");
        assert_eq!(first["result"]["status"], "awaiting_approval");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let auth = store
            .inspect_tool_execution_authorization(&run_id, "node-tool")
            .expect("inspect")
            .expect("authorization");
        assert_eq!(auth["status"], "requested");
        let approval_id = auth["requested_approval_id"].as_str().unwrap();

        let resolved = store
            .resolve_requested_workflow_run_approval(
                &run_id,
                approval_id,
                "approved",
                "operator",
                Some("bounded fixture approval"),
            )
            .expect("resolve");
        assert_eq!(resolved["metadata_only"], false);
        assert_eq!(resolved["execution_authority"], "single_tool_invocation");

        let second = store
            .tick_with_executor(&run_id, "scheduler", 0, &executor)
            .expect("second tick");
        assert_eq!(second["result"]["status"], "completed");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let auth = store
            .inspect_tool_execution_authorization(&run_id, "node-tool")
            .expect("inspect")
            .expect("authorization");
        assert_eq!(auth["status"], "consumed");
        let passed = store
            .audit_events(100)
            .expect("audit events")
            .into_iter()
            .find(|event| event["action"] == "tool_execution.pre_policy_passed")
            .expect("pre-policy audit");
        assert_eq!(passed["details"]["approval_consumed"], true);

        let duplicate = executor.execute_node(&NodeExecutionInput {
            node_id: "node-tool".to_string(),
            task_type: "command".to_string(),
            run_id,
            workflow_id,
            node_metadata: json!({
                "node_id": "node-tool",
                "task_type": "command",
                "profile_id": "locked",
                "command": "echo approved"
            }),
        });
        assert_eq!(duplicate.status, "failed");
        assert_eq!(
            duplicate.error_domain.as_deref(),
            Some("tool_execution_outcome_unknown")
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn approved_tool_fails_closed_when_current_policy_no_longer_matches_binding() {
        let store = Arc::new(LocalProductStore::new(":memory:").expect("store"));
        let (run_id, _) = setup_command_run(&store);
        store
            .set_tool_allowlist("locked", &["echo".to_string()])
            .expect("allowlist");
        store
            .register_tool_capability("echo", "fixture", None, None, true, "medium")
            .expect("approval capability");
        let calls = Arc::new(AtomicUsize::new(0));
        let executor = ToolPolicyNodeExecutor::command(
            Arc::new(CountingExecutor {
                calls: calls.clone(),
            }),
            store.clone(),
        );
        let requested = store
            .tick_with_executor(&run_id, "scheduler", 0, &executor)
            .expect("approval request");
        assert_eq!(requested["result"]["status"], "awaiting_approval");
        let authorization = store
            .inspect_tool_execution_authorization(&run_id, "node-tool")
            .unwrap()
            .unwrap();
        store
            .resolve_requested_workflow_run_approval(
                &run_id,
                authorization["requested_approval_id"].as_str().unwrap(),
                "approved",
                "operator",
                Some("approve exact original policy"),
            )
            .unwrap();
        store
            .register_tool_capability("echo", "fixture", None, None, false, "medium")
            .expect("changed capability");

        let changed = store
            .tick_with_executor(&run_id, "scheduler", 0, &executor)
            .expect("bounded failed tick");

        assert_eq!(changed["result"]["status"], "failed");
        assert_eq!(
            changed["result"]["error_domain"],
            "tool_approval_binding_error"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn explicit_empty_allowlist_blocks_inner_executor() {
        let store = Arc::new(LocalProductStore::new(":memory:").expect("store"));
        store
            .set_tool_allowlist("locked", &[])
            .expect("empty allowlist");
        let calls = Arc::new(AtomicUsize::new(0));
        let executor = ToolPolicyNodeExecutor::command(
            Arc::new(CountingExecutor {
                calls: calls.clone(),
            }),
            store,
        );
        let result = executor.execute_node(&NodeExecutionInput {
            node_id: "node-tool".to_string(),
            task_type: "command".to_string(),
            run_id: "run-tool".to_string(),
            workflow_id: "workflow-tool".to_string(),
            node_metadata: json!({
                "profile_id": "locked",
                "command": "echo blocked"
            }),
        });
        assert_eq!(result.status, "failed");
        assert_eq!(result.error_domain.as_deref(), Some("tool_not_allowed"));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn missing_invocation_fails_instead_of_executing_a_noop() {
        let store = Arc::new(LocalProductStore::new(":memory:").expect("store"));
        let calls = Arc::new(AtomicUsize::new(0));
        let executor = ToolPolicyNodeExecutor::command(
            Arc::new(CountingExecutor {
                calls: calls.clone(),
            }),
            store,
        );

        let result = executor.execute_node(&NodeExecutionInput {
            node_id: "node-tool".to_string(),
            task_type: "command".to_string(),
            run_id: "run-tool".to_string(),
            workflow_id: "workflow-tool".to_string(),
            node_metadata: json!({"profile_id": "default"}),
        });

        assert_eq!(result.status, "failed");
        assert_eq!(
            result.error_domain.as_deref(),
            Some("tool_policy_invalid_tool")
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn cli_requires_exact_identity_and_nonempty_bound_workspace_before_receipt() {
        for (configured_executor, expected_domain) in [
            (Some("claude_code_cli"), "tool_policy_executor_mismatch"),
            (Some("codex_cli"), "cli_workspace_not_bound"),
            (None, "tool_policy_executor_mismatch"),
        ] {
            let store = Arc::new(LocalProductStore::new(":memory:").expect("store"));
            let calls = Arc::new(AtomicUsize::new(0));
            let executor = ToolPolicyNodeExecutor::cli(
                Arc::new(CountingExecutor {
                    calls: calls.clone(),
                }),
                store.clone(),
                "codex_cli",
            );
            let mut metadata = json!({
                "profile_id": "default",
                "prompt": "bounded fixture",
            });
            if let Some(configured_executor) = configured_executor {
                metadata["executor"] = json!(configured_executor);
            }

            let result = executor.execute_node(&NodeExecutionInput {
                node_id: "node-cli".to_string(),
                task_type: "codex_cli".to_string(),
                run_id: "run-cli".to_string(),
                workflow_id: "workflow-cli".to_string(),
                node_metadata: metadata,
            });

            assert_eq!(result.status, "failed");
            assert_eq!(result.executor_type, "codex_cli");
            assert_eq!(result.error_domain.as_deref(), Some(expected_domain));
            assert_eq!(calls.load(Ordering::SeqCst), 0);
            assert!(store
                .inspect_tool_execution_authorization("run-cli", "node-cli")
                .expect("inspect authorization")
                .is_none());
        }
    }

    #[test]
    fn managed_cli_revalidates_binding_and_workspace_status_before_inner_effect() {
        let target = tempfile::tempdir().expect("target");
        let workspace = tempfile::tempdir().expect("workspace");
        let target_path = target.path().to_string_lossy().to_string();
        let workspace_path = workspace.path().to_string_lossy().to_string();
        let store = Arc::new(LocalProductStore::new(":memory:").expect("store"));
        store
            .import_supervised_patch_workspace(&json!({
                "schema_version": "supervised_patch_workspace.v1",
                "workspace_id": "managed-binding-workspace",
                "run_id": "source-run",
                "target_id": "target",
                "target_repo_path": target_path,
                "target_repo_canonical_path": target_path,
                "workspace_path": workspace_path,
                "workspace_canonical_path": workspace_path,
                "source_revision": "fixture",
                "status": "requested",
                "metadata_only": true,
                "execution_authority": "disabled",
            }))
            .expect("workspace import");

        let identity = serde_json::to_vec(&json!({
            "schema_version": "managed_supervised_patch_identity.v1",
            "workspace_id": "managed-binding-workspace",
            "operation": "repair",
            "attempt": 1,
        }))
        .expect("identity");
        let run_id = format!("managed-run-{}", hex::encode(Sha256::digest(identity)));
        let mut metadata = json!({
            "profile_id": "supervised_patch_repair",
            "executor": "codex_cli",
            "prompt": "repair the fixture",
            "verification_command": "cargo test -p engine fixture",
            "failure_summary_sha256": "ab".repeat(32),
            "workspace_path": workspace_path,
            "workspace_root": workspace_path,
            "executor_timeout_ms": 1_000,
        });
        let binding_sha256 =
            managed_tool_binding_sha256("managed-binding-workspace", "repair", 1, &metadata)
                .expect("binding");
        metadata["managed_supervised_patch"] = json!({
            "schema_version": "managed_supervised_patch.v1",
            "workspace_id": "managed-binding-workspace",
            "operation": "repair",
            "attempt": 1,
            "binding_sha256": binding_sha256,
            "content_excluded": true,
        });
        let calls = Arc::new(AtomicUsize::new(0));
        let executor = ToolPolicyNodeExecutor::cli(
            Arc::new(CountingExecutor {
                calls: calls.clone(),
            }),
            store.clone(),
            "codex_cli",
        );

        let mut changed_metadata = metadata.clone();
        changed_metadata["prompt"] = json!("different repair");
        let changed = executor.execute_node(&NodeExecutionInput {
            node_id: "supervised-repair-1".to_string(),
            task_type: "codex_cli".to_string(),
            run_id: run_id.clone(),
            workflow_id: "managed-workflow".to_string(),
            node_metadata: changed_metadata,
        });
        assert_eq!(changed.status, "failed");
        assert_eq!(
            changed.error_domain.as_deref(),
            Some("cli_workspace_binding_error")
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        store
            .quarantine_workspace("managed-binding-workspace", "test")
            .expect("quarantine");
        let quarantined = executor.execute_node(&NodeExecutionInput {
            node_id: "supervised-repair-1".to_string(),
            task_type: "codex_cli".to_string(),
            run_id,
            workflow_id: "managed-workflow".to_string(),
            node_metadata: metadata,
        });
        assert_eq!(quarantined.status, "failed");
        assert_eq!(
            quarantined.error_domain.as_deref(),
            Some("cli_workspace_binding_error")
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        let legacy_run_metadata = json!({
            "profile_id": "supervised_patch_repair",
            "executor": "codex_cli",
            "prompt": "repair the fixture",
            "workspace_path": workspace_path,
            "workspace_root": workspace_path,
        });
        let legacy_quarantined = executor.execute_node(&NodeExecutionInput {
            node_id: "legacy-cli-node".to_string(),
            task_type: "codex_cli".to_string(),
            run_id: "source-run".to_string(),
            workflow_id: "legacy-workflow".to_string(),
            node_metadata: legacy_run_metadata,
        });
        assert_eq!(legacy_quarantined.status, "failed");
        assert_eq!(
            legacy_quarantined.error_domain.as_deref(),
            Some("cli_workspace_binding_error")
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn implicit_receipt_prevents_duplicate_unapproved_tool_effect() {
        struct FailingEffectExecutor {
            calls: Arc<AtomicUsize>,
        }

        impl NodeExecutor for FailingEffectExecutor {
            fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
                self.calls.fetch_add(1, Ordering::SeqCst);
                NodeExecutionOutput {
                    status: "failed".to_string(),
                    executor_type: "command".to_string(),
                    output: None,
                    error_domain: Some("command_exit_nonzero".to_string()),
                    error_message: Some("fixture effect may already exist".to_string()),
                    input_tokens: None,
                    output_tokens: None,
                    estimated_cost: None,
                    latency_ms: Some(1),
                    process_outcome: None,
                    resolved_model: None,
                }
            }

            fn executor_type_name(&self) -> &str {
                "command"
            }
        }

        let store = Arc::new(LocalProductStore::new(":memory:").expect("store"));
        let (run_id, workflow_id) = setup_command_run(&store);
        store
            .set_tool_allowlist("locked", &["echo".to_string()])
            .expect("allowlist");
        let calls = Arc::new(AtomicUsize::new(0));
        let executor = ToolPolicyNodeExecutor::command(
            Arc::new(FailingEffectExecutor {
                calls: calls.clone(),
            }),
            store.clone(),
        );
        let input = NodeExecutionInput {
            node_id: "node-tool".to_string(),
            task_type: "command".to_string(),
            run_id,
            workflow_id,
            node_metadata: json!({
                "profile_id": "locked",
                "command": "echo approved"
            }),
        };

        let first = executor.execute_node(&input);
        let duplicate = executor.execute_node(&input);

        assert_eq!(first.status, "failed");
        assert_eq!(
            first.error_domain.as_deref(),
            Some("tool_effect_outcome_unknown")
        );
        assert_eq!(duplicate.status, "failed");
        assert_eq!(
            duplicate.error_domain.as_deref(),
            Some("tool_execution_outcome_unknown")
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let receipt = store
            .inspect_tool_execution_authorization(&input.run_id, &input.node_id)
            .expect("inspect receipt")
            .expect("implicit receipt");
        assert_eq!(receipt["status"], "consumed");
        assert_eq!(receipt["resolved_by"], "tool-policy:implicit");
    }

    #[test]
    fn post_hook_block_audit_failure_preserves_usage_and_marks_effect_unknown() {
        struct AuditBreakingExecutor {
            db_path: std::path::PathBuf,
            calls: Arc<AtomicUsize>,
        }

        impl NodeExecutor for AuditBreakingExecutor {
            fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
                self.calls.fetch_add(1, Ordering::SeqCst);
                let connection = rusqlite::Connection::open(&self.db_path).expect("open fixture");
                connection
                    .execute("DROP TABLE audit_log", [])
                    .expect("break audit persistence");
                NodeExecutionOutput {
                    status: "completed".to_string(),
                    executor_type: "command".to_string(),
                    output: Some("effect already happened".to_string()),
                    error_domain: None,
                    error_message: None,
                    input_tokens: Some(17),
                    output_tokens: Some(5),
                    estimated_cost: Some(0.25),
                    latency_ms: Some(9),
                    process_outcome: None,
                    resolved_model: None,
                }
            }

            fn executor_type_name(&self) -> &str {
                "command"
            }
        }

        let directory = tempfile::tempdir().expect("tempdir");
        let db_path = directory.path().join("tool-policy-audit.db");
        let store = Arc::new(LocalProductStore::new(&db_path).expect("store"));
        let (run_id, workflow_id) = setup_command_run(&store);
        store
            .set_tool_allowlist("locked", &["echo".to_string()])
            .expect("allowlist");
        store
            .add_tool_hook(
                "post-block",
                "post_execution",
                Some("echo"),
                None,
                "block",
                Some(&json!({"reason": "reject fixture result"})),
            )
            .expect("post hook");
        let calls = Arc::new(AtomicUsize::new(0));
        let executor = ToolPolicyNodeExecutor::command(
            Arc::new(AuditBreakingExecutor {
                db_path,
                calls: calls.clone(),
            }),
            store,
        );

        let result = executor.execute_node(&NodeExecutionInput {
            node_id: "node-tool".to_string(),
            task_type: "command".to_string(),
            run_id,
            workflow_id,
            node_metadata: json!({
                "profile_id": "locked",
                "command": "echo invoked"
            }),
        });

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(result.status, "failed");
        assert_eq!(
            result.error_domain.as_deref(),
            Some("tool_effect_outcome_unknown")
        );
        assert!(result
            .error_message
            .as_deref()
            .is_some_and(|message| message.contains("tool effect outcome is unknown")));
        assert_eq!(result.input_tokens, Some(17));
        assert_eq!(result.output_tokens, Some(5));
        assert_eq!(result.estimated_cost, Some(0.25));
        assert_eq!(result.latency_ms, Some(9));
    }

    #[test]
    fn allowlist_revocation_committed_before_receipt_claim_blocks_effect() {
        let store = Arc::new(LocalProductStore::new(":memory:").expect("store"));
        let (run_id, workflow_id) = setup_command_run(&store);
        store
            .configure_tool_capability(
                "operator", "echo", "fixture", None, None, false, "medium", None,
            )
            .expect("capability");
        store
            .configure_tool_allowlist("operator", "locked", &["echo".to_string()], None)
            .expect("allowlist");
        let calls = Arc::new(AtomicUsize::new(0));
        let mutation_store = store.clone();
        let executor = ToolPolicyNodeExecutor::command(
            Arc::new(CountingExecutor {
                calls: calls.clone(),
            }),
            store.clone(),
        )
        .with_before_authorization_claim_for_test(move || {
            let current = mutation_store
                .read_tool_allowlist_policy("locked")
                .expect("read current allowlist")
                .expect("configured allowlist");
            mutation_store
                .configure_tool_allowlist(
                    "operator",
                    "locked",
                    &[],
                    current["resource_sha256"].as_str(),
                )
                .expect("commit allowlist revocation");
        });

        let result = executor.execute_node(&NodeExecutionInput {
            node_id: "node-tool".to_string(),
            task_type: "command".to_string(),
            run_id: run_id.clone(),
            workflow_id,
            node_metadata: json!({
                "profile_id": "locked",
                "command": "echo approved"
            }),
        });

        assert_eq!(result.status, "failed");
        assert_eq!(
            result.error_domain.as_deref(),
            Some("tool_execution_receipt_error")
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(store
            .inspect_tool_execution_authorization(&run_id, "node-tool")
            .expect("inspect authorization")
            .is_none());
    }

    #[test]
    fn block_or_approval_hook_committed_before_receipt_claim_blocks_effect() {
        for action in ["block", "request_approval"] {
            let store = Arc::new(LocalProductStore::new(":memory:").expect("store"));
            let (run_id, workflow_id) = setup_command_run(&store);
            store
                .configure_tool_capability(
                    "operator", "echo", "fixture", None, None, false, "medium", None,
                )
                .expect("capability");
            store
                .configure_tool_allowlist("operator", "locked", &["echo".to_string()], None)
                .expect("allowlist");
            let calls = Arc::new(AtomicUsize::new(0));
            let mutation_store = store.clone();
            let hook_id = format!("concurrent-{action}");
            let executor = ToolPolicyNodeExecutor::command(
                Arc::new(CountingExecutor {
                    calls: calls.clone(),
                }),
                store.clone(),
            )
            .with_before_authorization_claim_for_test(move || {
                mutation_store
                    .configure_tool_hook(
                        "operator",
                        &hook_id,
                        "pre_execution",
                        Some("echo"),
                        None,
                        action,
                        Some(&json!({"reason": "concurrent policy change"})),
                        true,
                        None,
                    )
                    .expect("commit hook policy");
            });

            let result = executor.execute_node(&NodeExecutionInput {
                node_id: "node-tool".to_string(),
                task_type: "command".to_string(),
                run_id: run_id.clone(),
                workflow_id,
                node_metadata: json!({
                    "profile_id": "locked",
                    "command": "echo approved"
                }),
            });

            assert_eq!(result.status, "failed", "action={action}");
            assert_eq!(
                result.error_domain.as_deref(),
                Some("tool_execution_receipt_error"),
                "action={action}"
            );
            assert_eq!(calls.load(Ordering::SeqCst), 0, "action={action}");
            assert!(store
                .inspect_tool_execution_authorization(&run_id, "node-tool")
                .expect("inspect authorization")
                .is_none());
        }
    }
}

#[cfg(all(test, feature = "pg-tests"))]
mod pg_tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingPgExecutor {
        calls: Arc<AtomicUsize>,
    }

    impl NodeExecutor for CountingPgExecutor {
        fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
            self.calls.fetch_add(1, Ordering::SeqCst);
            NodeExecutionOutput {
                status: "completed".to_string(),
                executor_type: "command".to_string(),
                output: Some("fixture completed".to_string()),
                error_domain: None,
                error_message: None,
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(1),
                process_outcome: None,
                resolved_model: None,
            }
        }

        fn executor_type_name(&self) -> &str {
            "command"
        }
    }

    fn pg_store() -> Option<Arc<LocalProductStore>> {
        let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
            eprintln!("ACP_TEST_DATABASE_URL not set; skipping pg-tests");
            return None;
        };
        Some(Arc::new(
            LocalProductStore::new_postgres(&url, || {
                chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
            })
            .expect("PostgreSQL store"),
        ))
    }

    fn setup_pg_command_run(
        store: &LocalProductStore,
        profile_id: &str,
        tool_name: &str,
    ) -> (String, String, String) {
        let node_id = format!("node-{tool_name}");
        let command = format!("{tool_name} bounded");
        let plan = store
            .create_workflow_plan("PG tool policy test", "test", "actor", |ids, _| {
                Ok(json!({
                    "schema_version": "read_only_plan.v1",
                    "plan_id": ids.plan_id,
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "analysis": {"analysis_id": "pg-tool-policy", "task_domain": "test"},
                    "graph": {
                        "schema_version": "workflow_graph.v1",
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "status": "decomposed",
                        "created_at": "2026-07-14T00:00:00Z",
                        "updated_at": "2026-07-14T00:00:00Z",
                        "nodes": [{
                            "schema_version": "workflow_node.v1",
                            "node_id": node_id,
                            "workflow_id": ids.workflow_id,
                            "task_type": "command",
                            "status": "pending",
                            "profile_id": profile_id,
                            "command": command,
                        }],
                        "edges": [],
                    },
                    "boundaries": {"execution_authority": "bounded_trusted_local"},
                }))
            })
            .expect("create PG tool plan");
        let run = store
            .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
            .expect("create PG tool run");
        (
            run["run_id"].as_str().unwrap().to_string(),
            plan["workflow_id"].as_str().unwrap().to_string(),
            node_id,
        )
    }

    #[test]
    fn postgres_policy_mutations_committed_before_claim_prevent_tool_effect() {
        let Some(store) = pg_store() else {
            return;
        };

        for mutation in ["revoke", "block", "request_approval"] {
            let tag = uuid::Uuid::new_v4().simple().to_string();
            let tool_name = format!("tool_{tag}");
            let profile_id = format!("profile_{tag}");
            store
                .configure_tool_capability(
                    "operator",
                    &tool_name,
                    "bounded PG fixture",
                    None,
                    None,
                    false,
                    "medium",
                    None,
                )
                .expect("configure PG capability");
            store
                .configure_tool_allowlist(
                    "operator",
                    &profile_id,
                    std::slice::from_ref(&tool_name),
                    None,
                )
                .expect("configure PG allowlist");
            let (run_id, workflow_id, node_id) =
                setup_pg_command_run(&store, &profile_id, &tool_name);
            let calls = Arc::new(AtomicUsize::new(0));
            let mutation_store = store.clone();
            let mutation_profile = profile_id.clone();
            let mutation_tool = tool_name.clone();
            let hook_id = format!("hook_{mutation}_{tag}");
            let executor = ToolPolicyNodeExecutor::command(
                Arc::new(CountingPgExecutor {
                    calls: calls.clone(),
                }),
                store.clone(),
            )
            .with_before_authorization_claim_for_test(move || {
                if mutation == "revoke" {
                    let current = mutation_store
                        .read_tool_allowlist_policy(&mutation_profile)
                        .expect("read PG allowlist")
                        .expect("configured PG allowlist");
                    mutation_store
                        .configure_tool_allowlist(
                            "operator",
                            &mutation_profile,
                            &[],
                            current["resource_sha256"].as_str(),
                        )
                        .expect("commit PG allowlist revocation");
                } else {
                    mutation_store
                        .configure_tool_hook(
                            "operator",
                            &hook_id,
                            "pre_execution",
                            Some(&mutation_tool),
                            None,
                            mutation,
                            Some(&json!({"reason": "committed PG policy change"})),
                            true,
                            None,
                        )
                        .expect("commit PG hook mutation");
                }
            });

            let result = executor.execute_node(&NodeExecutionInput {
                node_id: node_id.clone(),
                task_type: "command".to_string(),
                run_id: run_id.clone(),
                workflow_id,
                node_metadata: json!({
                    "profile_id": profile_id,
                    "command": format!("{tool_name} bounded"),
                }),
            });

            assert_eq!(result.status, "failed", "mutation={mutation}");
            assert_eq!(
                result.error_domain.as_deref(),
                Some("tool_execution_receipt_error"),
                "mutation={mutation}"
            );
            assert_eq!(calls.load(Ordering::SeqCst), 0, "mutation={mutation}");
            assert!(store
                .inspect_tool_execution_authorization(&run_id, &node_id)
                .expect("inspect PG authorization")
                .is_none());
        }
    }
}
