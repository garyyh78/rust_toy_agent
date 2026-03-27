//! s03_todo_write.rs - TodoWrite binary
//!
//! Entry point for the agent. Wires up the library modules and runs
//! the interactive REPL.
//!
//! Module dependency graph:
//!
//!   ┌─────────────────────────────────────────────────────────────┐
//!   │                 s03_todo_write.rs (binary)                  │
//!   ├─────────────────────────────────────────────────────────────┤
//!   │                                                             │
//!   │   Imports:                                                  │
//!   │     agent_loop ─── agent_loop(), Messages                   │
//!   │     logger    ─── log_info()                                │
//!   │     client    ─── AnthropicClient                           │
//!   │     tools     ─── TodoManager, TOOLS                        │
//!   │                                                             │
//!   │   main() flow:                                              │
//!   │     ┌──────────┐    ┌──────────┐    ┌──────────┐           │
//!   │     │  init    │───→│  REPL    │───→│  agent_  │           │
//!   │     │  client  │    │  loop    │    │  loop()  │           │
//!   │     │  tools   │    │  read    │    │  await   │           │
//!   │     │  todo    │    │  prompt  │    │          │           │
//!   │     └──────────┘    └────┬─────┘    └──────────┘           │
//!   │                         │                                  │
//!   │                    read_prompt() ── stdin                  │
//!   │                    print_final_response() ── stdout        │
//!   └─────────────────────────────────────────────────────────────┘
//!
//!     +----------+      +-------+      +---------+
//!     |   User   | ---> |  LLM  | ---> | Tools   |
//!     |  prompt  |      |       |      | + todo  |
//!     +----------+      +---+---+      +----+----+
//!                           ^               |
//!                           |   tool_result |
//!                           +---------------+
//!                                 |
//!                     +-----------+-----------+
//!                     | TodoManager state     |
//!                     | [ ] task A            |
//!                     | [>] task B <- doing   |
//!                     | [x] task C            |
//!                     +-----------------------+
//!                                 |
//!                     if rounds_since_todo >= 3:
//!                       inject <reminder>
//!
//! Key insight: "The agent can track its own progress -- and I can see it."

use rust_toy_agent::agent_loop::{agent_loop, Messages};
use rust_toy_agent::client::AnthropicClient;
use rust_toy_agent::logger::log_info;
use rust_toy_agent::tools::{TodoManager, TOOLS};
use serde_json::Value as Json;
use std::env;
use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex};

// -- REPL helpers --
// These stay in the binary because they're specific to the interactive
// terminal interface; a non-interactive binary wouldn't need them.

fn read_prompt(prompt: &str) -> Option<String> {
    print!("{prompt}");
    std::io::stdout().flush().ok();
    let mut line = String::new();
    match std::io::stdin().lock().read_line(&mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(line.trim().to_string()),
    }
}

fn print_final_response(messages: &[serde_json::Value]) {
    if let Some(last) = messages.last() {
        if let Some(blocks) = last["content"].as_array() {
            for block in blocks {
                if block["type"] == "text" {
                    if let Some(text) = block["text"].as_str() {
                        println!("{text}");
                    }
                }
            }
        }
    }
}

// -- Main entry point --
// Loads env, constructs the client and tools, then enters the REPL loop.

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

    // Print startup banner
    eprintln!();
    eprintln!("\x1b[35m╔══════════════════════════════════════════════════════════════╗\x1b[0m");
    eprintln!("\x1b[35m║          S03 Agent Loop - TodoWrite Edition                 ║\x1b[0m");
    eprintln!("\x1b[35m╚══════════════════════════════════════════════════════════════╝\x1b[0m");
    eprintln!();
    log_info("model", &model);
    log_info("workdir", &workdir.display().to_string());
    log_info("tools", "bash, read_file, write_file, edit_file, todo");
    log_info("max_tokens", "8000");
    eprintln!("\x1b[34m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m");
    eprintln!();

    // REPL: each turn reads input, runs agent_loop, prints the final text
    let mut history: Messages = Vec::new();
    let mut turn = 0usize;

    loop {
        let query = match read_prompt("\x1b[36ms03 >> \x1b[0m") {
            None => break,
            Some(q) => q,
        };
        if matches!(query.as_str(), "q" | "exit" | "") {
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

        // Push user message, run the agent, then display results
        history.push(serde_json::json!({"role": "user", "content": query}));
        agent_loop(
            &client,
            &model,
            &system,
            &tools,
            &mut history,
            &workdir,
            &todo,
        )
        .await;
        print_final_response(&history);
        println!();
    }
}
