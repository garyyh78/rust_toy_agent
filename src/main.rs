//! main.rs - Binary entry point
//!
//! This is a thin wrapper that delegates to the `bin_core` modules.
//! All functionality is organized under `src/bin_core/`:
//!   - constants: Application constants
//!   - state: Agent state management
//!   - dispatch: Tool dispatch logic
//!   - `agent_loop`: Main agent loop
//!   - teammate: Teammate agent loop
//!   - repl: Interactive REPL
//!   - `test_mode`: End-to-end test harness

use rust_toy_agent::bin_core::{
    repl::{print_usage, run_repl},
    state::State,
    test_mode::{run_swe_bench_mode, run_test_mode},
};
use rust_toy_agent::llm_client::AnthropicClient;
use std::env;
use std::path::PathBuf;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let (file_writer, _guard) =
        tracing_appender::non_blocking(tracing_appender::rolling::daily("logs", "session.log"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr.and(file_writer))
        .with_ansi(true)
        .init();

    if let Err(e) = dotenvy::dotenv() {
        tracing::warn!(error = %e, "could not load .env");
    }

    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_usage();
        return;
    }

    let metrics_out = match args.iter().position(|a| a == "--metrics-out") {
        Some(idx) => args.get(idx + 1).map(PathBuf::from),
        None => None,
    };

    if let Some(idx) = args.iter().position(|a| a == "--test") {
        if let Some(test_name) = args.get(idx + 1) {
            run_test_mode(test_name).await;
            return;
        }
        tracing::error!("--test requires a test name argument");
        print_usage();
        std::process::exit(1);
    }

    if let Some(idx) = args.iter().position(|a| a == "--swe-bench") {
        if let Some(instance_id) = args.get(idx + 1) {
            run_swe_bench_mode(instance_id).await;
            return;
        }
        tracing::error!("--swe-bench requires an instance ID argument");
        print_usage();
        std::process::exit(1);
    }

    // REPL mode
    let workdir = env::current_dir().unwrap();
    let client = AnthropicClient::from_env();
    let model = env::var("MODEL_ID").unwrap_or_else(|_| {
        tracing::warn!("MODEL_ID not set, defaulting to claude-opus-4-6");
        "claude-opus-4-6".to_string()
    });

    let state = match State::new(client, model, workdir) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "failed to initialize state");
            std::process::exit(1);
        }
    };
    run_repl(state, metrics_out).await;
}
