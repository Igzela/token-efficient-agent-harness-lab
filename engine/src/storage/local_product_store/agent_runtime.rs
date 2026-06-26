use rusqlite::{params, Row};
use serde_json::{json, Value};

use super::{append_audit_locked, DatabaseConnection, LocalProductStore};
use crate::orchestration::schemas::{
    AgentState, MailboxMessage, AGENT_MESSAGE_SCHEMA_VERSION, AGENT_STATE_SCHEMA_VERSION,
    PROPOSAL_STATUSES, PROPOSAL_TYPES,
};
use crate::provider::redaction::{contains_sensitive_patterns, redact_sensitive_patterns};

const MAX_SCRATCHPAD_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 64 * 1024;
const MAX_BODY_SUMMARY_BYTES: usize = 1024;

// ── AgentState CRUD ──────────────────────────────────────────────────────────

impl LocalProductStore {
    pub fn create_agent_state(
        &self,
        agent_id: &str,
        run_id: &str,
        role: &str,
        capability_profile: &[String],
        objective: Option<&str>,
        status: &str,
        metadata: &Value,
    ) -> Result<AgentState, String> {
        let now = self.now();
        let caps_json = serde_json::to_string(capability_profile).map_err(|e| e.to_string())?;
        let meta_json = serde_json::to_string(metadata).map_err(|e| e.to_string())?;

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute(
                    "INSERT INTO agent_state
                     (agent_id, run_id, role, capability_profile_json, objective, status,
                      scratchpad_summary, redaction_filter, metadata_json, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL, ?7, ?8, ?9)",
                    params![
                        agent_id, run_id, role, caps_json, objective, status, meta_json, now, now
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    conn,
                    &now,
                    "system",
                    "agent_state.create",
                    &format!("agent_state/{agent_id}/{run_id}"),
                    &json!({"agent_id": agent_id, "run_id": run_id, "role": role}),
                )?;
                Ok(build_agent_state(
                    agent_id, run_id, role, &caps_json, objective, status, None, None, &meta_json,
                    &now, &now,
                ))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .execute(
                        "INSERT INTO agent_state
                     (agent_id, run_id, role, capability_profile_json, objective, status,
                      scratchpad_summary, redaction_filter, metadata_json, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, $5, $6, NULL, NULL, $7, $8, $9)",
                        &[
                            &agent_id, &run_id, &role, &caps_json, &objective, &status, &meta_json,
                            &now, &now,
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                pg_runtime_audit(
                    client,
                    &now,
                    "system",
                    "agent_state.create",
                    &format!("agent_state/{agent_id}/{run_id}"),
                    &json!({"agent_id": agent_id, "run_id": run_id, "role": role}),
                )?;
                Ok(build_agent_state(
                    agent_id, run_id, role, &caps_json, objective, status, None, None, &meta_json,
                    &now, &now,
                ))
            }),
        }
    }

    pub fn update_agent_state(
        &self,
        agent_id: &str,
        run_id: &str,
        status: Option<&str>,
        scratchpad_summary: Option<&str>,
        objective: Option<&str>,
        metadata: Option<&Value>,
    ) -> Result<Option<AgentState>, String> {
        let current = self.get_agent_state(agent_id, run_id)?;
        let mut state = match current {
            Some(s) => s,
            None => return Ok(None),
        };

        let now = self.now();
        if let Some(s) = status {
            state.status = s.to_string();
        }
        if let Some(s) = scratchpad_summary {
            state.scratchpad_summary = Some(apply_size_cap_and_redact(s, MAX_SCRATCHPAD_BYTES));
        }
        if let Some(o) = objective {
            state.objective = Some(o.to_string());
        }
        if let Some(m) = metadata {
            if let Some(obj) = m.as_object() {
                for (k, v) in obj {
                    state.metadata.insert(k.clone(), v.clone());
                }
            }
        }
        state.updated_at = now.clone();

        let caps_json =
            serde_json::to_string(&state.capability_profile).map_err(|e| e.to_string())?;
        let meta_json = serde_json::to_string(&state.metadata).map_err(|e| e.to_string())?;

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute(
                    "UPDATE agent_state SET role=?1, capability_profile_json=?2, objective=?3,
                     status=?4, scratchpad_summary=?5, redaction_filter=?6, metadata_json=?7,
                     updated_at=?8
                     WHERE agent_id=?9 AND run_id=?10",
                    params![
                        state.role,
                        caps_json,
                        state.objective,
                        state.status,
                        state.scratchpad_summary,
                        state.redaction_filter,
                        meta_json,
                        state.updated_at,
                        agent_id,
                        run_id,
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    conn,
                    &now,
                    "system",
                    "agent_state.update",
                    &format!("agent_state/{agent_id}/{run_id}"),
                    &json!({"agent_id": agent_id, "run_id": run_id}),
                )?;
                Ok(Some(state))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => {
                self.with_pg_conn(|client| {
                    client.execute(
                    "UPDATE agent_state SET role=$1, capability_profile_json=$2, objective=$3,
                     status=$4, scratchpad_summary=$5, redaction_filter=$6, metadata_json=$7,
                     updated_at=$8
                     WHERE agent_id=$9 AND run_id=$10",
                    &[&state.role, &caps_json, &state.objective, &state.status,
                      &state.scratchpad_summary, &state.redaction_filter, &meta_json,
                      &state.updated_at, &agent_id, &run_id],
                ).map_err(|e| e.to_string())?;
                    pg_runtime_audit(
                        client,
                        &now,
                        "system",
                        "agent_state.update",
                        &format!("agent_state/{agent_id}/{run_id}"),
                        &json!({"agent_id": agent_id, "run_id": run_id}),
                    )?;
                    Ok(Some(state))
                })
            }
        }
    }

    pub fn get_agent_state(
        &self,
        agent_id: &str,
        run_id: &str,
    ) -> Result<Option<AgentState>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT agent_id, run_id, role, capability_profile_json, objective,
                            status, scratchpad_summary, redaction_filter, metadata_json,
                            created_at, updated_at
                     FROM agent_state WHERE agent_id=?1 AND run_id=?2",
                    )
                    .map_err(|e| e.to_string())?;
                let mut rows = stmt
                    .query_map(params![agent_id, run_id], sqlite_agent_state_row)
                    .map_err(|e| e.to_string())?;
                match rows.next() {
                    Some(r) => Ok(Some(r.map_err(|e| e.to_string())?)),
                    None => Ok(None),
                }
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT agent_id, run_id, role, capability_profile_json, objective,
                            status, scratchpad_summary, redaction_filter, metadata_json,
                            created_at, updated_at
                     FROM agent_state WHERE agent_id=$1 AND run_id=$2",
                        &[&agent_id, &run_id],
                    )
                    .map_err(|e| e.to_string())?;
                match rows.into_iter().next() {
                    Some(row) => Ok(Some(pg_agent_state_row(&row))),
                    None => Ok(None),
                }
            }),
        }
    }

    pub fn list_agent_state_by_run(&self, run_id: &str) -> Result<Vec<AgentState>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT agent_id, run_id, role, capability_profile_json, objective,
                            status, scratchpad_summary, redaction_filter, metadata_json,
                            created_at, updated_at
                     FROM agent_state WHERE run_id=?1 ORDER BY agent_id",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![run_id], sqlite_agent_state_row)
                    .map_err(|e| e.to_string())?;
                let mut out = Vec::new();
                for r in rows {
                    out.push(r.map_err(|e| e.to_string())?);
                }
                Ok(out)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT agent_id, run_id, role, capability_profile_json, objective,
                            status, scratchpad_summary, redaction_filter, metadata_json,
                            created_at, updated_at
                     FROM agent_state WHERE run_id=$1 ORDER BY agent_id",
                        &[&run_id],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(rows.iter().map(pg_agent_state_row).collect())
            }),
        }
    }

    pub fn delete_agent_state(&self, agent_id: &str, run_id: &str) -> Result<bool, String> {
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let n = conn
                    .execute(
                        "DELETE FROM agent_state WHERE agent_id=?1 AND run_id=?2",
                        params![agent_id, run_id],
                    )
                    .map_err(|e| e.to_string())?;
                if n > 0 {
                    append_audit_locked(
                        conn,
                        &now,
                        "system",
                        "agent_state.delete",
                        &format!("agent_state/{agent_id}/{run_id}"),
                        &json!({"agent_id": agent_id, "run_id": run_id}),
                    )?;
                }
                Ok(n > 0)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let n = client
                    .execute(
                        "DELETE FROM agent_state WHERE agent_id=$1 AND run_id=$2",
                        &[&agent_id, &run_id],
                    )
                    .map_err(|e| e.to_string())?;
                if n > 0 {
                    pg_runtime_audit(
                        client,
                        &now,
                        "system",
                        "agent_state.delete",
                        &format!("agent_state/{agent_id}/{run_id}"),
                        &json!({"agent_id": agent_id, "run_id": run_id}),
                    )?;
                }
                Ok(n > 0)
            }),
        }
    }

    // ── Mailbox: send / read / ack / reply / list ──────────────────────────

    pub fn send_message(
        &self,
        message_id: &str,
        from_agent_id: &str,
        to_agent_id: &str,
        message_type: &str,
        body: Option<&str>,
        correlation_id: Option<&str>,
        run_id: Option<&str>,
        node_id: Option<&str>,
        reply_to_message_id: Option<&str>,
        metadata: &Value,
    ) -> Result<MailboxMessage, String> {
        let now = self.now();
        let (safe_body, redact_status) = redact_message_body(body);
        let body_summary = safe_body
            .as_ref()
            .map(|b| apply_size_cap(b, MAX_BODY_SUMMARY_BYTES));
        let meta_json = serde_json::to_string(metadata).map_err(|e| e.to_string())?;

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute(
                    "INSERT INTO agent_mailbox
                     (message_id, correlation_id, from_agent_id, to_agent_id,
                      run_id, node_id, message_type, status, body, body_summary,
                      redaction_status, created_at, reply_to_message_id, metadata_json)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,'pending',?8,?9,?10,?11,?12,?13)",
                    params![
                        message_id,
                        correlation_id,
                        from_agent_id,
                        to_agent_id,
                        run_id,
                        node_id,
                        message_type,
                        safe_body,
                        body_summary,
                        redact_status,
                        now,
                        reply_to_message_id,
                        meta_json,
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    conn,
                    &now,
                    &format!("agent:{from_agent_id}"),
                    "agent_mailbox.send",
                    &format!("agent_mailbox/{message_id}"),
                    &json!({"message_id": message_id, "from_agent_id": from_agent_id,
                            "to_agent_id": to_agent_id, "message_type": message_type,
                            "redaction_status": redact_status}),
                )?;
                Ok(build_mailbox_message(
                    message_id,
                    correlation_id,
                    from_agent_id,
                    to_agent_id,
                    run_id,
                    node_id,
                    message_type,
                    "pending",
                    &safe_body,
                    &body_summary,
                    redact_status,
                    &now,
                    None,
                    None,
                    reply_to_message_id,
                    &meta_json,
                ))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .execute(
                        "INSERT INTO agent_mailbox
                     (message_id, correlation_id, from_agent_id, to_agent_id,
                      run_id, node_id, message_type, status, body, body_summary,
                      redaction_status, created_at, reply_to_message_id, metadata_json)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,'pending',$8,$9,$10,$11,$12,$13,$14)",
                        &[
                            &message_id,
                            &correlation_id,
                            &from_agent_id,
                            &to_agent_id,
                            &run_id,
                            &node_id,
                            &message_type,
                            &safe_body,
                            &body_summary,
                            &redact_status,
                            &now,
                            &reply_to_message_id,
                            &meta_json,
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                pg_runtime_audit(
                    client,
                    &now,
                    &format!("agent:{from_agent_id}"),
                    "agent_mailbox.send",
                    &format!("agent_mailbox/{message_id}"),
                    &json!({"message_id": message_id, "from_agent_id": from_agent_id,
                            "to_agent_id": to_agent_id, "message_type": message_type,
                            "redaction_status": redact_status}),
                )?;
                Ok(build_mailbox_message(
                    message_id,
                    correlation_id,
                    from_agent_id,
                    to_agent_id,
                    run_id,
                    node_id,
                    message_type,
                    "pending",
                    &safe_body,
                    &body_summary,
                    redact_status,
                    &now,
                    None,
                    None,
                    reply_to_message_id,
                    &meta_json,
                ))
            }),
        }
    }

    pub fn read_message(&self, message_id: &str) -> Result<Option<MailboxMessage>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT message_id, correlation_id, from_agent_id, to_agent_id,
                            run_id, node_id, message_type, status, body, body_summary,
                            redaction_status, created_at, read_at, ack_at,
                            reply_to_message_id, metadata_json
                     FROM agent_mailbox WHERE message_id=?1",
                    )
                    .map_err(|e| e.to_string())?;
                let mut rows = stmt
                    .query_map(params![message_id], sqlite_mailbox_row)
                    .map_err(|e| e.to_string())?;
                match rows.next() {
                    Some(r) => Ok(Some(r.map_err(|e| e.to_string())?)),
                    None => Ok(None),
                }
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT message_id, correlation_id, from_agent_id, to_agent_id,
                            run_id, node_id, message_type, status, body, body_summary,
                            redaction_status, created_at, read_at, ack_at,
                            reply_to_message_id, metadata_json
                     FROM agent_mailbox WHERE message_id=$1",
                        &[&message_id],
                    )
                    .map_err(|e| e.to_string())?;
                match rows.into_iter().next() {
                    Some(row) => Ok(Some(pg_mailbox_row(&row))),
                    None => Ok(None),
                }
            }),
        }
    }

    pub fn mark_message_read(&self, message_id: &str) -> Result<Option<MailboxMessage>, String> {
        let now = self.now();
        let affected = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let n = conn
                    .execute(
                        "UPDATE agent_mailbox SET status='read', read_at=?1
                     WHERE message_id=?2 AND status='pending'",
                        params![now, message_id],
                    )
                    .map_err(|e| e.to_string())?;
                if n > 0 {
                    append_audit_locked(
                        conn,
                        &now,
                        "system",
                        "agent_mailbox.read",
                        &format!("agent_mailbox/{message_id}"),
                        &json!({"message_id": message_id}),
                    )?;
                }
                Ok(n)
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let n = client
                    .execute(
                        "UPDATE agent_mailbox SET status='read', read_at=$1
                     WHERE message_id=$2 AND status='pending'",
                        &[&now, &message_id],
                    )
                    .map_err(|e| e.to_string())?;
                if n > 0 {
                    pg_runtime_audit(
                        client,
                        &now,
                        "system",
                        "agent_mailbox.read",
                        &format!("agent_mailbox/{message_id}"),
                        &json!({"message_id": message_id}),
                    )?;
                }
                Ok(n as usize)
            })?,
        };
        if affected == 0 {
            Ok(None)
        } else {
            self.read_message(message_id)
        }
    }

    pub fn ack_message_for_agent(
        &self,
        message_id: &str,
        agent_id: &str,
        run_id: &str,
    ) -> Result<Option<MailboxMessage>, String> {
        let msg = match self.read_message(message_id)? {
            Some(m) => m,
            None => return Ok(None),
        };
        if msg.to_agent_id != agent_id {
            return Err(format!(
                "agent {agent_id} is not the target of message {message_id}"
            ));
        }
        match msg.run_id {
            Some(ref mid) if mid == run_id => {}
            Some(ref mid) => {
                return Err(format!(
                    "message {message_id} has run_id '{mid}', not '{run_id}'"
                ));
            }
            None => {
                return Err(format!(
                    "message {message_id} has no run_id; agent-scoped ack requires run_id '{run_id}'"
                ));
            }
        }
        self.ack_message(message_id)
    }

    pub fn ack_message(&self, message_id: &str) -> Result<Option<MailboxMessage>, String> {
        let now = self.now();
        let affected = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let n = conn
                    .execute(
                        "UPDATE agent_mailbox SET status='acked', ack_at=?1
                     WHERE message_id=?2 AND status IN ('pending','read')",
                        params![now, message_id],
                    )
                    .map_err(|e| e.to_string())?;
                if n > 0 {
                    append_audit_locked(
                        conn,
                        &now,
                        "system",
                        "agent_mailbox.ack",
                        &format!("agent_mailbox/{message_id}"),
                        &json!({"message_id": message_id}),
                    )?;
                }
                Ok(n)
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let n = client
                    .execute(
                        "UPDATE agent_mailbox SET status='acked', ack_at=$1
                     WHERE message_id=$2 AND status IN ('pending','read')",
                        &[&now, &message_id],
                    )
                    .map_err(|e| e.to_string())?;
                if n > 0 {
                    pg_runtime_audit(
                        client,
                        &now,
                        "system",
                        "agent_mailbox.ack",
                        &format!("agent_mailbox/{message_id}"),
                        &json!({"message_id": message_id}),
                    )?;
                }
                Ok(n as usize)
            })?,
        };
        if affected == 0 {
            Ok(None)
        } else {
            self.read_message(message_id)
        }
    }

    pub fn reply_to_message(
        &self,
        reply_message_id: &str,
        original_message_id: &str,
        from_agent_id: &str,
        to_agent_id: &str,
        message_type: &str,
        body: Option<&str>,
        metadata: &Value,
    ) -> Result<Option<MailboxMessage>, String> {
        let original = match self.read_message(original_message_id)? {
            Some(m) => m,
            None => return Ok(None),
        };
        let correlation_id = original
            .correlation_id
            .clone()
            .unwrap_or_else(|| original_message_id.to_string());

        let sent = self.send_message(
            reply_message_id,
            from_agent_id,
            to_agent_id,
            message_type,
            body,
            Some(&correlation_id),
            original.run_id.as_deref(),
            original.node_id.as_deref(),
            Some(original_message_id),
            metadata,
        )?;

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute(
                    "UPDATE agent_mailbox SET status='replied' WHERE message_id=?1",
                    params![original_message_id],
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .execute(
                        "UPDATE agent_mailbox SET status='replied' WHERE message_id=$1",
                        &[&original_message_id],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(())
            })?,
        }
        Ok(Some(sent))
    }

    pub fn list_mailbox(
        &self,
        agent_id: Option<&str>,
        run_id: Option<&str>,
        node_id: Option<&str>,
        status_filter: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<MailboxMessage>, String> {
        let mut wheres = Vec::new();
        if let Some(a) = agent_id {
            wheres.push(format!("to_agent_id='{}'", esc_sql(a)));
        }
        if let Some(r) = run_id {
            wheres.push(format!("run_id='{}'", esc_sql(r)));
        }
        if let Some(n) = node_id {
            wheres.push(format!("node_id='{}'", esc_sql(n)));
        }
        if let Some(s) = status_filter {
            wheres.push(format!("status='{}'", esc_sql(s)));
        }
        let where_clause = if wheres.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", wheres.join(" AND "))
        };

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let sql = format!(
                    "SELECT message_id, correlation_id, from_agent_id, to_agent_id,
                            run_id, node_id, message_type, status, body, body_summary,
                            redaction_status, created_at, read_at, ack_at,
                            reply_to_message_id, metadata_json
                     FROM agent_mailbox {where_clause}
                     ORDER BY message_sequence DESC LIMIT ?1 OFFSET ?2"
                );
                let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![limit, offset], sqlite_mailbox_row)
                    .map_err(|e| e.to_string())?;
                let mut out = Vec::new();
                for r in rows {
                    out.push(r.map_err(|e| e.to_string())?);
                }
                Ok(out)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let sql = format!(
                    "SELECT message_id, correlation_id, from_agent_id, to_agent_id,
                            run_id, node_id, message_type, status, body, body_summary,
                            redaction_status, created_at, read_at, ack_at,
                            reply_to_message_id, metadata_json
                     FROM agent_mailbox {where_clause}
                     ORDER BY message_sequence DESC LIMIT $1 OFFSET $2"
                );
                let rows = client
                    .query(&sql, &[&limit, &offset])
                    .map_err(|e| e.to_string())?;
                Ok(rows.iter().map(pg_mailbox_row).collect())
            }),
        }
    }

    pub fn count_mailbox(
        &self,
        agent_id: Option<&str>,
        run_id: Option<&str>,
        status_filter: Option<&str>,
    ) -> Result<i64, String> {
        let mut wheres = Vec::new();
        if let Some(a) = agent_id {
            wheres.push(format!("to_agent_id='{}'", esc_sql(a)));
        }
        if let Some(r) = run_id {
            wheres.push(format!("run_id='{}'", esc_sql(r)));
        }
        if let Some(s) = status_filter {
            wheres.push(format!("status='{}'", esc_sql(s)));
        }
        let where_clause = if wheres.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", wheres.join(" AND "))
        };

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let sql = format!("SELECT COUNT(*) FROM agent_mailbox {where_clause}");
                conn.query_row(&sql, [], |row| row.get::<_, i64>(0))
                    .map_err(|e| e.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let sql = format!("SELECT COUNT(*) FROM agent_mailbox {where_clause}");
                let row = client.query_one(&sql, &[]).map_err(|e| e.to_string())?;
                Ok(row.get::<_, i64>(0))
            }),
        }
    }
}

// ── Agent Proposal CRUD (AR-3) ──────────────────────────────────────────────

const MAX_PROPOSAL_OBJECTIVE_BYTES: usize = 4096;
const MAX_PROPOSAL_CONTEXT_BYTES: usize = 16384;

impl LocalProductStore {
    pub fn create_proposal(
        &self,
        proposal_id: &str,
        correlation_id: &str,
        run_id: &str,
        parent_node_id: &str,
        agent_id: &str,
        proposal_type: &str,
        objective: &str,
        context_summary: &str,
        target_agent_id: Option<&str>,
        proposed_node_id: Option<&str>,
        proposed_edge_id: Option<&str>,
    ) -> Result<String, String> {
        if !PROPOSAL_TYPES.contains(&proposal_type) {
            return Err(format!(
                "invalid proposal_type '{proposal_type}', expected one of {}",
                PROPOSAL_TYPES.join(", ")
            ));
        }
        if objective.len() > MAX_PROPOSAL_OBJECTIVE_BYTES {
            return Err(format!(
                "objective exceeds max size of {} bytes",
                MAX_PROPOSAL_OBJECTIVE_BYTES
            ));
        }
        let context = apply_size_cap(context_summary, MAX_PROPOSAL_CONTEXT_BYTES);
        let safe_objective = apply_size_cap_and_redact(objective, MAX_PROPOSAL_OBJECTIVE_BYTES);
        let safe_context = apply_size_cap_and_redact(&context, MAX_PROPOSAL_CONTEXT_BYTES);
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute(
                    "INSERT INTO agent_proposals
                     (proposal_id, correlation_id, run_id, parent_node_id, agent_id,
                      target_agent_id, proposed_node_id, proposed_edge_id, proposal_type,
                      objective, context_summary, status, created_at, updated_at)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'pending',?12,?13)",
                    params![
                        proposal_id,
                        correlation_id,
                        run_id,
                        parent_node_id,
                        agent_id,
                        target_agent_id,
                        proposed_node_id,
                        proposed_edge_id,
                        proposal_type,
                        safe_objective,
                        safe_context,
                        now,
                        now,
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    conn,
                    &now,
                    &format!("agent:{agent_id}"),
                    "agent_proposal.create",
                    &format!("agent_proposal/{proposal_id}"),
                    &json!({"proposal_id": proposal_id, "correlation_id": correlation_id,
                            "proposal_type": proposal_type, "run_id": run_id,
                            "agent_id": agent_id, "parent_node_id": parent_node_id}),
                )?;
                Ok(proposal_id.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .execute(
                        "INSERT INTO agent_proposals
                     (proposal_id, correlation_id, run_id, parent_node_id, agent_id,
                      target_agent_id, proposed_node_id, proposed_edge_id, proposal_type,
                      objective, context_summary, status, created_at, updated_at)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'pending',$12,$13)",
                        &[
                            &proposal_id,
                            &correlation_id,
                            &run_id,
                            &parent_node_id,
                            &agent_id,
                            &target_agent_id,
                            &proposed_node_id,
                            &proposed_edge_id,
                            &proposal_type,
                            &safe_objective,
                            &safe_context,
                            &now,
                            &now,
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                pg_runtime_audit(
                    client,
                    &now,
                    &format!("agent:{agent_id}"),
                    "agent_proposal.create",
                    &format!("agent_proposal/{proposal_id}"),
                    &json!({"proposal_id": proposal_id, "correlation_id": correlation_id,
                            "proposal_type": proposal_type, "run_id": run_id,
                            "agent_id": agent_id, "parent_node_id": parent_node_id}),
                )?;
                Ok(proposal_id.to_string())
            }),
        }
    }

    pub fn get_proposal(&self, proposal_id: &str) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT proposal_id, correlation_id, run_id, parent_node_id, agent_id,
                                target_agent_id, proposed_node_id, proposed_edge_id,
                                proposal_type, objective, context_summary, status,
                                created_at, updated_at
                         FROM agent_proposals WHERE proposal_id=?1",
                    )
                    .map_err(|e| e.to_string())?;
                let mut rows = stmt
                    .query_map(params![proposal_id], |row| {
                        Ok(json!({
                            "proposal_id": row.get::<_, String>(0)?,
                            "correlation_id": row.get::<_, String>(1)?,
                            "run_id": row.get::<_, String>(2)?,
                            "parent_node_id": row.get::<_, String>(3)?,
                            "agent_id": row.get::<_, String>(4)?,
                            "target_agent_id": row.get::<_, Option<String>>(5)?,
                            "proposed_node_id": row.get::<_, Option<String>>(6)?,
                            "proposed_edge_id": row.get::<_, Option<String>>(7)?,
                            "proposal_type": row.get::<_, String>(8)?,
                            "objective": row.get::<_, String>(9)?,
                            "context_summary": row.get::<_, String>(10)?,
                            "status": row.get::<_, String>(11)?,
                            "created_at": row.get::<_, String>(12)?,
                            "updated_at": row.get::<_, String>(13)?,
                        }))
                    })
                    .map_err(|e| e.to_string())?;
                match rows.next() {
                    Some(r) => Ok(Some(r.map_err(|e| e.to_string())?)),
                    None => Ok(None),
                }
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT proposal_id, correlation_id, run_id, parent_node_id, agent_id,
                                target_agent_id, proposed_node_id, proposed_edge_id,
                                proposal_type, objective, context_summary, status,
                                created_at, updated_at
                         FROM agent_proposals WHERE proposal_id=$1",
                        &[&proposal_id],
                    )
                    .map_err(|e| e.to_string())?;
                match rows.into_iter().next() {
                    Some(row) => Ok(Some(pg_proposal_row(&row))),
                    None => Ok(None),
                }
            }),
        }
    }

    pub fn update_proposal_status(
        &self,
        proposal_id: &str,
        new_status: &str,
    ) -> Result<bool, String> {
        if !PROPOSAL_STATUSES.contains(&new_status) {
            return Err(format!(
                "invalid status '{new_status}', expected one of {}",
                PROPOSAL_STATUSES.join(", ")
            ));
        }
        let now = self.now();
        let affected = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let n = conn
                    .execute(
                        "UPDATE agent_proposals SET status=?1, updated_at=?2
                         WHERE proposal_id=?3 AND status='pending'",
                        params![new_status, now, proposal_id],
                    )
                    .map_err(|e| e.to_string())?;
                if n > 0 {
                    append_audit_locked(
                        conn,
                        &now,
                        "system",
                        "agent_proposal.update_status",
                        &format!("agent_proposal/{proposal_id}"),
                        &json!({"proposal_id": proposal_id, "new_status": new_status}),
                    )?;
                }
                Ok(n)
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let n = client
                    .execute(
                        "UPDATE agent_proposals SET status=$1, updated_at=$2
                         WHERE proposal_id=$3 AND status='pending'",
                        &[&new_status, &now, &proposal_id],
                    )
                    .map_err(|e| e.to_string())?;
                if n > 0 {
                    pg_runtime_audit(
                        client,
                        &now,
                        "system",
                        "agent_proposal.update_status",
                        &format!("agent_proposal/{proposal_id}"),
                        &json!({"proposal_id": proposal_id, "new_status": new_status}),
                    )?;
                }
                Ok(n as usize)
            })?,
        };
        Ok(affected > 0)
    }

    pub fn list_proposals_by_run(
        &self,
        run_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT proposal_id, correlation_id, run_id, parent_node_id, agent_id,
                                target_agent_id, proposed_node_id, proposed_edge_id,
                                proposal_type, objective, context_summary, status,
                                created_at, updated_at
                         FROM agent_proposals WHERE run_id=?1
                         ORDER BY created_at DESC LIMIT ?2 OFFSET ?3",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![run_id, limit, offset], |row| {
                        Ok(json!({
                            "proposal_id": row.get::<_, String>(0)?,
                            "correlation_id": row.get::<_, String>(1)?,
                            "run_id": row.get::<_, String>(2)?,
                            "parent_node_id": row.get::<_, String>(3)?,
                            "agent_id": row.get::<_, String>(4)?,
                            "target_agent_id": row.get::<_, Option<String>>(5)?,
                            "proposed_node_id": row.get::<_, Option<String>>(6)?,
                            "proposed_edge_id": row.get::<_, Option<String>>(7)?,
                            "proposal_type": row.get::<_, String>(8)?,
                            "objective": row.get::<_, String>(9)?,
                            "context_summary": row.get::<_, String>(10)?,
                            "status": row.get::<_, String>(11)?,
                            "created_at": row.get::<_, String>(12)?,
                            "updated_at": row.get::<_, String>(13)?,
                        }))
                    })
                    .map_err(|e| e.to_string())?;
                let mut out = Vec::new();
                for r in rows {
                    out.push(r.map_err(|e| e.to_string())?);
                }
                Ok(out)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT proposal_id, correlation_id, run_id, parent_node_id, agent_id,
                                target_agent_id, proposed_node_id, proposed_edge_id,
                                proposal_type, objective, context_summary, status,
                                created_at, updated_at
                         FROM agent_proposals WHERE run_id=$1
                         ORDER BY created_at DESC LIMIT $2 OFFSET $3",
                        &[&run_id, &limit, &offset],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(rows.iter().map(pg_proposal_row).collect())
            }),
        }
    }

    pub fn find_pending_handoff_for_target(
        &self,
        correlation_id: &str,
        target_agent_id: &str,
        run_id: &str,
    ) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT proposal_id, correlation_id, run_id, parent_node_id, agent_id,
                                target_agent_id, proposed_node_id, proposed_edge_id,
                                proposal_type, objective, context_summary, status,
                                created_at, updated_at
                         FROM agent_proposals
                         WHERE correlation_id=?1
                           AND target_agent_id=?2
                           AND run_id=?3
                           AND proposal_type='handoff'
                           AND status='pending'
                         ORDER BY created_at DESC LIMIT 1",
                    )
                    .map_err(|e| e.to_string())?;
                let mut rows = stmt
                    .query_map(params![correlation_id, target_agent_id, run_id], |row| {
                        Ok(json!({
                            "proposal_id": row.get::<_, String>(0)?,
                            "correlation_id": row.get::<_, String>(1)?,
                            "run_id": row.get::<_, String>(2)?,
                            "parent_node_id": row.get::<_, String>(3)?,
                            "agent_id": row.get::<_, String>(4)?,
                            "target_agent_id": row.get::<_, Option<String>>(5)?,
                            "proposed_node_id": row.get::<_, Option<String>>(6)?,
                            "proposed_edge_id": row.get::<_, Option<String>>(7)?,
                            "proposal_type": row.get::<_, String>(8)?,
                            "objective": row.get::<_, String>(9)?,
                            "context_summary": row.get::<_, String>(10)?,
                            "status": row.get::<_, String>(11)?,
                            "created_at": row.get::<_, String>(12)?,
                            "updated_at": row.get::<_, String>(13)?,
                        }))
                    })
                    .map_err(|e| e.to_string())?;
                match rows.next() {
                    Some(r) => Ok(Some(r.map_err(|e| e.to_string())?)),
                    None => Ok(None),
                }
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT proposal_id, correlation_id, run_id, parent_node_id, agent_id,
                                target_agent_id, proposed_node_id, proposed_edge_id,
                                proposal_type, objective, context_summary, status,
                                created_at, updated_at
                         FROM agent_proposals
                         WHERE correlation_id=$1
                           AND target_agent_id=$2
                           AND run_id=$3
                           AND proposal_type='handoff'
                           AND status='pending'
                         ORDER BY created_at DESC LIMIT 1",
                        &[&correlation_id, &target_agent_id, &run_id],
                    )
                    .map_err(|e| e.to_string())?;
                match rows.into_iter().next() {
                    Some(row) => Ok(Some(pg_proposal_row(&row))),
                    None => Ok(None),
                }
            }),
        }
    }

    pub fn update_proposal_context_summary(
        &self,
        proposal_id: &str,
        new_context_summary: &str,
    ) -> Result<bool, String> {
        let capped = apply_size_cap(new_context_summary, MAX_PROPOSAL_CONTEXT_BYTES);
        let now = self.now();
        let affected = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let n = conn
                    .execute(
                        "UPDATE agent_proposals SET context_summary=?1, updated_at=?2
                         WHERE proposal_id=?3",
                        params![capped, now, proposal_id],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(n)
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let n = client
                    .execute(
                        "UPDATE agent_proposals SET context_summary=$1, updated_at=$2
                         WHERE proposal_id=$3",
                        &[&capped, &now, &proposal_id],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(n as usize)
            })?,
        };
        Ok(affected > 0)
    }

    pub fn find_proposal_by_correlation(
        &self,
        correlation_id: &str,
        agent_id: &str,
    ) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT proposal_id, correlation_id, run_id, parent_node_id, agent_id,
                                target_agent_id, proposed_node_id, proposed_edge_id,
                                proposal_type, objective, context_summary, status,
                                created_at, updated_at
                         FROM agent_proposals
                         WHERE correlation_id=?1 AND (agent_id=?2 OR target_agent_id=?2)
                         ORDER BY created_at DESC LIMIT 1",
                    )
                    .map_err(|e| e.to_string())?;
                let mut rows = stmt
                    .query_map(params![correlation_id, agent_id], |row| {
                        Ok(json!({
                            "proposal_id": row.get::<_, String>(0)?,
                            "correlation_id": row.get::<_, String>(1)?,
                            "run_id": row.get::<_, String>(2)?,
                            "parent_node_id": row.get::<_, String>(3)?,
                            "agent_id": row.get::<_, String>(4)?,
                            "target_agent_id": row.get::<_, Option<String>>(5)?,
                            "proposed_node_id": row.get::<_, Option<String>>(6)?,
                            "proposed_edge_id": row.get::<_, Option<String>>(7)?,
                            "proposal_type": row.get::<_, String>(8)?,
                            "objective": row.get::<_, String>(9)?,
                            "context_summary": row.get::<_, String>(10)?,
                            "status": row.get::<_, String>(11)?,
                            "created_at": row.get::<_, String>(12)?,
                            "updated_at": row.get::<_, String>(13)?,
                        }))
                    })
                    .map_err(|e| e.to_string())?;
                match rows.next() {
                    Some(r) => Ok(Some(r.map_err(|e| e.to_string())?)),
                    None => Ok(None),
                }
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT proposal_id, correlation_id, run_id, parent_node_id, agent_id,
                                target_agent_id, proposed_node_id, proposed_edge_id,
                                proposal_type, objective, context_summary, status,
                                created_at, updated_at
                         FROM agent_proposals
                         WHERE correlation_id=$1 AND (agent_id=$2 OR target_agent_id=$2)
                         ORDER BY created_at DESC LIMIT 1",
                        &[&correlation_id, &agent_id],
                    )
                    .map_err(|e| e.to_string())?;
                match rows.into_iter().next() {
                    Some(row) => Ok(Some(pg_proposal_row(&row))),
                    None => Ok(None),
                }
            }),
        }
    }
}

#[cfg(feature = "pg")]
fn pg_proposal_row(row: &postgres::Row) -> Value {
    json!({
        "proposal_id": row.get::<_, String>(0),
        "correlation_id": row.get::<_, String>(1),
        "run_id": row.get::<_, String>(2),
        "parent_node_id": row.get::<_, String>(3),
        "agent_id": row.get::<_, String>(4),
        "target_agent_id": row.get::<_, Option<String>>(5),
        "proposed_node_id": row.get::<_, Option<String>>(6),
        "proposed_edge_id": row.get::<_, Option<String>>(7),
        "proposal_type": row.get::<_, String>(8),
        "objective": row.get::<_, String>(9),
        "context_summary": row.get::<_, String>(10),
        "status": row.get::<_, String>(11),
        "created_at": row.get::<_, String>(12),
        "updated_at": row.get::<_, String>(13),
    })
}

// ── Build helper ─────────────────────────────────────────────────────────────

fn build_agent_state(
    agent_id: &str,
    run_id: &str,
    role: &str,
    caps_json: &str,
    objective: Option<&str>,
    status: &str,
    scratchpad_summary: Option<String>,
    redaction_filter: Option<String>,
    meta_json: &str,
    created_at: &str,
    updated_at: &str,
) -> AgentState {
    let caps: Vec<String> = serde_json::from_str(caps_json).unwrap_or_default();
    let meta_val: Value = serde_json::from_str(meta_json).unwrap_or(Value::Null);
    let meta = meta_val.as_object().cloned().unwrap_or_default();
    AgentState {
        schema_version: AGENT_STATE_SCHEMA_VERSION.to_string(),
        agent_id: agent_id.to_string(),
        run_id: run_id.to_string(),
        role: role.to_string(),
        capability_profile: caps,
        objective: objective.map(|s| s.to_string()),
        status: status.to_string(),
        scratchpad_summary,
        redaction_filter,
        metadata: meta.into_iter().collect(),
        created_at: created_at.to_string(),
        updated_at: updated_at.to_string(),
    }
}

fn build_mailbox_message(
    message_id: &str,
    correlation_id: Option<&str>,
    from_agent_id: &str,
    to_agent_id: &str,
    run_id: Option<&str>,
    node_id: Option<&str>,
    message_type: &str,
    status: &str,
    body: &Option<String>,
    body_summary: &Option<String>,
    redaction_status: &str,
    created_at: &str,
    read_at: Option<String>,
    ack_at: Option<String>,
    reply_to_message_id: Option<&str>,
    meta_json: &str,
) -> MailboxMessage {
    let meta_val: Value = serde_json::from_str(meta_json).unwrap_or(Value::Null);
    let meta = meta_val.as_object().cloned().unwrap_or_default();
    MailboxMessage {
        schema_version: AGENT_MESSAGE_SCHEMA_VERSION.to_string(),
        message_id: message_id.to_string(),
        correlation_id: correlation_id.map(|s| s.to_string()),
        from_agent_id: from_agent_id.to_string(),
        to_agent_id: to_agent_id.to_string(),
        run_id: run_id.map(|s| s.to_string()),
        node_id: node_id.map(|s| s.to_string()),
        message_type: message_type.to_string(),
        status: status.to_string(),
        body: body.clone(),
        body_summary: body_summary.clone(),
        redaction_status: redaction_status.to_string(),
        created_at: created_at.to_string(),
        read_at,
        ack_at,
        reply_to_message_id: reply_to_message_id.map(|s| s.to_string()),
        metadata: meta.into_iter().collect(),
    }
}

// ── SQLite row mappers ───────────────────────────────────────────────────────

fn sqlite_agent_state_row(row: &Row<'_>) -> rusqlite::Result<AgentState> {
    Ok(AgentState {
        schema_version: AGENT_STATE_SCHEMA_VERSION.to_string(),
        agent_id: row.get(0)?,
        run_id: row.get(1)?,
        role: row.get(2)?,
        capability_profile: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
        objective: row.get(4)?,
        status: row.get(5)?,
        scratchpad_summary: row.get(6)?,
        redaction_filter: row.get(7)?,
        metadata: {
            let v: Value = serde_json::from_str(&row.get::<_, String>(8)?).unwrap_or(Value::Null);
            v.as_object()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect()
        },
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn sqlite_mailbox_row(row: &Row<'_>) -> rusqlite::Result<MailboxMessage> {
    let meta_val: Value = serde_json::from_str(&row.get::<_, String>(15)?).unwrap_or(Value::Null);
    let meta = meta_val.as_object().cloned().unwrap_or_default();
    Ok(MailboxMessage {
        schema_version: AGENT_MESSAGE_SCHEMA_VERSION.to_string(),
        message_id: row.get(0)?,
        correlation_id: row.get(1)?,
        from_agent_id: row.get(2)?,
        to_agent_id: row.get(3)?,
        run_id: row.get(4)?,
        node_id: row.get(5)?,
        message_type: row.get(6)?,
        status: row.get(7)?,
        body: row.get(8)?,
        body_summary: row.get(9)?,
        redaction_status: row.get(10)?,
        created_at: row.get(11)?,
        read_at: row.get(12)?,
        ack_at: row.get(13)?,
        reply_to_message_id: row.get(14)?,
        metadata: meta.into_iter().collect(),
    })
}

// ── PostgreSQL row mappers ────────────────────────────────────────────────────

#[cfg(feature = "pg")]
fn pg_agent_state_row(row: &postgres::Row) -> AgentState {
    let meta_val: Value = serde_json::from_str(&row.get::<_, String>(8)).unwrap_or(Value::Null);
    let meta = meta_val.as_object().cloned().unwrap_or_default();
    AgentState {
        schema_version: AGENT_STATE_SCHEMA_VERSION.to_string(),
        agent_id: row.get(0),
        run_id: row.get(1),
        role: row.get(2),
        capability_profile: serde_json::from_str(&row.get::<_, String>(3)).unwrap_or_default(),
        objective: row.get(4),
        status: row.get(5),
        scratchpad_summary: row.get(6),
        redaction_filter: row.get(7),
        metadata: meta.into_iter().collect(),
        created_at: row.get(9),
        updated_at: row.get(10),
    }
}

#[cfg(feature = "pg")]
fn pg_mailbox_row(row: &postgres::Row) -> MailboxMessage {
    let meta_val: Value = serde_json::from_str(&row.get::<_, String>(15)).unwrap_or(Value::Null);
    let meta = meta_val.as_object().cloned().unwrap_or_default();
    MailboxMessage {
        schema_version: AGENT_MESSAGE_SCHEMA_VERSION.to_string(),
        message_id: row.get(0),
        correlation_id: row.get(1),
        from_agent_id: row.get(2),
        to_agent_id: row.get(3),
        run_id: row.get(4),
        node_id: row.get(5),
        message_type: row.get(6),
        status: row.get(7),
        body: row.get(8),
        body_summary: row.get(9),
        redaction_status: row.get(10),
        created_at: row.get(11),
        read_at: row.get(12),
        ack_at: row.get(13),
        reply_to_message_id: row.get(14),
        metadata: meta.into_iter().collect(),
    }
}

// ── Audit helper for PG ──────────────────────────────────────────────────────

#[cfg(feature = "pg")]
fn pg_runtime_audit(
    client: &mut postgres::Client,
    now: &str,
    actor: &str,
    action: &str,
    resource: &str,
    details: &Value,
) -> Result<(), String> {
    let details_json = details.to_string();
    client
        .execute(
            "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
         VALUES ($1, $2, $3, $4, $5)",
            &[&now, &actor, &action, &resource, &details_json],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ── Redaction helpers ─────────────────────────────────────────────────────────

fn redact_message_body(body: Option<&str>) -> (Option<String>, &'static str) {
    let body = match body {
        Some(b) => b,
        None => return (None, "none"),
    };
    if contains_sensitive_patterns(body) {
        let redacted = redact_sensitive_patterns(body);
        let capped = apply_size_cap(&redacted, MAX_BODY_BYTES);
        (Some(capped), "redacted")
    } else {
        let capped = apply_size_cap(body, MAX_BODY_BYTES);
        (Some(capped), "none")
    }
}

fn apply_size_cap(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut split = max_bytes;
    while split > 0 && !text.is_char_boundary(split) {
        split -= 1;
    }
    let mut result = text[..split].to_string();
    result.push_str(&format!(" [truncated {} bytes]", text.len() - split));
    result
}

fn apply_size_cap_and_redact(text: &str, max_bytes: usize) -> String {
    let redacted = redact_sensitive_patterns(text);
    apply_size_cap(&redacted, max_bytes)
}

fn esc_sql(s: &str) -> String {
    s.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_store() -> LocalProductStore {
        LocalProductStore::new(":memory:").expect("failed to create in-memory store")
    }

    fn create_test_agent(store: &LocalProductStore, agent_id: &str, run_id: &str) -> AgentState {
        store
            .create_agent_state(
                agent_id,
                run_id,
                "implementer",
                &["code".to_string(), "test".to_string()],
                Some("implement feature X"),
                "idle",
                &json!({"source": "test"}),
            )
            .expect("create_agent_state failed")
    }

    // ── AgentState tests ────────────────────────────────────────────────────

    #[test]
    fn test_create_agent_state() {
        let store = test_store();
        let state = create_test_agent(&store, "agent-1", "run-1");
        assert_eq!(state.agent_id, "agent-1");
        assert_eq!(state.run_id, "run-1");
        assert_eq!(state.role, "implementer");
        assert_eq!(state.status, "idle");
        assert_eq!(
            state.capability_profile,
            vec!["code".to_string(), "test".to_string()]
        );
        assert_eq!(state.objective, Some("implement feature X".to_string()));
    }

    #[test]
    fn test_get_agent_state() {
        let store = test_store();
        create_test_agent(&store, "agent-1", "run-1");
        let found = store
            .get_agent_state("agent-1", "run-1")
            .expect("get failed");
        assert!(found.is_some());
        assert_eq!(found.unwrap().agent_id, "agent-1");
    }

    #[test]
    fn test_get_agent_state_not_found() {
        let store = test_store();
        let found = store
            .get_agent_state("nonexistent", "run-1")
            .expect("get failed");
        assert!(found.is_none());
    }

    #[test]
    fn test_update_agent_state_status() {
        let store = test_store();
        create_test_agent(&store, "agent-1", "run-1");
        let updated = store
            .update_agent_state("agent-1", "run-1", Some("busy"), None, None, None)
            .expect("update failed");
        assert!(updated.is_some());
        assert_eq!(updated.unwrap().status, "busy");
    }

    #[test]
    fn test_update_agent_state_scratchpad() {
        let store = test_store();
        create_test_agent(&store, "agent-1", "run-1");
        let updated = store
            .update_agent_state(
                "agent-1",
                "run-1",
                None,
                Some("progress: 50% done"),
                None,
                None,
            )
            .expect("update failed");
        assert!(updated.is_some());
        assert_eq!(
            updated.unwrap().scratchpad_summary,
            Some("progress: 50% done".to_string())
        );
    }

    #[test]
    fn test_update_agent_state_nonexistent() {
        let store = test_store();
        let updated = store
            .update_agent_state("nonexistent", "run-1", Some("busy"), None, None, None)
            .expect("update failed");
        assert!(updated.is_none());
    }

    #[test]
    fn test_list_agent_state_by_run() {
        let store = test_store();
        create_test_agent(&store, "agent-1", "run-1");
        create_test_agent(&store, "agent-2", "run-1");
        create_test_agent(&store, "agent-3", "run-2");
        let agents = store.list_agent_state_by_run("run-1").expect("list failed");
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].agent_id, "agent-1");
        assert_eq!(agents[1].agent_id, "agent-2");
    }

    #[test]
    fn test_delete_agent_state() {
        let store = test_store();
        create_test_agent(&store, "agent-1", "run-1");
        let deleted = store
            .delete_agent_state("agent-1", "run-1")
            .expect("delete failed");
        assert!(deleted);
        let found = store
            .get_agent_state("agent-1", "run-1")
            .expect("get failed");
        assert!(found.is_none());
    }

    #[test]
    fn test_delete_agent_state_nonexistent() {
        let store = test_store();
        let deleted = store
            .delete_agent_state("nonexistent", "run-1")
            .expect("delete failed");
        assert!(!deleted);
    }

    // ── Mailbox tests ───────────────────────────────────────────────────────

    #[test]
    fn test_send_message() {
        let store = test_store();
        let msg = store
            .send_message(
                "msg-1",
                "agent-a",
                "agent-b",
                "task_assign",
                Some("hello from a to b"),
                None,
                Some("run-1"),
                Some("node-1"),
                None,
                &json!({"priority": "high"}),
            )
            .expect("send failed");
        assert_eq!(msg.message_id, "msg-1");
        assert_eq!(msg.from_agent_id, "agent-a");
        assert_eq!(msg.to_agent_id, "agent-b");
        assert_eq!(msg.status, "pending");
        assert_eq!(msg.run_id, Some("run-1".to_string()));
        assert_eq!(msg.node_id, Some("node-1".to_string()));
    }

    #[test]
    fn test_read_message() {
        let store = test_store();
        store
            .send_message(
                "msg-1",
                "agent-a",
                "agent-b",
                "task_assign",
                Some("body content"),
                None,
                None,
                None,
                None,
                &json!({}),
            )
            .expect("send failed");
        let msg = store.read_message("msg-1").expect("read failed");
        assert!(msg.is_some());
        assert_eq!(msg.unwrap().body, Some("body content".to_string()));
    }

    #[test]
    fn test_read_message_not_found() {
        let store = test_store();
        let msg = store.read_message("nonexistent").expect("read failed");
        assert!(msg.is_none());
    }

    #[test]
    fn test_ack_message() {
        let store = test_store();
        store
            .send_message(
                "msg-1",
                "agent-a",
                "agent-b",
                "task_assign",
                Some("body"),
                None,
                None,
                None,
                None,
                &json!({}),
            )
            .expect("send failed");
        let acked = store.ack_message("msg-1").expect("ack failed");
        assert!(acked.is_some());
        assert_eq!(acked.unwrap().status, "acked");
    }

    #[test]
    fn test_ack_message_twice_returns_none() {
        let store = test_store();
        store
            .send_message(
                "msg-1",
                "agent-a",
                "agent-b",
                "task_assign",
                Some("body"),
                None,
                None,
                None,
                None,
                &json!({}),
            )
            .expect("send failed");
        store.ack_message("msg-1").expect("first ack failed");
        let second = store.ack_message("msg-1").expect("second ack failed");
        // Second ack on already-acked message: no matching row updated
        assert!(second.is_none());
    }

    #[test]
    fn test_mark_message_read() {
        let store = test_store();
        store
            .send_message(
                "msg-1",
                "agent-a",
                "agent-b",
                "task_assign",
                Some("body"),
                None,
                None,
                None,
                None,
                &json!({}),
            )
            .expect("send failed");
        let read = store.mark_message_read("msg-1").expect("mark read failed");
        assert!(read.is_some());
        assert_eq!(read.unwrap().status, "read");
    }

    #[test]
    fn test_reply_to_message() {
        let store = test_store();
        store
            .send_message(
                "msg-1",
                "agent-a",
                "agent-b",
                "task_assign",
                Some("original body"),
                None,
                Some("run-1"),
                Some("node-1"),
                None,
                &json!({}),
            )
            .expect("send failed");
        let reply = store
            .reply_to_message(
                "msg-2",
                "msg-1",
                "agent-b",
                "agent-a",
                "result",
                Some("reply body"),
                &json!({}),
            )
            .expect("reply failed");
        assert!(reply.is_some());
        let r = reply.unwrap();
        assert_eq!(r.message_id, "msg-2");
        assert_eq!(r.from_agent_id, "agent-b");
        assert_eq!(r.to_agent_id, "agent-a");
        assert_eq!(r.reply_to_message_id, Some("msg-1".to_string()));
        // Original should now be 'replied'
        let original = store.read_message("msg-1").expect("read original failed");
        assert_eq!(original.unwrap().status, "replied");
    }

    #[test]
    fn test_reply_to_message_not_found() {
        let store = test_store();
        let reply = store
            .reply_to_message(
                "msg-2",
                "nonexistent",
                "agent-b",
                "agent-a",
                "result",
                Some("body"),
                &json!({}),
            )
            .expect("reply failed");
        assert!(reply.is_none());
    }

    #[test]
    fn test_list_mailbox_by_agent() {
        let store = test_store();
        store
            .send_message(
                "msg-1",
                "a",
                "target",
                "type1",
                None,
                None,
                None,
                None,
                None,
                &json!({}),
            )
            .expect("send");
        store
            .send_message(
                "msg-2",
                "b",
                "target",
                "type1",
                None,
                None,
                None,
                None,
                None,
                &json!({}),
            )
            .expect("send");
        store
            .send_message(
                "msg-3",
                "c",
                "other",
                "type1",
                None,
                None,
                None,
                None,
                None,
                &json!({}),
            )
            .expect("send");
        let msgs = store
            .list_mailbox(Some("target"), None, None, None, 100, 0)
            .expect("list failed");
        assert_eq!(msgs.len(), 2);
        for m in &msgs {
            assert_eq!(m.to_agent_id, "target");
        }
    }

    #[test]
    fn test_list_mailbox_by_run() {
        let store = test_store();
        store
            .send_message(
                "msg-1",
                "a",
                "b",
                "type1",
                None,
                None,
                Some("run-1"),
                None,
                None,
                &json!({}),
            )
            .expect("send");
        store
            .send_message(
                "msg-2",
                "a",
                "b",
                "type1",
                None,
                None,
                Some("run-2"),
                None,
                None,
                &json!({}),
            )
            .expect("send");
        let msgs = store
            .list_mailbox(None, Some("run-1"), None, None, 100, 0)
            .expect("list failed");
        assert_eq!(msgs.len(), 1);
    }

    #[test]
    fn test_list_mailbox_by_status() {
        let store = test_store();
        store
            .send_message(
                "msg-1",
                "a",
                "b",
                "type1",
                None,
                None,
                None,
                None,
                None,
                &json!({}),
            )
            .expect("send");
        store
            .send_message(
                "msg-2",
                "a",
                "b",
                "type1",
                None,
                None,
                None,
                None,
                None,
                &json!({}),
            )
            .expect("send");
        store.ack_message("msg-1").expect("ack");
        let pending = store
            .list_mailbox(None, None, None, Some("pending"), 100, 0)
            .expect("list failed");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].message_id, "msg-2");
    }

    #[test]
    fn test_count_mailbox() {
        let store = test_store();
        store
            .send_message(
                "msg-1",
                "a",
                "target",
                "type1",
                None,
                None,
                None,
                None,
                None,
                &json!({}),
            )
            .expect("send");
        store
            .send_message(
                "msg-2",
                "a",
                "target",
                "type1",
                None,
                None,
                None,
                None,
                None,
                &json!({}),
            )
            .expect("send");
        let count = store
            .count_mailbox(Some("target"), None, None)
            .expect("count failed");
        assert_eq!(count, 2);
    }

    #[test]
    fn test_list_mailbox_pagination() {
        let store = test_store();
        for i in 0..5 {
            let mid = format!("msg-{i}");
            store
                .send_message(
                    &mid,
                    "a",
                    "b",
                    "type1",
                    None,
                    None,
                    None,
                    None,
                    None,
                    &json!({}),
                )
                .expect("send");
        }
        let page1 = store
            .list_mailbox(None, None, None, None, 2, 0)
            .expect("list page1");
        assert_eq!(page1.len(), 2);
        let page2 = store
            .list_mailbox(None, None, None, None, 2, 2)
            .expect("list page2");
        assert_eq!(page2.len(), 2);
        // Ordered by message_sequence DESC, so page1 newest, page2 older
        assert_ne!(page1[0].message_id, page2[0].message_id);
    }

    // ── Correlation and run/node link tests ────────────────────────────────

    #[test]
    fn test_correlation_id_preserved() {
        let store = test_store();
        store
            .send_message(
                "msg-1",
                "a",
                "b",
                "task_assign",
                Some("body"),
                Some("corr-1"),
                Some("run-1"),
                Some("node-1"),
                None,
                &json!({}),
            )
            .expect("send failed");
        let msg = store.read_message("msg-1").expect("read failed").unwrap();
        assert_eq!(msg.correlation_id, Some("corr-1".to_string()));
        assert_eq!(msg.run_id, Some("run-1".to_string()));
        assert_eq!(msg.node_id, Some("node-1".to_string()));
    }

    #[test]
    fn test_correlation_id_inherited_on_reply() {
        let store = test_store();
        store
            .send_message(
                "msg-1",
                "a",
                "b",
                "task_assign",
                Some("body"),
                Some("corr-1"),
                Some("run-1"),
                Some("node-1"),
                None,
                &json!({}),
            )
            .expect("send failed");
        let reply = store
            .reply_to_message(
                "msg-2",
                "msg-1",
                "b",
                "a",
                "result",
                Some("reply"),
                &json!({}),
            )
            .expect("reply failed")
            .unwrap();
        assert_eq!(reply.correlation_id, Some("corr-1".to_string()));
        assert_eq!(reply.run_id, Some("run-1".to_string()));
        assert_eq!(reply.node_id, Some("node-1".to_string()));
    }

    // ── Redaction tests ─────────────────────────────────────────────────────

    #[test]
    fn test_message_body_redacted_for_secrets() {
        let store = test_store();
        let secret_body = "my api key is sk-live-a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6";
        store
            .send_message(
                "msg-1",
                "a",
                "b",
                "task_assign",
                Some(secret_body),
                None,
                None,
                None,
                None,
                &json!({}),
            )
            .expect("send failed");
        let msg = store.read_message("msg-1").expect("read failed").unwrap();
        assert_eq!(msg.redaction_status, "redacted");
        assert!(!msg.body.unwrap().contains("sk-live-"));
    }

    #[test]
    fn test_message_body_no_redaction_for_clean_text() {
        let store = test_store();
        store
            .send_message(
                "msg-1",
                "a",
                "b",
                "task_assign",
                Some("hello world"),
                None,
                None,
                None,
                None,
                &json!({}),
            )
            .expect("send failed");
        let msg = store.read_message("msg-1").expect("read failed").unwrap();
        assert_eq!(msg.redaction_status, "none");
        assert_eq!(msg.body, Some("hello world".to_string()));
    }

    #[test]
    fn test_body_truncated_at_max_bytes() {
        let store = test_store();
        let long_body = "x".repeat(MAX_BODY_BYTES + 100);
        store
            .send_message(
                "msg-1",
                "a",
                "b",
                "task_assign",
                Some(&long_body),
                None,
                None,
                None,
                None,
                &json!({}),
            )
            .expect("send failed");
        let msg = store.read_message("msg-1").expect("read failed").unwrap();
        let body = msg.body.unwrap();
        assert!(body.len() < long_body.len());
        assert!(body.contains("[truncated"));
    }

    #[test]
    fn test_scratchpad_redacted_for_secrets() {
        let store = test_store();
        create_test_agent(&store, "agent-1", "run-1");
        let bad_scratchpad = "token=sk-live-a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6";
        let updated = store
            .update_agent_state("agent-1", "run-1", None, Some(bad_scratchpad), None, None)
            .expect("update failed");
        assert!(updated.is_some());
        let scratchpad = updated.unwrap().scratchpad_summary.unwrap();
        assert!(!scratchpad.contains("sk-live-"));
        assert!(scratchpad.contains("***"));
    }

    #[test]
    fn test_audit_events_emitted_for_mailbox_actions() {
        let store = test_store();
        store
            .send_message(
                "msg-1",
                "a",
                "b",
                "task_assign",
                Some("body"),
                None,
                None,
                None,
                None,
                &json!({}),
            )
            .expect("send failed");
        store.mark_message_read("msg-1").expect("mark read failed");
        store.ack_message("msg-1").expect("ack failed");

        let events = store.audit_events(100).expect("audit events failed");
        let actions: Vec<&str> = events
            .iter()
            .filter_map(|e| e.get("action").and_then(|a| a.as_str()))
            .collect();
        assert!(actions.contains(&"agent_mailbox.send"));
        assert!(actions.contains(&"agent_mailbox.read"));
        assert!(actions.contains(&"agent_mailbox.ack"));
    }

    #[test]
    fn test_audit_events_emitted_for_agent_state_actions() {
        let store = test_store();
        create_test_agent(&store, "agent-1", "run-1");
        store
            .update_agent_state("agent-1", "run-1", Some("busy"), None, None, None)
            .expect("update failed");
        store
            .delete_agent_state("agent-1", "run-1")
            .expect("delete failed");

        let events = store.audit_events(100).expect("audit events failed");
        let actions: Vec<&str> = events
            .iter()
            .filter_map(|e| e.get("action").and_then(|a| a.as_str()))
            .collect();
        assert!(actions.contains(&"agent_state.create"));
        assert!(actions.contains(&"agent_state.update"));
        assert!(actions.contains(&"agent_state.delete"));
    }
}
