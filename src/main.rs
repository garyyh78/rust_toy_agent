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
use rust_toy_agent::logger::SessionLogger;
use rust_toy_agent::todo_manager::TodoManager;
use rust_toy_agent::tools::TOOLS;
use serde_json::Value as Json;
use std::env;
use std::io::{BufRead, Write};
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

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

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

    // Create session logger: stderr + file
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

    // Print startup banner
    eprintln!();
    eprintln!("\x1b[35m╔══════════════════════════════════════════════════════════════╗\x1b[0m");
    eprintln!("\x1b[35m║          S03 Agent Loop - TodoWrite Edition                 ║\x1b[0m");
    eprintln!("\x1b[35m╚══════════════════════════════════════════════════════════════╝\x1b[0m");
    eprintln!();
    logger.log_info("model", &model);
    logger.log_info("workdir", &workdir.display().to_string());
    logger.log_info("tools", "bash, read_file, write_file, edit_file, todo");
    logger.log_info("max_tokens", "8000");
    eprintln!("\x1b[34m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m");
    eprintln!();
    logger.log_session_start(&model, &workdir.display().to_string());

    // REPL
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
