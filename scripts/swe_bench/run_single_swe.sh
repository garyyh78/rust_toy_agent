#!/bin/bash
# Run SWE-bench Lite single test
# Usage: ./run_single_swe.sh <instance_id>
# Example: ./run_single_swe.sh django__django-12113

set -e

INSTANCE_ID="${1:-marshmallow-code__marshmallow-1359}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"

echo "Running SWE-bench Lite test for: $INSTANCE_ID"
echo ""

# Build and run the agent
cargo run -- --swe-bench "$INSTANCE_ID"

# Get the results directory
RESULTS_DIR="$PROJECT_DIR/swe_bench_results"
PREDICTION_FILE="$RESULTS_DIR/${INSTANCE_ID}.jsonl"

echo ""
echo "Running SWE-bench evaluation..."
echo ""

# Check if swebench is installed
if ! python3 -c "import swebench" 2>/dev/null; then
    echo "Installing swebench..."
    pip3 install swebench
fi

# Run evaluation
python3 -m swebench.harness.run_evaluation \
    --dataset_name princeton-nlp/SWE-bench_Lite \
    --predictions_path "$PREDICTION_FILE" \
    --max_workers 1 \
    --instance_ids "$INSTANCE_ID" \
    --run_id rust_toy_agent_"$INSTANCE_ID"

echo ""
echo "Done! Results saved to: $RESULTS_DIR"