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