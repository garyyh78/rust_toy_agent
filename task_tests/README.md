# Task Tests

A collection of test cases for evaluating the coding agent's capabilities.

## Tests

| Test | Language | Description |
|------|----------|-------------|
| `api_mock` | Python | Parse JSON data, filter by score > 80, extract and sort names |
| `bug_fix` | Python | Find and fix 3 bugs in a buggy Python script for palindrome counting |
| `chinese_literary_style_5` | Python | Generate Chinese text mimicking 5 wuxia authors (金庸, 古龙, 梁羽生, 黄易, 温瑞安), then detect styles |
| `csv_transform` | Python | Read CSV, compute total revenue for 'West' region |
| `dependency_resolve` | Python | Implement topological sort on a dependency graph (A→B, A→C, B→D, C→D, D→E) |
| `fibonacci_sum` | C++ | Calculate and sum first 30 Fibonacci numbers |
| `graph_bfs` | Rust | Implement BFS on graph, print nodes in visit order |
| `literary_style_detection` | Python | Generate text files mimicking 4 authors, then analyze and detect styles |
| `multiline_transform` | Python | Read file, filter lines > 3 chars, sort by length descending |
| `prime_sum` | TypeScript | Find and sum first 1000 prime numbers |
| `regex_extractor` | Python | Use regex to extract emails from text file, sort alphabetically |
| `sum_1_to_n` | Python | Sum 1 to N=10000, verify with N*(N+1)/2 formula |

## Structure

Each test directory contains:
- `test.json` — test name, prompt, expected output, language
- `workspace/` — initial files provided to the agent
- Supporting files (scripts, data)

## Running Tests

The agent is given the prompt from `test.json` and must produce output matching `expected_output`.

Test results are stored in `test_results/` as JSON files.

## Latest Results (2026-04-24, MODEL_ID=claude-opus-4-6)

| Test | Passed | Time (ms) | Tokens | Notes |
|------|--------|-----------|--------|-------|
| multiline_transform | ✓ | 15,534 | 4,102 | |
| csv_transform | ✗ | 14,801 | 4,041 | Expected 47850, got 14000 (West region only) |
| sum_1_to_n | ✓ | 5,218 | 2,406 | |
| prime_sum | ✓ | 33,186 | 5,593 | |
| regex_extractor | ✓ | 28,129 | 7,705 | |
| fibonacci_sum | ✓ | 18,603 | 3,839 | |
| dependency_resolve | ✓ | 11,855 | 3,540 | |
| api_mock (json_parse) | ✗ | 11,920 | 3,900 | Expected alice,charlie, got all names (filter bug) |
| graph_bfs | ✓ | 22,905 | 5,761 | |
| bug_fix | ✓ | 181,315 | 66,209 | |
| literary_style_detection | ✓ | 51,065 | 10,304 | |
| chinese_literary_style_5 | ✓ | 77,148 | 27,773 | |

**Summary:** 10 passed, 2 failed (83.3% pass rate)