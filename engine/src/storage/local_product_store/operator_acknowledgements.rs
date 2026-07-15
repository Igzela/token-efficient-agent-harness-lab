use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::{append_audit_locked, DatabaseConnection, LocalProductStore};

type AcknowledgementRow = (
    String,
    String,
    String,
    String,
    Option<String>,
    String,
    String,
);

impl LocalProductStore {
    pub fn acknowledge_operator_source(
        &self,
        decision_id: &str,
        source_type: &str,
        source_id: &str,
        source_sha256: &str,
        reason: Option<&str>,
        actor: &str,
    ) -> Result<Value, String> {
        if source_sha256.len() != 64 || !source_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("operator acknowledgement requires exact source hash".to_string());
        }
        if reason.is_some_and(|value| value.len() > 1024) {
            return Err("operator acknowledgement reason is oversized".to_string());
        }
        let binding = json!({
            "source_type": source_type,
            "source_id": source_id,
            "source_sha256": source_sha256,
        });
        let acknowledgement_id = format!(
            "ack-{:x}",
            Sha256::digest(serde_json::to_vec(&binding).map_err(|error| error.to_string())?)
        );
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx=rusqlite::Transaction::new_unchecked(conn,TransactionBehavior::Immediate).map_err(|error|error.to_string())?;
                let existing:Option<AcknowledgementRow>=tx.query_row("SELECT decision_id,source_type,source_id,source_sha256,reason,actor,created_at FROM operator_acknowledgements WHERE acknowledgement_id=?1",params![acknowledgement_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?))).optional().map_err(|error|error.to_string())?;
                if let Some(existing)=existing {if (existing.1.as_str(),existing.2.as_str(),existing.3.as_str())!=(source_type,source_id,source_sha256){return Err("operator acknowledgement binding conflict".to_string())}return Ok(acknowledgement_value(&acknowledgement_id,&existing.0,&existing.1,&existing.2,&existing.3,existing.4.as_deref(),&existing.5,&existing.6))}
                tx.execute("INSERT INTO operator_acknowledgements (acknowledgement_id,decision_id,source_type,source_id,source_sha256,reason,actor,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![acknowledgement_id,decision_id,source_type,source_id,source_sha256,reason,actor,now]).map_err(|error|error.to_string())?;
                append_audit_locked(&tx,&now,actor,"operator_decision.acknowledge",decision_id,&json!({"acknowledgement_id":acknowledgement_id,"source_type":source_type,"source_id":source_id,"source_sha256":source_sha256,"approval_granted":false}))?;
                tx.commit().map_err(|error|error.to_string())?;Ok(acknowledgement_value(&acknowledgement_id,decision_id,source_type,source_id,source_sha256,reason,actor,&now))
            }),
            #[cfg(feature="pg")]
            DatabaseConnection::Pg(_)=>self.with_pg_conn(|client|{let mut tx=client.transaction().map_err(|error|error.to_string())?;tx.execute("SELECT pg_advisory_xact_lock(hashtext($1))", &[&acknowledgement_id]).map_err(|error|error.to_string())?;let existing=tx.query_opt("SELECT decision_id,source_type,source_id,source_sha256,reason,actor,created_at FROM operator_acknowledgements WHERE acknowledgement_id=$1 FOR UPDATE",&[&acknowledgement_id]).map_err(|error|error.to_string())?;if let Some(row)=existing{let existing:AcknowledgementRow=(row.get(0),row.get(1),row.get(2),row.get(3),row.get(4),row.get(5),row.get(6));if (existing.1.as_str(),existing.2.as_str(),existing.3.as_str())!=(source_type,source_id,source_sha256){return Err("operator acknowledgement binding conflict".to_string())}return Ok(acknowledgement_value(&acknowledgement_id,&existing.0,&existing.1,&existing.2,&existing.3,existing.4.as_deref(),&existing.5,&existing.6))}tx.execute("INSERT INTO operator_acknowledgements (acknowledgement_id,decision_id,source_type,source_id,source_sha256,reason,actor,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",&[&acknowledgement_id,&decision_id,&source_type,&source_id,&source_sha256,&reason,&actor,&now]).map_err(|error|error.to_string())?;tx.execute("INSERT INTO audit_log (created_at,actor,action,resource,details_json) VALUES ($1,$2,$3,$4,$5)",&[&now,&actor,&"operator_decision.acknowledge",&decision_id,&json!({"acknowledgement_id":acknowledgement_id,"source_type":source_type,"source_id":source_id,"source_sha256":source_sha256,"approval_granted":false}).to_string()]).map_err(|error|error.to_string())?;tx.commit().map_err(|error|error.to_string())?;Ok(acknowledgement_value(&acknowledgement_id,decision_id,source_type,source_id,source_sha256,reason,actor,&now))}),
        }
    }

    pub(crate) fn is_operator_source_acknowledged(
        &self,
        source_type: &str,
        source_id: &str,
        source_sha256: &str,
    ) -> Result<bool, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| conn.query_row("SELECT EXISTS(SELECT 1 FROM operator_acknowledgements WHERE source_type=?1 AND source_id=?2 AND source_sha256=?3)",params![source_type,source_id,source_sha256],|row|row.get(0)).map_err(|error|error.to_string())),
            #[cfg(feature="pg")]
            DatabaseConnection::Pg(_)=>self.with_pg_conn(|client|Ok(client.query_one("SELECT EXISTS(SELECT 1 FROM operator_acknowledgements WHERE source_type=$1 AND source_id=$2 AND source_sha256=$3)",&[&source_type,&source_id,&source_sha256]).map_err(|error|error.to_string())?.get(0))),
        }
    }
}

fn acknowledgement_value(
    acknowledgement_id: &str,
    decision_id: &str,
    source_type: &str,
    source_id: &str,
    source_sha256: &str,
    reason: Option<&str>,
    actor: &str,
    created_at: &str,
) -> Value {
    json!({"schema_version":"operator_acknowledgement.v1","acknowledgement_id":acknowledgement_id,"decision_id":decision_id,"source_type":source_type,"source_id":source_id,"source_sha256":source_sha256,"reason":reason,"actor":actor,"created_at":created_at,"approval_granted":false,"mutation_authority":"acknowledgement_only"})
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn acknowledgement_is_restart_safe_idempotent_and_never_approval() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("acknowledgements.db");
        let store = LocalProductStore::new_with_clock(&path, || "2026-07-14T00:00:00Z".to_string())
            .unwrap();
        let source_sha256 = "ab".repeat(32);
        let first = store
            .acknowledge_operator_source(
                "decision-1",
                "budget",
                "artifact-1",
                &source_sha256,
                Some("reviewed only"),
                "operator-a",
            )
            .unwrap();
        let repeated = store
            .acknowledge_operator_source(
                "decision-rederived-after-restart",
                "budget",
                "artifact-1",
                &source_sha256,
                Some("different retry text"),
                "operator-b",
            )
            .unwrap();
        assert_eq!(repeated, first);
        assert_eq!(first["approval_granted"], false);
        assert_eq!(first["mutation_authority"], "acknowledgement_only");
        assert_eq!(first["actor"], "operator-a");
        drop(store);

        let restarted = LocalProductStore::new(&path).unwrap();
        assert!(restarted
            .is_operator_source_acknowledged("budget", "artifact-1", &source_sha256)
            .unwrap());
        let audits = restarted
            .search_audit_events(10, 0, Some("operator_decision.acknowledge"))
            .unwrap();
        assert_eq!(audits.len(), 1);
        assert_eq!(audits[0]["details"]["approval_granted"], false);
    }
}
