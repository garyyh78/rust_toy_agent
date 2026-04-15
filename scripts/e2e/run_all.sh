#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
RESULTS_DIR="$PROJECT_DIR/task_tests/test_results"

mkdir -p "$RESULTS_DIR"

TESTS=(
    "api_mock"
    "bug_fix"
    "csv_transform"
    "dependency_resolve"
    "fibonacci_sum"
    "graph_bfs"
    "literary_style_detection"
    "multiline_transform"
    "prime_sum"
    "regex_extractor"
    "sum_1_to_n"
)

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

print_header() {
    echo -e "\n${BLUE}========================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}========================================${NC}\n"
}

print_result() {
    local test_name=$1
    local status=$2
    local duration=$3
    local output=$4

    if [ "$status" = "PASSED" ]; then
        echo -e "${GREEN}✓ $test_name${NC} - ${duration}s"
    else
        echo -e "${RED}✗ $test_name${NC} - ${duration}s"
        echo -e "  Output: $output"
    fi
}

run_test() {
    local test_name=$1
    local start_time=$(date +%s)

    print_header "Running: $test_name"

    local output
    if output=$(cd "$PROJECT_DIR" && cargo run -- --test "$test_name" 2>&1); then
        local end_time=$(date +%s)
        local duration=$((end_time - start_time))
        echo "$output" | tail -20

        local result_file="$RESULTS_DIR/${test_name}_$(date +%Y%m%d_%H%M%S).json"
        echo "$output" > "$result_file"

        print_result "$test_name" "PASSED" "$duration"
        return 0
    else
        local end_time=$(date +%s)
        local duration=$((end_time - start_time))
        echo "$output" | tail -20

        local result_file="$RESULTS_DIR/${test_name}_$(date +%Y%m%d_%H%M%S).json"
        echo "$output" > "$result_file"

        print_result "$test_name" "FAILED" "$duration" "$output"
        return 1
    fi
}

main() {
    print_header "E2E Test Suite"

    local passed=0
    local failed=0

    for test in "${TESTS[@]}"; do
        if run_test "$test"; then
            ((passed++))
        else
            ((failed++))
        fi
    done

    print_header "Results"
    echo -e "Passed: ${GREEN}$passed${NC}"
    echo -e "Failed: ${RED}$failed${NC}"
    echo -e "Total:  $((passed + failed))"

    if [ $failed -gt 0 ]; then
        exit 1
    fi
}

main "$@"