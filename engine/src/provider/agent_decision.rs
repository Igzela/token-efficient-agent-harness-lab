use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::node_executor::{
    allowed_agent_action_types, AgentAction, AgentDecision, AgentDecisionUsage, AgentStepContext,
    MeasuredAgentDecisionFn,
};
use crate::orchestration::schemas::{
    ChildTaskProposal, DebatePosition, DebateRequest, DebateResolution, HandoffRequest,
    ReviewRequest, ReviewVerdict,
};
use crate::provider::config::ProviderPricingConfig;
use crate::provider::executor::invoke_provider_blocking;
use crate::provider::redaction::redact_sensitive_patterns;
use crate::provider::{CostGateConfig, Provider, ProviderAuditRecorder, ProviderRequest};
use crate::storage::local_product_store::LocalProductStore;

pub const AGENT_ACTION_SCHEMA_VERSION: &str = "agent_action.v1";
pub const MAX_AGENT_ACTION_RESPONSE_BYTES: usize = 16 * 1024;
const MAX_AGENT_DECISION_PROMPT_BYTES: usize = 32 * 1024;
const MAX_AGENT_OBJECTIVE_BYTES: usize = 4096;
const MAX_OBSERVATION_SUMMARY_BYTES: usize = 512;
const MAX_OBSERVATION_SOURCES: usize = 8;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentActionEnvelope {
    schema_version: String,
    action: AgentActionWire,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum AgentActionWire {
    Wait {},
    Complete {},
    UpdateScratchpadSummary { summary: String },
    ReadMailbox {},
    AckMessage { message_id: String },
    EmitNote { note: String },
    RecordObservation { observation: String },
    ProposeChildTask { proposal: ChildTaskProposal },
    RequestHandoff { request: HandoffRequest },
    AcceptHandoff { correlation_id: String },
    RejectHandoff { correlation_id: String },
    CancelProposal { correlation_id: String },
    RequestReview { request: ReviewRequest },
    SubmitReviewVerdict { verdict: ReviewVerdict },
    OpenDebate { request: DebateRequest },
    SubmitDebatePosition { position: DebatePosition },
    ResolveDebate { resolution: DebateResolution },
}

pub fn parse_agent_action_response(response: &str) -> Result<AgentAction, String> {
    if response.len() > MAX_AGENT_ACTION_RESPONSE_BYTES {
        return Err(format!(
            "agent action response exceeds {MAX_AGENT_ACTION_RESPONSE_BYTES} byte cap"
        ));
    }
    let envelope: AgentActionEnvelope = serde_json::from_str(response)
        .map_err(|error| format!("invalid agent action response: {error}"))?;
    if envelope.schema_version != AGENT_ACTION_SCHEMA_VERSION {
        return Err(format!(
            "invalid agent action schema_version: {}",
            envelope.schema_version
        ));
    }
    Ok(match envelope.action {
        AgentActionWire::Wait {} => AgentAction::Wait,
        AgentActionWire::Complete {} => AgentAction::Complete,
        AgentActionWire::UpdateScratchpadSummary { summary } => {
            AgentAction::UpdateScratchpadSummary(summary)
        }
        AgentActionWire::ReadMailbox {} => AgentAction::ReadMailbox,
        AgentActionWire::AckMessage { message_id } => AgentAction::AckMessage(message_id),
        AgentActionWire::EmitNote { note } => AgentAction::EmitNote(note),
        AgentActionWire::RecordObservation { observation } => {
            AgentAction::RecordObservation(observation)
        }
        AgentActionWire::ProposeChildTask { proposal } => AgentAction::ProposeChildTask(proposal),
        AgentActionWire::RequestHandoff { request } => AgentAction::RequestHandoff(request),
        AgentActionWire::AcceptHandoff { correlation_id } => {
            AgentAction::AcceptHandoff(correlation_id)
        }
        AgentActionWire::RejectHandoff { correlation_id } => {
            AgentAction::RejectHandoff(correlation_id)
        }
        AgentActionWire::CancelProposal { correlation_id } => {
            AgentAction::CancelProposal(correlation_id)
        }
        AgentActionWire::RequestReview { request } => AgentAction::RequestReview(request),
        AgentActionWire::SubmitReviewVerdict { verdict } => {
            AgentAction::SubmitReviewVerdict(verdict)
        }
        AgentActionWire::OpenDebate { request } => AgentAction::OpenDebate(request),
        AgentActionWire::SubmitDebatePosition { position } => {
            AgentAction::SubmitDebatePosition(position)
        }
        AgentActionWire::ResolveDebate { resolution } => AgentAction::ResolveDebate(resolution),
    })
}

fn bounded_redacted_summary(value: &str) -> String {
    let mut value = redact_sensitive_patterns(value);
    if value.len() > MAX_OBSERVATION_SUMMARY_BYTES {
        let mut boundary = MAX_OBSERVATION_SUMMARY_BYTES;
        while boundary > 0 && !value.is_char_boundary(boundary) {
            boundary -= 1;
        }
        value.truncate(boundary);
    }
    value
}

fn bounded_observation_identifier(value: &str) -> Option<&str> {
    if value.is_empty()
        || value.len() > 256
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return None;
    }
    Some(value)
}

fn agent_action_type(action: &AgentAction) -> &'static str {
    match action {
        AgentAction::Wait => "wait",
        AgentAction::Complete => "complete",
        AgentAction::UpdateScratchpadSummary(_) => "update_scratchpad_summary",
        AgentAction::ReadMailbox => "read_mailbox",
        AgentAction::AckMessage(_) => "ack_message",
        AgentAction::EmitNote(_) => "emit_note",
        AgentAction::RecordObservation(_) => "record_observation",
        AgentAction::Unsupported(_) => "unsupported",
        AgentAction::ProposeChildTask(_) => "propose_child_task",
        AgentAction::RequestHandoff(_) => "request_handoff",
        AgentAction::AcceptHandoff(_) => "accept_handoff",
        AgentAction::RejectHandoff(_) => "reject_handoff",
        AgentAction::CancelProposal(_) => "cancel_proposal",
        AgentAction::RequestReview(_) => "request_review",
        AgentAction::SubmitReviewVerdict(_) => "submit_review_verdict",
        AgentAction::OpenDebate(_) => "open_debate",
        AgentAction::SubmitDebatePosition(_) => "submit_debate_position",
        AgentAction::ResolveDebate(_) => "resolve_debate",
    }
}

fn predecessor_action_observations(context: &AgentStepContext) -> Vec<serde_json::Value> {
    let Some(sources) = context
        .node_metadata
        .pointer("/context_injection/sources")
        .and_then(serde_json::Value::as_array)
    else {
        return Vec::new();
    };
    sources
        .iter()
        .take(MAX_OBSERVATION_SOURCES)
        .filter_map(|source| {
            let output = source.get("output")?;
            let parsed = match output {
                serde_json::Value::String(text) => serde_json::from_str(text).ok()?,
                value => value.clone(),
            };
            let action = parsed.get("action")?.as_str()?;
            let mut observation = serde_json::Map::new();
            observation.insert(
                "source_node_id".to_string(),
                source.get("from_node_id").cloned().unwrap_or(serde_json::Value::Null),
            );
            observation.insert("action".to_string(), json!(action));
            for field in [
                "message_id",
                "correlation_id",
                "proposal_id",
                "handoff_id",
                "review_id",
                "debate_id",
                "resolution_proposal_id",
                "target_agent_id",
            ] {
                if let Some(value) = parsed.get(field).and_then(serde_json::Value::as_str) {
                    if value.len() <= 256 {
                        observation.insert(field.to_string(), json!(value));
                    }
                }
            }
            if let Some(messages) = parsed.get("messages").and_then(serde_json::Value::as_array) {
                let messages = messages
                    .iter()
                    .take(10)
                    .filter_map(|message| {
                        let message_id = bounded_observation_identifier(
                            message.get("message_id")?.as_str()?,
                        )?;
                        Some(json!({
                            "message_id": message_id,
                            "correlation_id": message
                                .get("correlation_id")
                                .and_then(serde_json::Value::as_str)
                                .and_then(bounded_observation_identifier),
                            "from_agent_id": message
                                .get("from")
                                .and_then(serde_json::Value::as_str)
                                .and_then(bounded_observation_identifier),
                            "node_id": message
                                .get("node_id")
                                .and_then(serde_json::Value::as_str)
                                .and_then(bounded_observation_identifier),
                            "message_type": message.get("type").and_then(serde_json::Value::as_str),
                            "proposal_id": message
                                .get("proposal_id")
                                .and_then(serde_json::Value::as_str)
                                .and_then(bounded_observation_identifier),
                            "summary": message.get("summary").and_then(serde_json::Value::as_str).map(bounded_redacted_summary),
                        }))
                    })
                    .collect::<Vec<_>>();
                observation.insert("messages".to_string(), serde_json::Value::Array(messages));
            }
            Some(serde_json::Value::Object(observation))
        })
        .collect()
}

fn agent_action_response_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "additionalProperties": false,
        "required": ["schema_version", "action"],
        "properties": {
            "schema_version": {"const": AGENT_ACTION_SCHEMA_VERSION},
            "action": {
                "oneOf": [
                    {"type":"object","additionalProperties":false,"required":["type"],"properties":{"type":{"const":"wait"}}},
                    {"type":"object","additionalProperties":false,"required":["type"],"properties":{"type":{"const":"complete"}}},
                    {"type":"object","additionalProperties":false,"required":["type","summary"],"properties":{"type":{"const":"update_scratchpad_summary"},"summary":{"$ref":"#/$defs/bounded_text"}}},
                    {"type":"object","additionalProperties":false,"required":["type"],"properties":{"type":{"const":"read_mailbox"}}},
                    {"type":"object","additionalProperties":false,"required":["type","message_id"],"properties":{"type":{"const":"ack_message"},"message_id":{"$ref":"#/$defs/identifier"}}},
                    {"type":"object","additionalProperties":false,"required":["type","note"],"properties":{"type":{"const":"emit_note"},"note":{"$ref":"#/$defs/bounded_text"}}},
                    {"type":"object","additionalProperties":false,"required":["type","observation"],"properties":{"type":{"const":"record_observation"},"observation":{"$ref":"#/$defs/bounded_text"}}},
                    {"type":"object","additionalProperties":false,"required":["type","proposal"],"properties":{"type":{"const":"propose_child_task"},"proposal":{"$ref":"#/$defs/child_task_proposal"}}},
                    {"type":"object","additionalProperties":false,"required":["type","request"],"properties":{"type":{"const":"request_handoff"},"request":{"$ref":"#/$defs/handoff_request"}}},
                    {"type":"object","additionalProperties":false,"required":["type","correlation_id"],"properties":{"type":{"const":"accept_handoff"},"correlation_id":{"$ref":"#/$defs/identifier"}}},
                    {"type":"object","additionalProperties":false,"required":["type","correlation_id"],"properties":{"type":{"const":"reject_handoff"},"correlation_id":{"$ref":"#/$defs/identifier"}}},
                    {"type":"object","additionalProperties":false,"required":["type","correlation_id"],"properties":{"type":{"const":"cancel_proposal"},"correlation_id":{"$ref":"#/$defs/identifier"}}},
                    {"type":"object","additionalProperties":false,"required":["type","request"],"properties":{"type":{"const":"request_review"},"request":{"$ref":"#/$defs/review_request"}}},
                    {"type":"object","additionalProperties":false,"required":["type","verdict"],"properties":{"type":{"const":"submit_review_verdict"},"verdict":{"$ref":"#/$defs/review_verdict"}}},
                    {"type":"object","additionalProperties":false,"required":["type","request"],"properties":{"type":{"const":"open_debate"},"request":{"$ref":"#/$defs/debate_request"}}},
                    {"type":"object","additionalProperties":false,"required":["type","position"],"properties":{"type":{"const":"submit_debate_position"},"position":{"$ref":"#/$defs/debate_position"}}},
                    {"type":"object","additionalProperties":false,"required":["type","resolution"],"properties":{"type":{"const":"resolve_debate"},"resolution":{"$ref":"#/$defs/debate_resolution"}}}
                ]
            }
        },
        "$defs": {
            "identifier": {
                "type": "string",
                "minLength": 1,
                "maxLength": 256,
                "pattern": "^[A-Za-z0-9._:-]+$"
            },
            "bounded_text": {"type":"string","maxLength":4096},
            "optional_identifier": {"type":["string","null"],"minLength":1,"maxLength":256,"pattern":"^[A-Za-z0-9._:-]+$"},
            "optional_text": {"type":["string","null"],"maxLength":4096},
            "child_task_proposal": {
                "type":"object","additionalProperties":false,
                "required":["schema_version","correlation_id","objective","context_summary","parent_node_id","run_id","agent_id"],
                "properties":{
                    "schema_version":{"const":"child_task_proposal.v1"},
                    "correlation_id":{"$ref":"#/$defs/identifier"},
                    "objective":{"$ref":"#/$defs/bounded_text"},
                    "context_summary":{"$ref":"#/$defs/bounded_text"},
                    "proposed_node_id":{"$ref":"#/$defs/optional_identifier"},
                    "proposed_edge_id":{"$ref":"#/$defs/optional_identifier"},
                    "parent_node_id":{"$ref":"#/$defs/identifier"},
                    "run_id":{"$ref":"#/$defs/identifier"},
                    "agent_id":{"$ref":"#/$defs/identifier"}
                }
            },
            "handoff_request": {
                "type":"object","additionalProperties":false,
                "required":["schema_version","correlation_id","objective","context_summary","target_agent_id","source_agent_id","run_id","node_id"],
                "properties":{
                    "schema_version":{"const":"handoff_request.v1"},
                    "correlation_id":{"$ref":"#/$defs/identifier"},
                    "objective":{"$ref":"#/$defs/bounded_text"},
                    "context_summary":{"$ref":"#/$defs/bounded_text"},
                    "target_agent_id":{"$ref":"#/$defs/identifier"},
                    "source_agent_id":{"$ref":"#/$defs/identifier"},
                    "run_id":{"$ref":"#/$defs/identifier"},
                    "node_id":{"$ref":"#/$defs/identifier"}
                }
            },
            "review_request": {
                "type":"object","additionalProperties":false,
                "required":["schema_version","correlation_id","subject_summary","rationale_summary","target_agent_id","run_id","node_id","blocking"],
                "properties":{
                    "schema_version":{"const":"review_request.v1"},
                    "correlation_id":{"$ref":"#/$defs/identifier"},
                    "subject_summary":{"$ref":"#/$defs/bounded_text"},
                    "rationale_summary":{"$ref":"#/$defs/bounded_text"},
                    "target_agent_id":{"$ref":"#/$defs/identifier"},
                    "run_id":{"$ref":"#/$defs/identifier"},
                    "node_id":{"$ref":"#/$defs/identifier"},
                    "blocking":{"type":"boolean"}
                }
            },
            "review_verdict": {
                "type":"object","additionalProperties":false,
                "required":["schema_version","correlation_id","review_request_id","verdict","rationale_summary","run_id","node_id","blocking"],
                "properties":{
                    "schema_version":{"const":"review_verdict.v1"},
                    "correlation_id":{"$ref":"#/$defs/identifier"},
                    "review_request_id":{"$ref":"#/$defs/identifier"},
                    "verdict":{"enum":["accepted","rejected"]},
                    "rationale_summary":{"$ref":"#/$defs/bounded_text"},
                    "run_id":{"$ref":"#/$defs/identifier"},
                    "node_id":{"$ref":"#/$defs/identifier"},
                    "blocking":{"type":"boolean"}
                }
            },
            "debate_request": {
                "type":"object","additionalProperties":false,
                "required":["schema_version","correlation_id","subject_summary","participant_agent_ids","max_rounds","run_id","node_id"],
                "properties":{
                    "schema_version":{"const":"debate_request.v1"},
                    "correlation_id":{"$ref":"#/$defs/identifier"},
                    "subject_summary":{"$ref":"#/$defs/bounded_text"},
                    "participant_agent_ids":{"type":"array","minItems":1,"maxItems":8,"uniqueItems":true,"items":{"$ref":"#/$defs/identifier"}},
                    "max_rounds":{"type":"integer","minimum":1,"maximum":10},
                    "run_id":{"$ref":"#/$defs/identifier"},
                    "node_id":{"$ref":"#/$defs/identifier"}
                }
            },
            "debate_position": {
                "type":"object","additionalProperties":false,
                "required":["schema_version","correlation_id","debate_id","position","rationale_summary","run_id","node_id"],
                "properties":{
                    "schema_version":{"const":"debate_position.v1"},
                    "correlation_id":{"$ref":"#/$defs/identifier"},
                    "debate_id":{"$ref":"#/$defs/identifier"},
                    "position":{"$ref":"#/$defs/bounded_text"},
                    "rationale_summary":{"$ref":"#/$defs/bounded_text"},
                    "run_id":{"$ref":"#/$defs/identifier"},
                    "node_id":{"$ref":"#/$defs/identifier"}
                }
            },
            "debate_resolution": {
                "type":"object","additionalProperties":false,
                "required":["schema_version","correlation_id","debate_id","resolution","run_id","node_id"],
                "properties":{
                    "schema_version":{"const":"debate_resolution.v1"},
                    "correlation_id":{"$ref":"#/$defs/identifier"},
                    "debate_id":{"$ref":"#/$defs/identifier"},
                    "resolution":{"$ref":"#/$defs/bounded_text"},
                    "winning_position":{"$ref":"#/$defs/optional_text"},
                    "unresolved_risks":{"$ref":"#/$defs/optional_text"},
                    "run_id":{"$ref":"#/$defs/identifier"},
                    "node_id":{"$ref":"#/$defs/identifier"}
                }
            }
        }
    })
}

fn agent_action_typed_examples(context: &AgentStepContext) -> Vec<Value> {
    let envelope =
        |action: Value| json!({"schema_version": AGENT_ACTION_SCHEMA_VERSION, "action": action});
    vec![
        envelope(json!({"type":"wait"})),
        envelope(json!({"type":"complete"})),
        envelope(json!({"type":"update_scratchpad_summary","summary":"bounded summary"})),
        envelope(json!({"type":"read_mailbox"})),
        envelope(json!({"type":"ack_message","message_id":"message-1"})),
        envelope(json!({"type":"emit_note","note":"bounded note"})),
        envelope(json!({"type":"record_observation","observation":"bounded observation"})),
        envelope(json!({"type":"propose_child_task","proposal":{
            "schema_version":"child_task_proposal.v1","correlation_id":"correlation-child-1",
            "objective":"bounded objective","context_summary":"bounded context",
            "parent_node_id":context.node_id,"run_id":context.run_id,"agent_id":context.agent_id
        }})),
        envelope(json!({"type":"request_handoff","request":{
            "schema_version":"handoff_request.v1","correlation_id":"correlation-handoff-1",
            "objective":"bounded objective","context_summary":"bounded context","target_agent_id":"agent-peer",
            "source_agent_id":context.agent_id,"run_id":context.run_id,"node_id":context.node_id
        }})),
        envelope(json!({"type":"accept_handoff","correlation_id":"correlation-handoff-1"})),
        envelope(json!({"type":"reject_handoff","correlation_id":"correlation-handoff-1"})),
        envelope(json!({"type":"cancel_proposal","correlation_id":"correlation-proposal-1"})),
        envelope(json!({"type":"request_review","request":{
            "schema_version":"review_request.v1","correlation_id":"correlation-review-1",
            "subject_summary":"bounded subject","rationale_summary":"bounded rationale","target_agent_id":"agent-reviewer",
            "run_id":context.run_id,"node_id":context.node_id,"blocking":true
        }})),
        envelope(json!({"type":"submit_review_verdict","verdict":{
            "schema_version":"review_verdict.v1","correlation_id":"correlation-review-1","review_request_id":"review-request-1",
            "verdict":"accepted","rationale_summary":"bounded rationale","run_id":context.run_id,
            "node_id":context.node_id,"blocking":true
        }})),
        envelope(json!({"type":"open_debate","request":{
            "schema_version":"debate_request.v1","correlation_id":"correlation-debate-1","subject_summary":"bounded subject",
            "participant_agent_ids":["agent-peer"],"max_rounds":1,"run_id":context.run_id,"node_id":context.node_id
        }})),
        envelope(json!({"type":"submit_debate_position","position":{
            "schema_version":"debate_position.v1","correlation_id":"correlation-debate-1","debate_id":"debate-1",
            "position":"bounded position","rationale_summary":"bounded rationale","run_id":context.run_id,"node_id":context.node_id
        }})),
        envelope(json!({"type":"resolve_debate","resolution":{
            "schema_version":"debate_resolution.v1","correlation_id":"correlation-debate-1","debate_id":"debate-1",
            "resolution":"bounded resolution","winning_position":"bounded position","unresolved_risks":null,
            "run_id":context.run_id,"node_id":context.node_id
        }})),
    ]
}

fn build_agent_decision_prompt(
    context: &AgentStepContext,
    store: &LocalProductStore,
) -> Result<String, String> {
    let state = context
        .agent_state
        .as_ref()
        .ok_or_else(|| "agent state is required for provider decisions".to_string())?;
    if state.agent_id != context.agent_id || state.run_id != context.run_id {
        return Err("agent state scope does not match provider decision context".to_string());
    }
    if state
        .objective
        .as_deref()
        .is_some_and(|value| value.len() > MAX_AGENT_OBJECTIVE_BYTES)
    {
        return Err("agent objective exceeds provider prompt byte cap".to_string());
    }
    if state
        .objective
        .as_deref()
        .is_some_and(crate::provider::redaction::contains_sensitive_patterns)
    {
        return Err("agent objective contains secret-shaped content".to_string());
    }
    let allowed_actions = allowed_agent_action_types(state);
    let agent_state = json!({
        "role": state.role,
        "capability_profile": state.capability_profile,
        "objective": state.objective,
        "status": state.status,
        "scratchpad_summary": state.scratchpad_summary,
    });
    let mailbox = store
        .list_mailbox(
            Some(&context.agent_id),
            Some(&context.run_id),
            None,
            Some("pending"),
            10,
            0,
        )?
        .into_iter()
        .map(|message| {
            let proposal_id = message
                .metadata
                .get("proposal_id")
                .and_then(Value::as_str)
                .and_then(bounded_observation_identifier);
            json!({
                "message_id": bounded_observation_identifier(&message.message_id),
                "correlation_id": message.correlation_id.as_deref().and_then(bounded_observation_identifier),
                "from_agent_id": bounded_observation_identifier(&message.from_agent_id),
                "node_id": message.node_id.as_deref().and_then(bounded_observation_identifier),
                "message_type": message.message_type,
                "proposal_id": proposal_id,
                "summary": message.body_summary.as_deref().map(bounded_redacted_summary),
            })
        })
        .collect::<Vec<_>>();
    let prompt = json!({
        "instruction": "Choose exactly one bounded action. action.type MUST be listed in allowed_actions. Return JSON only; do not add prose or markdown.",
        "allowed_actions": allowed_actions,
        "response_schema": agent_action_response_schema(),
        "typed_examples": agent_action_typed_examples(context),
        "scope": {
            "agent_id": context.agent_id,
            "run_id": context.run_id,
            "node_id": context.node_id,
            "workflow_id": context.workflow_id,
        },
        "agent_state": agent_state,
        "mailbox_pending_count": context.mailbox_pending_count,
        "mailbox_observations": mailbox,
        "predecessor_action_observations": predecessor_action_observations(context),
        "memory_digest": context.memory_digest,
        "memory_context": context.memory_context,
    })
    .to_string();
    if prompt.len() > MAX_AGENT_DECISION_PROMPT_BYTES {
        return Err(format!(
            "agent decision prompt exceeds {MAX_AGENT_DECISION_PROMPT_BYTES} byte cap"
        ));
    }
    if crate::provider::redaction::contains_sensitive_patterns(&prompt) {
        return Err("agent decision prompt contains secret-shaped content".to_string());
    }
    Ok(prompt)
}

fn reserved_agent_decision_cost(
    prompt: &str,
    pricing: &ProviderPricingConfig,
) -> Result<f64, String> {
    let (Some(input_rate), Some(output_rate)) =
        (pricing.input_cost_per_1k, pricing.output_cost_per_1k)
    else {
        return Err("agent decision provider requires explicit pricing".to_string());
    };
    if input_rate <= 0.0 || output_rate <= 0.0 {
        return Err("agent decision provider pricing must be positive".to_string());
    }
    let input_tokens = (prompt.len() as f64 / 4.0).ceil().max(1.0);
    let output_tokens = 1024.0;
    Ok((input_tokens / 1000.0 * input_rate) + (output_tokens / 1000.0 * output_rate))
}

pub fn provider_agent_decision_fn(
    provider: Arc<dyn Provider>,
    store: Arc<LocalProductStore>,
    audit_recorder: Arc<ProviderAuditRecorder>,
    cost_gate_config: CostGateConfig,
    pricing: ProviderPricingConfig,
    provider_execution_enabled: bool,
) -> MeasuredAgentDecisionFn {
    Box::new(move |context| {
        if !provider_execution_enabled {
            return Err("provider execution is not enabled for agent decisions".to_string());
        }
        if !provider.is_enabled() {
            return Err("provider is disabled for agent decisions".to_string());
        }

        let requested_model = context
            .node_metadata
            .get("model")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "agent decision requires an explicit node model".to_string())?;
        let provider_model = provider
            .default_model()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "agent decision provider requires a default model".to_string())?;
        if requested_model != provider_model {
            return Err(format!(
                "agent decision node model '{requested_model}' does not match provider model '{provider_model}'"
            ));
        }
        let model = requested_model.to_string();

        let prompt = build_agent_decision_prompt(context, &store)?;
        let reserved_cost = reserved_agent_decision_cost(&prompt, &pricing)?;
        let per_call_cap = cost_gate_config
            .per_dispatch_cap_usd
            .filter(|value| value.is_finite() && *value > 0.0)
            .ok_or_else(|| {
                "agent decision provider requires a positive per-call cap".to_string()
            })?;
        let daily_cap = cost_gate_config
            .daily_cap_usd
            .filter(|value| value.is_finite() && *value > 0.0)
            .ok_or_else(|| "agent decision provider requires a positive daily cap".to_string())?;

        let attempt = context
            .node_metadata
            .get("execution_attempt")
            .and_then(Value::as_i64)
            .filter(|value| *value > 0)
            .ok_or_else(|| "agent decision requires a positive scheduler attempt".to_string())?;
        let dispatch_ref = format!(
            "workflow:{}:{}:attempt-{}:agent-decision",
            context.run_id, context.node_id, attempt
        );
        audit_recorder.try_reserve_cost(
            &dispatch_ref,
            provider.provider_id(),
            reserved_cost,
            per_call_cap,
            daily_cap,
        )?;
        audit_recorder.try_create_and_record(
            &dispatch_ref,
            provider.provider_id(),
            "request_sent",
            Some(&json!({"redaction_status": "redacted"})),
        )?;
        let request = ProviderRequest {
            schema_version: "provider_request.v1".to_string(),
            provider_id: provider.provider_id().to_string(),
            model,
            prompt,
            metadata: json!({
                "run_id": context.run_id,
                "node_id": context.node_id,
                "workflow_id": context.workflow_id,
                "agent_id": context.agent_id,
                "max_tokens": 1024,
                "reserved_cost_usd": reserved_cost,
                "response_schema": AGENT_ACTION_SCHEMA_VERSION,
                "dispatch_id": dispatch_ref,
            }),
        };

        match invoke_provider_blocking(provider.clone(), &request) {
            Ok(response) => {
                if response.provider_id != provider.provider_id() || response.model != request.model
                {
                    audit_recorder.try_create_and_record(
                        &dispatch_ref,
                        provider.provider_id(),
                        "error",
                        Some(&json!({
                            "error_domain": "provider_response_identity_mismatch",
                            "response_identity_excluded": true,
                            "redaction_status": "redacted",
                        })),
                    )?;
                    return Err(
                        "agent decision provider response identity does not match the bound request"
                            .to_string(),
                    );
                }
                audit_recorder.try_create_and_record(
                    &dispatch_ref,
                    provider.provider_id(),
                    "response_received",
                    Some(&json!({
                        "input_token_count": response.input_tokens,
                        "output_token_count": response.output_tokens,
                        "cost": response.estimated_cost,
                        "currency": "USD",
                        "redaction_status": "redacted",
                    })),
                )?;
                if response
                    .estimated_cost
                    .is_some_and(|actual| actual > reserved_cost + f64::EPSILON)
                {
                    audit_recorder.try_create_and_record(
                        &dispatch_ref,
                        provider.provider_id(),
                        "error",
                        Some(&json!({"error_domain": "provider_cost_reservation_exceeded"})),
                    )?;
                    return Err("agent decision provider exceeded reserved cost".to_string());
                }
                audit_recorder.try_create_and_record(
                    &dispatch_ref,
                    provider.provider_id(),
                    "reservation_reconciled",
                    Some(&json!({
                        "cost": response.estimated_cost,
                        "currency": "USD",
                        "redaction_status": "redacted",
                    })),
                )?;
                let action = parse_agent_action_response(&response.output)?;
                let state = context
                    .agent_state
                    .as_ref()
                    .ok_or_else(|| "agent state is required for provider decisions".to_string())?;
                let allowed_actions = allowed_agent_action_types(state);
                if !allowed_actions.contains(&agent_action_type(&action)) {
                    audit_recorder.try_create_and_record(
                        &dispatch_ref,
                        provider.provider_id(),
                        "error",
                        Some(&json!({
                            "error_domain": "agent_action_unauthorized",
                            "redaction_status": "redacted",
                        })),
                    )?;
                    return Err(format!(
                        "provider selected unauthorized agent action {}",
                        agent_action_type(&action)
                    ));
                }
                Ok(AgentDecision {
                    action,
                    usage: AgentDecisionUsage {
                        provider_id: response.provider_id,
                        model: response.model,
                        input_tokens: response.input_tokens,
                        output_tokens: response.output_tokens,
                        estimated_cost_usd: response.estimated_cost,
                        reserved_cost_usd: reserved_cost,
                        token_provenance: if response.input_tokens.is_some()
                            || response.output_tokens.is_some()
                        {
                            "provider_reported".to_string()
                        } else {
                            "unavailable".to_string()
                        },
                        cost_provenance: if response.estimated_cost.is_some() {
                            "harness_derived".to_string()
                        } else {
                            "unavailable".to_string()
                        },
                    },
                })
            }
            Err(error) => {
                audit_recorder.try_create_and_record(
                    &dispatch_ref,
                    provider.provider_id(),
                    "error",
                    Some(&json!({
                        "error_domain": error.error_domain,
                        "redaction_status": "redacted",
                    })),
                )?;
                Err(format!(
                    "agent decision provider failed: {}",
                    redact_sensitive_patterns(&error.message)
                ))
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_executor::{AgentAction, AgentStepContext};
    use crate::orchestration::schemas::AgentState;
    use crate::provider::{Provider, ProviderError, ProviderRequest, ProviderResponse};
    use crate::storage::local_product_store::LocalProductStore;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    struct StaticActionProvider {
        output: String,
        calls: Arc<AtomicUsize>,
        requests: Arc<Mutex<Vec<ProviderRequest>>>,
    }

    struct MismatchedIdentityProvider {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl Provider for MismatchedIdentityProvider {
        fn provider_id(&self) -> &str {
            "fixture-agent-provider"
        }

        fn is_enabled(&self) -> bool {
            true
        }

        fn default_model(&self) -> Option<&str> {
            Some("fixture-agent-model")
        }

        async fn invoke(
            &self,
            _request: &ProviderRequest,
        ) -> Result<ProviderResponse, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ProviderResponse {
                schema_version: "provider_response.v1".to_string(),
                provider_id: "different-provider".to_string(),
                model: "different-model".to_string(),
                output: json!({
                    "schema_version": AGENT_ACTION_SCHEMA_VERSION,
                    "action": {"type":"complete"}
                })
                .to_string(),
                input_tokens: Some(1),
                output_tokens: Some(1),
                estimated_cost: Some(0.001),
                provider_request_id: Some("mismatched-request".to_string()),
            })
        }
    }

    #[async_trait::async_trait]
    impl Provider for StaticActionProvider {
        fn provider_id(&self) -> &str {
            "fixture-agent-provider"
        }

        fn is_enabled(&self) -> bool {
            true
        }

        fn default_model(&self) -> Option<&str> {
            Some("fixture-agent-model")
        }

        async fn invoke(
            &self,
            request: &ProviderRequest,
        ) -> Result<ProviderResponse, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.requests
                .lock()
                .expect("requests lock")
                .push(request.clone());
            Ok(ProviderResponse {
                schema_version: "provider_response.v1".to_string(),
                provider_id: self.provider_id().to_string(),
                model: "fixture-agent-model".to_string(),
                output: self.output.clone(),
                input_tokens: Some(21),
                output_tokens: Some(7),
                estimated_cost: Some(0.001),
                provider_request_id: Some("fixture-request-1".to_string()),
            })
        }
    }

    fn decision_context() -> AgentStepContext {
        AgentStepContext {
            agent_id: "agent-1".to_string(),
            run_id: "run-1".to_string(),
            node_id: "node-1".to_string(),
            workflow_id: "workflow-1".to_string(),
            agent_state: Some(AgentState {
                schema_version: "agent_state.v1".to_string(),
                agent_id: "agent-1".to_string(),
                run_id: "run-1".to_string(),
                role: "worker".to_string(),
                capability_profile: vec![
                    "memory".to_string(),
                    "mailbox".to_string(),
                    "child_task".to_string(),
                    "handoff".to_string(),
                    "review".to_string(),
                    "debate".to_string(),
                ],
                objective: Some("perform one bounded step".to_string()),
                status: "idle".to_string(),
                scratchpad_summary: None,
                redaction_filter: None,
                metadata: HashMap::new(),
                created_at: "2026-07-14T00:00:00Z".to_string(),
                updated_at: "2026-07-14T00:00:00Z".to_string(),
            }),
            mailbox_pending_count: 0,
            memory_digest: None,
            memory_context: None,
            memory_state_read_bytes: 0,
            node_metadata: json!({"model": "fixture-agent-model", "execution_attempt": 1}),
        }
    }

    fn fixture_pricing() -> ProviderPricingConfig {
        ProviderPricingConfig {
            input_cost_per_1k: Some(0.001),
            output_cost_per_1k: Some(0.002),
        }
    }

    #[test]
    fn parses_bounded_typed_complete_action() {
        let action = parse_agent_action_response(
            r#"{"schema_version":"agent_action.v1","action":{"type":"complete"}}"#,
        )
        .expect("typed action");

        assert_eq!(action, AgentAction::Complete);
    }

    #[test]
    fn rejects_unknown_action_fields() {
        let error = parse_agent_action_response(
            r#"{"schema_version":"agent_action.v1","action":{"type":"complete","run_id":"other"}}"#,
        )
        .expect_err("unknown fields must fail closed");

        assert!(error.contains("invalid agent action"));
    }

    #[test]
    fn rejects_oversized_provider_response_before_parsing() {
        let error = parse_agent_action_response(&"x".repeat(MAX_AGENT_ACTION_RESPONSE_BYTES + 1))
            .expect_err("oversized action must fail closed");

        assert!(error.contains("exceeds"));
    }

    #[test]
    fn provider_source_returns_only_typed_action_and_records_usage_audit() {
        let store = Arc::new(LocalProductStore::new(":memory:").expect("store"));
        let calls = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let provider: Arc<dyn Provider> = Arc::new(StaticActionProvider {
            output: r#"{"schema_version":"agent_action.v1","action":{"type":"complete"}}"#
                .to_string(),
            calls: calls.clone(),
            requests: requests.clone(),
        });
        let recorder = Arc::new(crate::provider::ProviderAuditRecorder::with_store(
            store.clone(),
        ));
        let source = provider_agent_decision_fn(
            provider,
            store,
            recorder.clone(),
            crate::provider::CostGateConfig::new(Some(1.0), Some(2.0)),
            fixture_pricing(),
            true,
        );

        let decision = source(&decision_context()).expect("decision");
        assert_eq!(decision.action, AgentAction::Complete);
        assert_eq!(decision.usage.input_tokens, Some(21));
        assert_eq!(decision.usage.output_tokens, Some(7));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let captured = requests.lock().expect("requests lock");
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].model, "fixture-agent-model");
        let prompt: Value = serde_json::from_str(&captured[0].prompt).expect("prompt JSON");
        assert_eq!(
            prompt.pointer("/agent_state/capability_profile"),
            Some(&json!([
                "memory",
                "mailbox",
                "child_task",
                "handoff",
                "review",
                "debate"
            ]))
        );
        assert!(prompt["allowed_actions"]
            .as_array()
            .expect("allowed actions")
            .contains(&json!("resolve_debate")));
        assert_eq!(
            prompt.pointer("/response_schema/additionalProperties"),
            Some(&json!(false))
        );
        assert_eq!(prompt["typed_examples"].as_array().map(Vec::len), Some(17));
        drop(captured);
        let events = recorder.list_events("workflow:run-1:node-1:attempt-1:agent-decision");
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].event_type, "request_reserved");
        assert!(events[0].event_id.starts_with("paudit-reservation-"));
        assert_eq!(events[1].event_type, "request_sent");
        assert_eq!(events[2].input_token_count, Some(21));
        assert_eq!(events[2].output_token_count, Some(7));
        assert_eq!(events[3].event_type, "reservation_reconciled");
    }

    #[test]
    fn provider_source_fails_before_call_when_execution_gate_is_closed() {
        let store = Arc::new(LocalProductStore::new(":memory:").expect("store"));
        let calls = Arc::new(AtomicUsize::new(0));
        let provider: Arc<dyn Provider> = Arc::new(StaticActionProvider {
            output: r#"{"schema_version":"agent_action.v1","action":{"type":"complete"}}"#
                .to_string(),
            calls: calls.clone(),
            requests: Arc::new(Mutex::new(Vec::new())),
        });
        let source = provider_agent_decision_fn(
            provider,
            store,
            Arc::new(crate::provider::ProviderAuditRecorder::new()),
            crate::provider::CostGateConfig::new(None, None),
            fixture_pricing(),
            false,
        );

        assert!(source(&decision_context())
            .expect_err("gate must fail closed")
            .contains("not enabled"));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn provider_source_accepts_every_typed_action_family() {
        let context = decision_context();
        let examples = agent_action_typed_examples(&context);
        assert_eq!(examples.len(), 17);

        for example in examples {
            let expected_type = example
                .pointer("/action/type")
                .and_then(Value::as_str)
                .expect("typed action")
                .to_string();
            let store = Arc::new(LocalProductStore::new(":memory:").expect("store"));
            let calls = Arc::new(AtomicUsize::new(0));
            let provider: Arc<dyn Provider> = Arc::new(StaticActionProvider {
                output: example.to_string(),
                calls: calls.clone(),
                requests: Arc::new(Mutex::new(Vec::new())),
            });
            let source = provider_agent_decision_fn(
                provider,
                store.clone(),
                Arc::new(crate::provider::ProviderAuditRecorder::with_store(store)),
                crate::provider::CostGateConfig::new(Some(1.0), Some(2.0)),
                fixture_pricing(),
                true,
            );

            let decision = source(&context).unwrap_or_else(|error| {
                panic!("provider action {expected_type} must be accepted: {error}")
            });
            assert_eq!(agent_action_type(&decision.action), expected_type);
            assert_eq!(calls.load(Ordering::SeqCst), 1);
        }
    }

    #[test]
    fn provider_prompt_is_capability_authoritative_and_output_fails_closed() {
        let store = Arc::new(LocalProductStore::new(":memory:").expect("store"));
        let calls = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let provider: Arc<dyn Provider> = Arc::new(StaticActionProvider {
            output: json!({
                "schema_version": AGENT_ACTION_SCHEMA_VERSION,
                "action": {"type":"emit_note","note":"not authorized"}
            })
            .to_string(),
            calls: calls.clone(),
            requests: requests.clone(),
        });
        let recorder = Arc::new(crate::provider::ProviderAuditRecorder::with_store(
            store.clone(),
        ));
        let source = provider_agent_decision_fn(
            provider,
            store,
            recorder.clone(),
            crate::provider::CostGateConfig::new(Some(1.0), Some(2.0)),
            fixture_pricing(),
            true,
        );
        let mut context = decision_context();
        context
            .agent_state
            .as_mut()
            .expect("agent state")
            .capability_profile = vec!["mailbox".to_string()];

        let error = source(&context).expect_err("unauthorized action must fail closed");
        assert!(error.contains("unauthorized agent action emit_note"));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let captured = requests.lock().expect("requests lock");
        let prompt: Value = serde_json::from_str(&captured[0].prompt).expect("prompt JSON");
        assert_eq!(
            prompt["allowed_actions"],
            json!(["wait", "complete", "read_mailbox", "ack_message"])
        );
        assert!(!prompt["allowed_actions"]
            .as_array()
            .expect("allowed actions")
            .contains(&json!("emit_note")));
        drop(captured);
        assert_eq!(
            recorder
                .list_events("workflow:run-1:node-1:attempt-1:agent-decision")
                .last()
                .map(|event| event.event_type.as_str()),
            Some("error")
        );
    }

    #[test]
    fn provider_source_rejects_model_mismatch_before_reservation_or_call() {
        let store = Arc::new(LocalProductStore::new(":memory:").expect("store"));
        let calls = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let provider: Arc<dyn Provider> = Arc::new(StaticActionProvider {
            output: json!({
                "schema_version": AGENT_ACTION_SCHEMA_VERSION,
                "action": {"type":"complete"}
            })
            .to_string(),
            calls: calls.clone(),
            requests: requests.clone(),
        });
        let recorder = Arc::new(crate::provider::ProviderAuditRecorder::with_store(
            store.clone(),
        ));
        let source = provider_agent_decision_fn(
            provider,
            store,
            recorder.clone(),
            crate::provider::CostGateConfig::new(Some(1.0), Some(2.0)),
            fixture_pricing(),
            true,
        );
        let mut context = decision_context();
        context.node_metadata["model"] = json!("different-model");

        let error = source(&context).expect_err("model mismatch must fail closed");
        assert!(error.contains("does not match provider model"));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(requests.lock().expect("requests lock").is_empty());
        assert!(recorder
            .list_events("workflow:run-1:node-1:attempt-1:agent-decision")
            .is_empty());
    }

    #[test]
    fn provider_source_rejects_mismatched_response_identity() {
        let store = Arc::new(LocalProductStore::new(":memory:").expect("store"));
        let calls = Arc::new(AtomicUsize::new(0));
        let provider: Arc<dyn Provider> = Arc::new(MismatchedIdentityProvider {
            calls: calls.clone(),
        });
        let recorder = Arc::new(crate::provider::ProviderAuditRecorder::with_store(
            store.clone(),
        ));
        let source = provider_agent_decision_fn(
            provider,
            store,
            recorder.clone(),
            crate::provider::CostGateConfig::new(Some(1.0), Some(2.0)),
            fixture_pricing(),
            true,
        );

        let error = source(&decision_context()).expect_err("identity mismatch must fail closed");
        assert!(error.contains("response identity"));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            recorder
                .list_events("workflow:run-1:node-1:attempt-1:agent-decision")
                .last()
                .and_then(|event| event.error_domain.as_deref()),
            Some("provider_response_identity_mismatch")
        );
    }

    #[test]
    fn prompt_preserves_bounded_follow_up_ids_without_raw_mailbox_fields() {
        let store = LocalProductStore::new(":memory:").expect("store");
        store
            .send_message(
                "message-1",
                "agent-peer",
                "agent-1",
                "status_update",
                Some("bounded redacted summary source"),
                Some("correlation-review-1"),
                Some("run-1"),
                Some("node-review-1"),
                None,
                &json!({"proposal_id":"review-proposal-1"}),
            )
            .expect("mailbox message");
        let mut context = decision_context();
        context.node_metadata["context_injection"] = json!({
            "sources": [{
                "from_node_id": "node-prior",
                "output": {
                    "action": "read_mailbox",
                    "messages": [{
                        "message_id": "message-2",
                        "correlation_id": "correlation-debate-1",
                        "from": "agent-peer",
                        "node_id": "node-debate-1",
                        "type": "status_update",
                        "proposal_id": "debate-proposal-1",
                        "summary": "bounded summary",
                        "body": "must-not-be-copied"
                    }]
                }
            }]
        });

        let prompt = build_agent_decision_prompt(&context, &store).expect("bounded prompt");
        assert!(prompt.len() <= MAX_AGENT_DECISION_PROMPT_BYTES);
        let prompt: Value = serde_json::from_str(&prompt).expect("prompt JSON");
        let mailbox = &prompt["mailbox_observations"][0];
        assert_eq!(mailbox["correlation_id"], "correlation-review-1");
        assert_eq!(mailbox["node_id"], "node-review-1");
        assert_eq!(mailbox["proposal_id"], "review-proposal-1");
        assert!(mailbox.get("body").is_none());
        let predecessor = &prompt["predecessor_action_observations"][0]["messages"][0];
        assert_eq!(predecessor["correlation_id"], "correlation-debate-1");
        assert_eq!(predecessor["node_id"], "node-debate-1");
        assert_eq!(predecessor["proposal_id"], "debate-proposal-1");
        assert!(predecessor.get("body").is_none());
        assert!(!prompt.to_string().contains("must-not-be-copied"));
    }

    #[test]
    fn prompt_rejects_context_that_exceeds_global_byte_cap() {
        let store = LocalProductStore::new(":memory:").expect("store");
        let mut context = decision_context();
        context.memory_context = Some(json!({"bounded_context":"x".repeat(40_000)}));

        let error = build_agent_decision_prompt(&context, &store)
            .expect_err("oversized provider prompt must fail closed");
        assert!(error.contains("prompt exceeds"));
    }
}
