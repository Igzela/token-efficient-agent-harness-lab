use clap::Parser;
use engine::local_scorecard_import::import_scorecard_artifacts;
use engine::storage::local_product_store::LocalProductStore;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "import-native-scorecard-artifacts",
    about = "Import bounded scorecard or token-efficiency regression JSON files into LocalProductStore"
)]
struct Args {
    #[arg(long, value_name = "PATH")]
    db: PathBuf,
    #[arg(long, default_value = "local-scorecard-import")]
    actor: String,
    #[arg(value_name = "FILE_OR_DIR", required = true)]
    inputs: Vec<PathBuf>,
}

fn main() {
    let args = Args::parse();
    let store = match LocalProductStore::new(&args.db) {
        Ok(store) => store,
        Err(error) => {
            eprintln!("failed to open LocalProductStore: {error}");
            std::process::exit(1);
        }
    };
    let summary = import_scorecard_artifacts(&store, &args.inputs, &args.actor);
    println!(
        "{}",
        serde_json::to_string_pretty(&summary).expect("summary serializes")
    );
    if !summary.errors.is_empty() {
        std::process::exit(1);
    }
}
