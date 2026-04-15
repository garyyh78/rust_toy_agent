use std::collections::VecDeque;

fn bfs(start: usize, adj: &[Vec<usize>]) -> Vec<usize> {
    let n = adj.len();
    let mut visited = vec![false; n];
    let mut queue = VecDeque::new();
    let mut result = Vec::new();

    visited[start] = true;
    queue.push_back(start);

    while let Some(node) = queue.pop_front() {
        result.push(node);
        for &neighbor in &adj[node] {
            if !visited[neighbor] {
                visited[neighbor] = true;
                queue.push_back(neighbor);
            }
        }
    }

    result
}

fn main() {
    let adj: Vec<Vec<usize>> = vec![
        vec![1, 2], // 0
        vec![2, 3], // 1
        vec![],     // 2
        vec![4],    // 3
        vec![],     // 4
    ];

    let visited = bfs(0, &adj);
    let output: Vec<String> = visited.iter().map(|x| x.to_string()).collect();
    println!("{}", output.join(" "));
}
