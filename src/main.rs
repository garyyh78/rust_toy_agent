//! main.rs - Agent entry point
//!
//! Wires up the library modules and runs the interactive REPL.
//!
//!   main.rs
//!     ├── agent_loop   ── agent_loop(), Messages
//!     ├── logger       ── SessionLogger
//!     ├── client       ── AnthropicClient
//!     ├── tools        ── TOOLS
//!     └── todo_manager ── TodoManager

use rust_toy_agent::agent_loop::{agent_loop, Messages};
use rust_toy_agent::client::AnthropicClient;
use rust_toy_agent::e2e_test::{load_test_case, print_test_result, run_test, save_test_result};
use rust_toy_agent::logger::SessionLogger;
use rust_toy_agent::todo_manager::TodoManager;
use rust_toy_agent::tools::TOOLS;
use serde_json::Value as Json;
use std::env;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

fn read_prompt(prompt: &str) -> Option<String> {
    print!("{prompt}");
    std::io::stdout().flush().ok();
    let mut line = String::new();
    match std::io::stdin().lock().read_line(&mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(line.trim().to_string()),
    }
}

fn extract_final_text(messages: &[serde_json::Value]) -> String {
    let mut text = String::new();
    if let Some(last) = messages.last() {
        if let Some(blocks) = last["content"].as_array() {
            for block in blocks {
                if block["type"] == "text" {
                    if let Some(t) = block["text"].as_str() {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(t);
                    }
                }
            }
        }
    }
    text
}

fn print_usage() {
    eprintln!("Usage: rust_toy_agent [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --test <test_name>    Run in end2end test mode with the specified test");
    eprintln!("  -h, --help            Show this help message");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  rust_toy_agent                    # Start in interactive REPL mode");
    eprintln!("  rust_toy_agent --test pi_series   # Run the pi_series test");
}

async fn run_test_mode(test_name: &str) {
    let workdir = env::current_dir().unwrap();
    let test_path = workdir.join("task_tests").join(test_name).join("test.json");
    let results_dir = workdir.join("task_tests").join("test_results");

    if !test_path.exists() {
        eprintln!("Error: Test '{}' not found at {}", test_name, test_path.display());
        std::process::exit(1);
    }

    let test_case = match load_test_case(&test_path) {
        Ok(tc) => tc,
        Err(e) => {
            eprintln!("Error loading test: {}", e);
            std::process::exit(1);
        }
    };

    eprintln!();
    eprintln!("\x1b[35m╔══════════════════════════════════════════════════════════════╗\x1b[0m");
    eprintln!("\x1b[35m║          End-to-End Test Mode                              ║\x1b[0m");
    eprintln!("\x1b[35m╚══════════════════════════════════════════════════════════════╝\x1b[0m");
    eprintln!();
    eprintln!("  Test: {}", test_case.name);
    eprintln!("  Path: {}", test_path.display());
    eprintln!();

    let client = AnthropicClient::from_env();
    let model = env::var("MODEL_ID").expect("MODEL_ID not set");

    let log_path = format!(
        "logs/test_{}_{}.log",
        test_name,
        chrono::Local::now().format("%Y%m%d_%H%M%S")
    );
    let mut logger = match SessionLogger::new(&log_path) {
        Ok(l) => {
            eprintln!("\x1b[36m  Log file  \x1b[0m {log_path}");
            l
        }
        Err(e) => {
            eprintln!("\x1b[33m  Warning: {e}\x1b[0m");
            SessionLogger::stderr_only()
        }
    };

    let todo = Arc::new(Mutex::new(TodoManager::new()));

    let test_workdir = workdir.join("task_tests").join(test_name).join("workspace");
        std::fs::create_dir_all(&test_workdir).ok();
        let result = run_test(&client, &model, &test_case, &test_workdir, &todo, &mut logger).await;

    if let Err(e) = save_test_result(&result, &results_dir) {
        eprintln!("Error saving test result: {}", e);
    } else {
        println!("  Result saved to: {}", results_dir.join(format!("{}_{}.json", result.name, result.test_time)).display());
    }

    commit_test_result(&result);

    print_test_result(&result);

    if result.passed {
        std::process::exit(0);
    } else {
        std::process::exit(1);
    }
}

fn commit_test_result(result: &rust_toy_agent::e2e_test::TestResult) {
    use std::process::Command;

    let workdir = env::current_dir().unwrap();
    let result_file = workdir.join("task_tests")
        .join("test_results")
        .join(format!("{}_{}.json", result.name, result.test_time));

    if !result_file.exists() {
        eprintln!("Warning: Result file {} not found, skipping git commit", result_file.display());
        return;
    }

    let add_status = Command::new("git")
        .args(["add", &result_file.to_string_lossy()])
        .current_dir(&workdir)
        .status();

    match add_status {
        Ok(status) => {
            if status.success() {
                let commit_msg = format!(
                    "Test result: {} - {} ({} ms, {} tokens)",
                    result.name,
                    if result.passed { "PASSED" } else { "FAILED" },
                    result.total_time_ms,
                    result.total_tokens
                );

                let commit_status = Command::new("git")
                    .args(["commit", "-m", &commit_msg])
                    .current_dir(&workdir)
                    .status();

                match commit_status {
                    Ok(cs) => {
                        if cs.success() {
                            eprintln!("\x1b[32m  Test result committed to git\x1b[0m");
                        }
                    }
                    Err(e) => eprintln!("Warning: Failed to commit: {}", e),
                }
            }
        }
        Err(e) => eprintln!("Warning: Failed to git add: {}", e),
    }
}

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

    let workdir = env::current_dir().unwrap();
    let client = AnthropicClient::from_env();
    let model = env::var("MODEL_ID").expect("MODEL_ID not set");
    let system = format!(
        "You are a coding agent at {}. \
Use the todo tool to plan multi-step tasks. Mark in_progress before starting, completed when done. \
Prefer tools over prose.",
        workdir.display()
    );
    let tools: Json = serde_json::from_str(TOOLS).unwrap();
    let todo = Arc::new(Mutex::new(TodoManager::new()));

    let log_path = format!(
        "logs/session_{}.log",
        chrono::Local::now().format("%Y%m%d_%H%M%S")
    );
    let mut logger = match SessionLogger::new(&log_path) {
        Ok(l) => {
            eprintln!("\x1b[36m  Log file  \x1b[0m {log_path}");
            l
        }
        Err(e) => {
            eprintln!("\x1b[33m  Warning: {e}\x1b[0m");
            SessionLogger::stderr_only()
        }
    };

    eprintln!();
    eprintln!("\x1b[35m╔══════════════════════════════════════════════════════════════╗\x1b[0m");
    eprintln!("\x1b[35m║          S03 Agent Loop - TodoWrite Edition                 ║\x1b[0m");
    eprintln!("\x1b[35m╚══════════════════════════════════════════════════════════════╝\x1b[0m");
    eprintln!();
    logger.log_info("model", &model);
    logger.log_info("workdir", &workdir.display().to_string());
    logger.log_info("tools", "bash, read_file, write_file, edit_file, todo");
    logger.log_info("max_tokens", "16384");
    eprintln!("\x1b[34m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m");
    eprintln!();
    logger.log_session_start(&model, &workdir.display().to_string());

    let mut history: Messages = Vec::new();
    let mut turn = 0usize;

    loop {
        let query = match read_prompt("\x1b[36ms03 >> \x1b[0m") {
            None => break,
            Some(q) => q,
        };
        if matches!(query.as_str(), "q" | "exit" | "") {
            logger.log_session_end();
            eprintln!("\x1b[35m  Session ended.\x1b[0m");
            break;
        }

        turn += 1;
        eprintln!();
        eprintln!(
            "\x1b[35m  Turn {turn}: {}\x1b[0m",
            &query[..std::cmp::min(50, query.len())]
        );
        eprintln!();
        logger.log_user_input(&query);

        history.push(serde_json::json!({"role": "user", "content": query}));
        agent_loop(
            &client,
            &model,
            &system,
            &tools,
            &mut history,
            &workdir,
            &todo,
            &mut logger,
        )
        .await;

        let response_text = extract_final_text(&history);
        logger.log_agent_response(&response_text);
        println!("{response_text}");
        println!();
    }
}
