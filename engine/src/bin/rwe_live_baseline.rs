//! Thin operator CLI for Minimum First RWE live-baseline coordination.
//!
//! Provider-free by default. Never prints credentials, raw prompts, or private paths.
//! Composes exact frozen RWE cells under Product Golden Path + LocalProductStore.

use clap::{Parser, Subcommand};
use engine::rwe::live_baseline_coordinator::{
    issue_and_admit_v2, operator_preflight, project_first_baseline_evidence, run_frozen_schedule,
    ProductGoldenPathCellDriver, RWE_LIVE_CELL_COMPOSITION_SEAM,
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
    /// Execute frozen 4-cell schedule under an admitted run.
    ///
    /// Production ProductGoldenPathCellDriver fails closed until the multi-path
    /// composition seam is authorized. Injected/test drivers are not exposed here.
    Run {
        #[arg(long)]
        authorization_id: String,
        #[arg(long)]
        run_id: String,
        /// Empty string triggers exact admit lease recovery.
        #[arg(long, default_value = "")]
        lease_token: String,
        /// Local clone of the frozen target (recorded only; live path still blocked).
        #[arg(long)]
        target_repo_path: Option<String>,
        /// When true, still fails closed until the composition seam exists.
        #[arg(long, default_value_t = false)]
        allow_live_provider_effects: bool,
        /// Provisioned operator key id for the role-separated delegated attempt
        /// activator (required when allow_live_provider_effects is true).
        #[arg(long)]
        cell_executor_key_id: Option<String>,
        /// Provisioned reviewer key id for the role-separated delegated artifact
        /// confirmer, distinct from the approver and activator keys (required
        /// when allow_live_provider_effects is true).
        #[arg(long)]
        cell_confirmer_key_id: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();
    let store = std::sync::Arc::new(LocalProductStore::new(&cli.db_path).unwrap_or_else(|e| {
        eprintln!("store open failed: {e}");
        std::process::exit(2);
    }));
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
            target_repo_path,
            allow_live_provider_effects,
            cell_executor_key_id,
            cell_confirmer_key_id,
        } => {
            let driver = ProductGoldenPathCellDriver {
                allow_live_provider_effects,
                target_repo_path: target_repo_path.map(std::path::PathBuf::from),
                fake_transport: None,
                cell_executor_key_id,
                cell_confirmer_key_id,
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
                let provider_call_performed = coord
                    .get("provider_call_performed")
                    .and_then(|v| v.as_bool())
                    .or_else(|| {
                        aggregate
                            .get("live_provider_request")
                            .and_then(|v| v.as_bool())
                    })
                    .unwrap_or(false);
                let provider_transport_provenance = coord
                    .get("provider_transport_provenance")
                    .or_else(|| aggregate.get("provider_transport_provenance"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("none");
                let injected_provider_call_performed = coord
                    .get("injected_provider_call_performed")
                    .or_else(|| aggregate.get("injected_provider_call_performed"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let integration_fixture_completed = coord
                    .get("integration_fixture_completed")
                    .or_else(|| aggregate.get("integration_fixture_completed"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let integration_fixture_succeeded = coord
                    .get("integration_fixture_succeeded")
                    .or_else(|| aggregate.get("integration_fixture_succeeded"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                json!({
                    "schema_version": "rwe_live_baseline_cli_run.v1",
                    "coordinator": coord,
                    "evidence_projection": project_first_baseline_evidence(&aggregate),
                    "provider_call_performed": provider_call_performed
                        && provider_transport_provenance == "external",
                    "provider_transport_provenance": provider_transport_provenance,
                    "injected_provider_call_performed": injected_provider_call_performed,
                    "integration_fixture_completed": integration_fixture_completed,
                    "integration_fixture_succeeded": integration_fixture_succeeded,
                    "target_write_performed": false,
                    "live_baseline_sealed": coord.get("live_baseline_sealed").cloned().unwrap_or(json!(false)),
                    "composition_seam": RWE_LIVE_CELL_COMPOSITION_SEAM,
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
