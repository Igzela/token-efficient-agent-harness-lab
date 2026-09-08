//! Thin operator CLI for Minimum First RWE live-baseline coordination.
//!
//! Provider-free by default. Never prints credentials, raw prompts, or private paths.
//! Composes exact frozen RWE cells under Product Golden Path + LocalProductStore.

use clap::{Parser, Subcommand};
use engine::rwe::live_baseline_coordinator::{
    issue_and_admit_v2_with_package, operator_preflight_read_only, project_first_baseline_evidence,
    recover_or_create_codex_subscription_golden_path_prerequisite, run_frozen_mx1_1x1x1,
    run_frozen_mx1_1x1x3, run_frozen_mx1_1x2x1, run_frozen_schedule, ProductGoldenPathCellDriver,
    RWE_LIVE_CELL_COMPOSITION_SEAM,
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
    /// Synchronize a terminal scheduler run through the existing ProductTask owner.
    /// Preserves workspace, provider journals, and unresolved effects.
    SyncProductTask {
        #[arg(long)]
        product_task_id: String,
    },
    /// Read a tenant-owned ProductTask's canonical research evidence projection.
    InspectProductTask {
        #[arg(long)]
        product_task_id: String,
    },
    /// Provider-free readiness check. Never consumes RWE authority.
    Preflight {
        #[arg(long)]
        authorization_id: Option<String>,
        #[arg(long)]
        golden_path_prerequisite_product_task_id: Option<String>,
    },
    /// Recover or create the exact-revision Product Golden Path prerequisite.
    /// Uses real Luna subscription calls and canonical Draft PR output.
    /// Does not merge the target default branch.
    Prerequisite {
        #[arg(long)]
        target_repo_path: String,
        /// Provisioned operator key id for the delegated attempt activator.
        #[arg(long)]
        cell_executor_key_id: String,
        /// Provisioned reviewer key id for independent artifact confirmation.
        #[arg(long)]
        cell_confirmer_key_id: String,
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
        /// Optional registered campaign package, e.g. the Codex subscription package.
        #[arg(long)]
        campaign_package_id: Option<String>,
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
        /// Execute the exact frozen MX1 1x2x1 matrix instead of the four-cell RWE schedule.
        #[arg(long, default_value_t = false)]
        mx1_1x2x1: bool,
        /// Execute the Luna-only exact frozen MX1 1x1x1 matrix.
        #[arg(long, default_value_t = false)]
        mx1_1x1x1: bool,
        /// Execute the Luna-only exact frozen MX1 1x1x3 strategy extension.
        #[arg(long, default_value_t = false)]
        mx1_1x1x3: bool,
        /// Registered campaign package selected for the driver.
        #[arg(long)]
        campaign_package_id: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();
    let read_only_preflight = matches!(
        &cli.command,
        Commands::Preflight { .. } | Commands::InspectProductTask { .. }
    );
    let store = if read_only_preflight {
        LocalProductStore::open_existing_read_only(&cli.db_path)
    } else {
        LocalProductStore::new(&cli.db_path)
    }
    .unwrap_or_else(|e| {
        eprintln!("store open failed: {e}");
        std::process::exit(2);
    });
    let store = std::sync::Arc::new(store);
    let principal = if read_only_preflight {
        store.authenticate_managed_acceptance_principal_read_only(
            &cli.tenant_id,
            &cli.operator_key_id,
            None,
        )
    } else {
        store.authenticate_managed_acceptance_principal(&cli.tenant_id, &cli.operator_key_id, None)
    }
    .unwrap_or_else(|e| {
        eprintln!("principal auth failed: {e}");
        std::process::exit(2);
    });

    let result = match cli.command {
        Commands::SyncProductTask { product_task_id } => (|| {
            if !principal.has_scope(engine::storage::local_product_store::SCOPE_DELEGATED_EXECUTE) {
                return Err("principal lacks delegated execution scope".into());
            }
            let task = store.get_product_task(&product_task_id)?.ok_or("ProductTask is missing")?;
            if task.get("tenant_id").and_then(serde_json::Value::as_str) != Some(principal.tenant_id()) {
                return Err("ProductTask tenant does not match principal".into());
            }
            let run_id = task.get("run_id").and_then(serde_json::Value::as_str)
                .ok_or("ProductTask has no scheduler run")?;
            let run = store.get_workflow_run(run_id)?.ok_or("scheduler run is missing")?;
            if !matches!(run.get("status").and_then(serde_json::Value::as_str), Some("failed" | "cancelled" | "killed")) {
                return Err("recovery synchronization requires a terminal unsuccessful run".into());
            }
            let synchronized = store.sync_product_task_from_run(&product_task_id, principal.principal_id())?;
            Ok(json!({
                "product_task_id": product_task_id,
                "previous_status": task.get("status"),
                "task_status": synchronized.get("status"),
                "run_id": run_id,
                "provider_call_performed": false,
                "workspace_cleanup_performed": false,
                "effect_reconciliation_performed": false,
            }))
        })(),
        Commands::InspectProductTask {
            product_task_id,
        } => {
            store.get_product_task(&product_task_id).and_then(|task| {
                let task = task.ok_or("ProductTask is missing")?;
                if task.get("tenant_id").and_then(serde_json::Value::as_str)
                    != Some(principal.tenant_id())
                {
                    return Err("ProductTask tenant does not match principal".into());
                }
                let run = task.get("run_id").and_then(serde_json::Value::as_str)
                    .map(|id| store.get_workflow_run(id)).transpose()?.flatten();
                Ok(json!({
                    "product_task_id": product_task_id,
                    "task_status": task.get("status"),
                    "source_revision": task.get("source_revision"),
                    "output_intent": task.get("output_intent"),
                    "run_id": task.get("run_id"),
                    "run_status": run.as_ref().and_then(|value| value.get("status")),
                    "nodes": run.as_ref().and_then(|value| value.get("nodes"))
                        .and_then(serde_json::Value::as_array).map(|nodes| nodes.iter().map(|node| json!({
                            "node_id": node.get("node_id"),
                            "status": node.get("status"),
                            "error_domain": node.pointer("/result/error_domain"),
                        })).collect::<Vec<_>>()),
                    "provider_call_performed": false,
                    "store_mutation_performed": false,
                }))
            })
        }
        Commands::Preflight {
            authorization_id,
            golden_path_prerequisite_product_task_id,
        } => operator_preflight_read_only(
            &store,
            &principal,
            authorization_id.as_deref(),
            golden_path_prerequisite_product_task_id.as_deref(),
        ),
        Commands::Prerequisite {
            target_repo_path,
            cell_executor_key_id,
            cell_confirmer_key_id,
        } => recover_or_create_codex_subscription_golden_path_prerequisite(
            &store,
            &principal,
            std::path::Path::new(&target_repo_path),
            &cell_executor_key_id,
            &cell_confirmer_key_id,
        ),
        Commands::Admit {
            authorization_id,
            run_id,
            golden_path_prerequisite_product_task_id,
            expires_at,
            campaign_package_id,
        } => issue_and_admit_v2_with_package(
            &store,
            &principal,
            &authorization_id,
            &run_id,
            &golden_path_prerequisite_product_task_id,
            &expires_at,
            campaign_package_id.as_deref(),
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
            mx1_1x2x1,
            mx1_1x1x1,
            mx1_1x1x3,
            campaign_package_id,
        } => {
            let campaign_package = campaign_package_id
                .as_deref()
                .map(engine::rwe::campaign_package::resolve_frozen_campaign_package)
                .transpose()
                .unwrap_or_else(|e| {
                    eprintln!("campaign package resolution failed: {e}");
                    std::process::exit(2);
                });
            let driver = ProductGoldenPathCellDriver {
                allow_live_provider_effects,
                target_repo_path: target_repo_path.map(std::path::PathBuf::from),
                fake_transport: None,
                cell_executor_key_id,
                cell_confirmer_key_id,
                campaign_package,
            };
            let selected_rungs = u8::from(mx1_1x1x1) + u8::from(mx1_1x1x3) + u8::from(mx1_1x2x1);
            if selected_rungs > 1 {
                eprintln!("select at most one MX1 rung");
                std::process::exit(2);
            }
            let run = if mx1_1x1x1 {
                run_frozen_mx1_1x1x1(
                    &store,
                    &principal,
                    &run_id,
                    &authorization_id,
                    &lease_token,
                    &driver,
                )
            } else if mx1_1x1x3 {
                run_frozen_mx1_1x1x3(
                    &store,
                    &principal,
                    &run_id,
                    &authorization_id,
                    &lease_token,
                    &driver,
                )
            } else if mx1_1x2x1 {
                run_frozen_mx1_1x2x1(
                    &store,
                    &principal,
                    &run_id,
                    &authorization_id,
                    &lease_token,
                    &driver,
                )
            } else {
                run_frozen_schedule(
                    &store,
                    &principal,
                    &run_id,
                    &authorization_id,
                    &lease_token,
                    &driver,
                )
            };
            run
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
