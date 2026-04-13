//! main.rs - Binary entry point
//!
//! This is a thin wrapper that delegates to the bin_core modules.
//! All functionality is organized under src/bin_core/:
//!   - constants: Application constants
//!   - state: Agent state management
//!   - dispatch: Tool dispatch logic
//!   - agent_loop: Main agent loop
//!   - teammate: Teammate agent loop
//!   - repl: Interactive REPL
//!   - test_mode: End-to-end test harness

use rust_toy_agent::bin_core::{
    repl::{print_usage, run_repl},
    state::State,
    test_mode::run_test_mode,
};
use rust_toy_agent::llm_client::AnthropicClient;

use std::env;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_usage();
        return;
    }

    if let Some(idx) = args.iter().position(|a| a == "--test") {
        if let Some(test_name) = args.get(idx + 1) {
            run_test_mode(test_name).await;
            return;
        } else {
            eprintln!("Error: --test requires a test name argument");
            print_usage();
            std::process::exit(1);
        }
    }

    // REPL mode
    let workdir = env::current_dir().unwrap();
    let client = AnthropicClient::from_env();
    let model = env::var("MODEL_ID").expect("MODEL_ID not set");

    let state = State::new(client, model, workdir);
    run_repl(state).await;
}
