//! Thin operator CLI for Minimum First RWE live-baseline coordination.
//!
//! Provider-free by default. Never prints credentials, raw prompts, or private paths.

use clap::{Parser, Subcommand};
use engine::rwe::live_baseline_coordinator::{
    issue_and_admit_v2, operator_preflight, project_first_baseline_evidence, run_frozen_schedule,
    ProductGoldenPathCellDriver,
};
use engine::storage::local_product_store::LocalProductStore;
use serde_json::json;

#[derive(Parser, Debug)]
#[command(name = "rwe-live-baseline")]
#[command(about = "Provider-free RWE first-live-baseline coordinator CLI")]
struct Cli {
    /// SQLite store path (LocalProductStore owner).
    #[arg(long, default_value = "./data/local_product_store.db")]
    db_path: String,

    /// Tenant id for operator principal authentication.
    #[arg(long)]
    tenant_id: String,

    /// Operator API key id recorded in store metadata (not the raw secret).
    #[arg(long)]
    operator_key_id: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Provider-free readiness check. Never consumes RWE authority.
    Preflight {
        #[arg(long)]
        authorization_id: Option<String>,
        #[arg(long)]
        golden_path_prerequisite_product_task_id: Option<String>,
    },
    /// Issue v2 + admit run (store-owned bindings). No cell execution.
    Admit {
        #[arg(long)]
        authorization_id: String,
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        golden_path_prerequisite_product_task_id: String,
        #[arg(long)]
        expires_at: String,
    },
    /// Execute frozen 4-cell schedule under an admitted run (provider-free mode by default).
    Run {
        #[arg(long)]
        authorization_id: String,
        #[arg(long)]
        run_id: String,
        /// Empty string triggers exact admit lease recovery.
        #[arg(long, default_value = "")]
        lease_token: String,
        /// When true, arms ProductGoldenPathCellDriver for live effects (post-merge only).
        #[arg(long, default_value_t = false)]
        allow_live_provider_effects: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let store = LocalProductStore::new(&cli.db_path).unwrap_or_else(|e| {
        eprintln!("store open failed: {e}");
        std::process::exit(2);
    });
    let principal = store
        .authenticate_managed_acceptance_principal(&cli.tenant_id, &cli.operator_key_id, None)
        .unwrap_or_else(|e| {
            eprintln!("principal auth failed: {e}");
            std::process::exit(2);
        });

    let result = match cli.command {
        Commands::Preflight {
            authorization_id,
            golden_path_prerequisite_product_task_id,
        } => operator_preflight(
            &store,
            &principal,
            authorization_id.as_deref(),
            golden_path_prerequisite_product_task_id.as_deref(),
        ),
        Commands::Admit {
            authorization_id,
            run_id,
            golden_path_prerequisite_product_task_id,
            expires_at,
        } => issue_and_admit_v2(
            &store,
            &principal,
            &authorization_id,
            &run_id,
            &golden_path_prerequisite_product_task_id,
            &expires_at,
        )
        .map(|admitted| {
            json!({
                "schema_version": "rwe_live_baseline_cli_admit.v1",
                "admitted": admitted,
                "provider_call_performed": false,
                "target_write_performed": false,
            })
        }),
        Commands::Run {
            authorization_id,
            run_id,
            lease_token,
            allow_live_provider_effects,
        } => {
            let driver = ProductGoldenPathCellDriver {
                allow_live_provider_effects,
                fake_transport: None,
                work_root: None,
            };
            run_frozen_schedule(
                &store,
                &principal,
                &run_id,
                &authorization_id,
                &lease_token,
                &driver,
            )
            .map(|coord| {
                let aggregate = coord.get("aggregate").cloned().unwrap_or(json!({}));
                // Derive from authoritative aggregate/cell receipts — never hard-code false
                // for a successful future live run.
                let provider_call_performed = coord
                    .get("provider_call_performed")
                    .and_then(|v| v.as_bool())
                    .or_else(|| {
                        aggregate
                            .get("live_provider_request")
                            .and_then(|v| v.as_bool())
                    })
                    .unwrap_or(false);
                json!({
                    "schema_version": "rwe_live_baseline_cli_run.v1",
                    "coordinator": coord,
                    "evidence_projection": project_first_baseline_evidence(&aggregate),
                    "provider_call_performed": provider_call_performed,
                    "target_write_performed": false,
                    "live_baseline_sealed": coord.get("live_baseline_sealed").cloned().unwrap_or(json!(false)),
                })
            })
        }
    };

    match result {
        Ok(v) => {
            println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
            if v.get("ready").and_then(|x| x.as_bool()) == Some(false) {
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
