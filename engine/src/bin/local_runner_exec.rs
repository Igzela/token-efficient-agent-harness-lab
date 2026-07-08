use std::path::PathBuf;

use clap::Parser;

use engine::local_runner_provider::{build_config, build_provider, run_pair, ProviderKind};
use engine::trusted_local::EffectiveExecutionGates;

#[derive(Debug, Parser)]
#[command(
    name = "local-runner-exec",
    about = "Run a local stateful-vs-stateless experiment with optional provider support"
)]
struct Args {
    #[arg(long, default_value = "stub")]
    provider: String,

    #[arg(long, default_value_t = 10)]
    iterations: usize,

    #[arg(long, default_value_t = 40)]
    max_calls: usize,

    #[arg(long, default_value_t = 120000)]
    max_tokens: i64,

    #[arg(long, default_value_t = 30.0)]
    timeout_seconds: f64,

    #[arg(long, default_value_t = 0.25)]
    run_cost_cap_usd: f64,

    #[arg(long, default_value_t = 1.0)]
    daily_cost_cap_usd: f64,

    #[arg(long, default_value_t = 0.94)]
    pass_threshold: f64,

    #[arg(long)]
    output_dir: Option<PathBuf>,

    #[arg(long)]
    compare_only: bool,
}

fn main() {
    let args = Args::parse();

    let provider_kind = match args.provider.as_str() {
        "stub" => ProviderKind::Stub,
        "fake" => ProviderKind::Fake,
        "live" => ProviderKind::Live,
        _ => {
            eprintln!("error: provider must be 'stub', 'fake', or 'live'");
            std::process::exit(1);
        }
    };

    let config = match build_config(
        provider_kind,
        args.iterations,
        args.max_calls,
        args.max_tokens,
        args.timeout_seconds,
        args.run_cost_cap_usd,
        args.daily_cost_cap_usd,
        args.pass_threshold,
    ) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    let gates = if provider_kind == ProviderKind::Live {
        Some(EffectiveExecutionGates::from_env())
    } else {
        None
    };

    let provider = match build_provider(&config, gates.as_ref()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    if args.compare_only {
        match run_pair(&config, &provider) {
            Ok((stateless, stateful)) => {
                let output = serde_json::json!({
                    "stateless_reread": stateless,
                    "stateful_store": stateful,
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            }
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    match run_pair(&config, &provider) {
        Ok((stateless, stateful)) => {
            if let Some(dir) = args.output_dir {
                std::fs::create_dir_all(&dir).unwrap_or_else(|e| {
                    eprintln!("error: cannot create output dir: {e}");
                    std::process::exit(1);
                });
                let stateless_path = dir.join("stateless_reread.scorecard.json");
                let stateful_path = dir.join("stateful_store.scorecard.json");
                std::fs::write(
                    &stateless_path,
                    serde_json::to_string_pretty(&stateless).unwrap() + "\n",
                )
                .unwrap_or_else(|e| {
                    eprintln!("error: cannot write stateless scorecard: {e}");
                    std::process::exit(1);
                });
                std::fs::write(
                    &stateful_path,
                    serde_json::to_string_pretty(&stateful).unwrap() + "\n",
                )
                .unwrap_or_else(|e| {
                    eprintln!("error: cannot write stateful scorecard: {e}");
                    std::process::exit(1);
                });
                eprintln!("wrote scorecards to {dir:?}");
            } else {
                let output = serde_json::json!({
                    "stateless_reread": stateless,
                    "stateful_store": stateful,
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_provider_parsing() {
        let result = build_config(ProviderKind::Stub, 10, 40, 120000, 30.0, 0.25, 1.0, 0.94);
        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.provider_kind, ProviderKind::Stub);
    }

    #[test]
    fn binary_fake_provider_runs() {
        let config =
            build_config(ProviderKind::Fake, 10, 40, 120000, 30.0, 0.25, 1.0, 0.94).unwrap();
        let provider = build_provider(&config, None).unwrap();
        let (stateless, stateful) = run_pair(&config, &provider).unwrap();
        assert_eq!(stateless["mode"], "stateless_reread");
        assert!(stateful["status"] == "pass" || stateful["status"] == "fail");
    }
}
