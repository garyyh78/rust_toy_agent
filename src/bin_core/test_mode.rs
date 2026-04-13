use crate::e2e_test::{load_test_case, print_test_result, run_test, save_test_result};
use crate::llm_client::AnthropicClient;
use crate::todo_manager::TodoManager;

use std::env;
use std::sync::{Arc, Mutex};

/// Run the agent in test mode with a specific test case.
pub async fn run_test_mode(test_name: &str) {
    let workdir = env::current_dir().unwrap();
    let test_path = workdir.join("task_tests").join(test_name).join("test.json");
    let results_dir = workdir.join("task_tests").join("test_results");

    if !test_path.exists() {
        tracing::error!(test_name = %test_name, path = %test_path.display(), "test not found");
        std::process::exit(1);
    }

    let test_case = match load_test_case(&test_path) {
        Ok(tc) => tc,
        Err(e) => {
            tracing::error!(error = %e, "failed to load test");
            std::process::exit(1);
        }
    };

    eprintln!();
    eprintln!("╔══════════════════════════════════════════════════════════════╗");
    eprintln!("║          End-to-End Test Mode                              ║");
    eprintln!("╚══════════════════════════════════════════════════════════════╝");
    eprintln!();
    eprintln!("  Test: {}", test_case.name);
    eprintln!("  Path: {}", test_path.display());
    eprintln!();

    let client = AnthropicClient::from_env();
    let model = env::var("MODEL_ID").expect("MODEL_ID not set");

    let todo = Arc::new(Mutex::new(TodoManager::new()));

    let test_workdir = workdir.join("task_tests").join(test_name).join("workspace");
    if let Err(e) = std::fs::remove_dir_all(&test_workdir) {
        tracing::warn!(error = %e, "could not remove test_workdir");
    }
    if let Err(e) = std::fs::create_dir_all(&test_workdir) {
        tracing::warn!(error = %e, "could not create test_workdir");
    }

    // Use a no-op logger for test mode
    let mut logger = crate::logger::SessionLogger::stderr_only();

    let result = run_test(
        &client,
        &model,
        &test_case,
        &test_workdir,
        &todo,
        &mut logger,
    )
    .await;

    if let Err(e) = save_test_result(&result, &results_dir) {
        tracing::error!(error = %e, "failed to save test result");
    } else {
        println!(
            "  Result saved to: {}",
            results_dir
                .join(format!("{}_{}.json", result.name, result.test_time))
                .display()
        );
    }

    print_test_result(&result);

    if result.passed {
        std::process::exit(0);
    } else {
        std::process::exit(1);
    }
}
