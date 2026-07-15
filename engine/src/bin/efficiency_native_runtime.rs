use std::path::PathBuf;

use clap::Parser;
use engine::efficiency_benchmark_runtime::{run_files, RuntimeKind};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    benchmark_request: PathBuf,
    #[arg(long)]
    benchmark_output: PathBuf,
}

fn main() {
    let args = Args::parse();
    if let Err(error) = run_files(
        RuntimeKind::Native,
        &args.benchmark_request,
        &args.benchmark_output,
    ) {
        eprintln!("efficiency native runtime refused: {error}");
        std::process::exit(1);
    }
}
