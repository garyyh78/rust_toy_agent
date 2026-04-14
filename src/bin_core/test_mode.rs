use crate::e2e_test::{load_test_case, print_test_result, run_test, save_test_result};
use crate::llm_client::AnthropicClient;
use crate::todo_manager::TodoManager;
use crate::tool_runners::WorkdirRoot;

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
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

    let workdir_root = WorkdirRoot::new(&test_workdir).expect("Failed to create workdir root");

    // Use a no-op logger for test mode
    let mut logger = crate::logger::SessionLogger::stderr_only();

    let result = run_test(
        &client,
        &model,
        &test_case,
        &workdir_root,
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

const SWE_BENCH_INSTANCE_ID: &str = "django__django-12113";
const SWE_BENCH_REPO_URL: &str = "https://github.com/django/django.git";
const SWE_BENCH_COMMIT: &str = "62254c5202e80a68f4fe6572a2be46a3d953de1a";

pub async fn run_swe_bench_mode(instance_id: &str) {
    let workdir = env::current_dir().unwrap();
    let swe_dir = workdir.join("swe_bench_data").join(instance_id);
    let results_dir = workdir.join("swe_bench_results");

    eprintln!();
    eprintln!("╔══════════════════════════════════════════════════════════════╗");
    eprintln!("║          SWE-bench Evaluation Mode                           ║");
    eprintln!("╚══════════════════════════════════════════════════════════════╝");
    eprintln!();
    eprintln!("  Instance: {instance_id}");
    eprintln!("  Output: {}", swe_dir.display());
    eprintln!();

    if let Err(e) = std::fs::create_dir_all(&swe_dir) {
        tracing::error!(error = %e, "failed to create swe_bench directory");
        std::process::exit(1);
    }

    setup_swe_instance(&swe_dir, instance_id);

    let repo_dir = swe_dir.join("repo");
    let problem_statement = get_problem_statement(instance_id);
    let prompt = format!(
        "You are working on a bug fix in the repository at {}.\n\n\
        Problem Description:\n{}\n\n\
        Your task is to:\n\
        1. Explore the codebase to understand the issue\n\
        2. Make the necessary code changes to fix the bug\n\
        3. Ensure your changes are correct by testing if needed\n\n\
        Use the todo tool to plan your approach.",
        repo_dir.display(),
        problem_statement
    );

    let test_case = crate::e2e_test::TestCase {
        name: instance_id.to_string(),
        prompt,
        expected_output: String::new(),
        language: Some("python".to_string()),
    };

    let client = AnthropicClient::from_env();
    let model = env::var("MODEL_ID").expect("MODEL_ID not set");

    let todo = Arc::new(Mutex::new(TodoManager::new()));
    if let Err(e) = std::fs::create_dir_all(&results_dir) {
        tracing::error!(error = %e, "failed to create results directory");
    }
    let log_path = results_dir.join(format!("session_{instance_id}.log"));
    let mut logger =
        crate::logger::SessionLogger::new(log_path.to_str().unwrap()).unwrap_or_else(|e| {
            tracing::error!(error = %e, "failed to create session logger, falling back to stderr");
            crate::logger::SessionLogger::stderr_only()
        });

    let repo_dir = swe_dir.join("repo");
    let workdir_root = WorkdirRoot::new(&repo_dir).expect("Failed to create workdir root");

    eprintln!("Running agent on SWE-bench instance...");
    eprintln!();

    let result = run_test(
        &client,
        &model,
        &test_case,
        &workdir_root,
        &todo,
        &mut logger,
    )
    .await;

    let patch = extract_patch_from_workdir(&repo_dir);
    save_predictions(instance_id, &model, &patch, &results_dir);

    eprintln!();
    eprintln!("Agent completed.");
    eprintln!("Steps: {}", result.steps);
    eprintln!("Time: {} ms", result.total_time_ms);
    eprintln!("Tokens: {}", result.total_tokens);
    eprintln!();
    eprintln!(
        "Patch extracted to: {}",
        results_dir.join(format!("{instance_id}.jsonl")).display()
    );
    eprintln!();
    eprintln!("To evaluate, run:");
    eprintln!("  python -m swebench.harness.run_evaluation \\");
    eprintln!("    --dataset_name princeton-nlp/SWE-bench_Lite \\");
    eprintln!(
        "    --predictions_path {} \\",
        results_dir.join(format!("{instance_id}.jsonl")).display()
    );
    eprintln!("    --max_workers 1 \\");
    eprintln!("    --instance_ids {instance_id} \\");
    eprintln!("    --run_id rust_toy_agent_test");
}

fn setup_swe_instance(workdir: &Path, instance_id: &str) {
    eprintln!("Setting up SWE-bench instance: {instance_id}");

    let repo_dir = workdir.join("repo");
    if repo_dir.exists() {
        eprintln!("  Repository already exists, skipping clone.");
        return;
    }

    eprintln!("  Cloning repository...");
    let clone_output = Command::new("git")
        .args(["clone", SWE_BENCH_REPO_URL, repo_dir.to_str().unwrap()])
        .output();

    match clone_output {
        Ok(output) if output.status.success() => eprintln!("  Cloned successfully."),
        Ok(output) => {
            eprintln!(
                "  Clone failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("  Clone error: {e}");
            std::process::exit(1);
        }
    }

    eprintln!("  Checking out commit...");
    let checkout_output = Command::new("git")
        .args(["checkout", SWE_BENCH_COMMIT])
        .current_dir(&repo_dir)
        .output();

    match checkout_output {
        Ok(output) if output.status.success() => eprintln!("  Checked out commit."),
        Ok(output) => {
            eprintln!(
                "  Checkout failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("  Checkout error: {e}");
            std::process::exit(1);
        }
    }
}

fn get_problem_statement(instance_id: &str) -> String {
    let workdir = env::current_dir().unwrap();
    let instance_file = workdir.join("swe_bench_instances.json");

    if instance_file.exists() {
        if let Ok(content) = std::fs::read_to_string(&instance_file) {
            if let Ok(instances) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(problem) = instances
                    .get(instance_id)
                    .and_then(|v| v.get("problem_statement"))
                    .and_then(|v| v.as_str())
                {
                    return problem.to_string();
                }
            }
        }
    }

    fetch_problem_from_huggingface(instance_id).unwrap_or_else(|_| {
        if instance_id == "django__django-12113" {
            return "admin_views.test_multidb fails with persistent test SQLite database. \
            When using persistent SQLite databases for tests (to make use of --keepdb), \
            at least some test fails with: sqlite3.OperationalError: database is locked."
                .to_string();
        }
        format!("Fix the bug in instance: {instance_id}")
    })
}

fn fetch_problem_from_huggingface(instance_id: &str) -> Result<String, String> {
    let output = Command::new("python3")
        .args([
            "-c",
            &format!(
                r#"
from datasets import load_dataset
ds = load_dataset('princeton-nlp/SWE-bench_Lite', split='test')
for item in ds:
    if item['instance_id'] == '{instance_id}':
        print(item['problem_statement'].replace('\n', '\\n'))
        break
"#
            ),
        ])
        .output()
        .map_err(|e| format!("Failed to run python: {e}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

fn extract_patch_from_workdir(workdir: &Path) -> String {
    if !workdir.exists() {
        return String::new();
    }

    let output = Command::new("git")
        .args(["diff"])
        .current_dir(workdir)
        .output();

    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => String::new(),
    }
}

fn save_predictions(instance_id: &str, model: &str, patch: &str, results_dir: &PathBuf) {
    if let Err(e) = std::fs::create_dir_all(results_dir) {
        tracing::error!(error = %e, "failed to create results directory");
        return;
    }

    let pred = serde_json::json!({
        "instance_id": instance_id,
        "model_name_or_path": model,
        "model_patch": patch
    });

    let filepath = results_dir.join(format!("{instance_id}.jsonl"));
    if let Ok(json_str) = serde_json::to_string(&pred) {
        let _ = std::fs::write(&filepath, json_str);
    }
}
