#!/usr/bin/env python3
"""
Topological sort implementation for dependency graph.
Graph: A->B, A->C, B->D, C->D, D->E
"""

from collections import defaultdict, deque


def topological_sort(graph):
    """
    Perform topological sort using Kahn's algorithm.
    
    Args:
        graph: dict mapping nodes to list of dependent nodes
    
    Returns:
        list: topological ordering of nodes
    """
    # Calculate in-degree for each node
    in_degree = defaultdict(int)
    for node in graph:
        for neighbor in graph[node]:
            in_degree[neighbor] += 1
        # Ensure all nodes appear in in_degree dict
        if node not in in_degree:
            in_degree[node] = 0
    
    # Initialize queue with nodes having zero in-degree
    queue = deque([node for node in in_degree if in_degree[node] == 0])
    result = []
    
    while queue:
        node = queue.popleft()
        result.append(node)
        
        # Reduce in-degree of neighbors
        for neighbor in graph.get(node, []):
            in_degree[neighbor] -= 1
            if in_degree[neighbor] == 0:
                queue.append(neighbor)
    
    # Check for cycles
    if len(result) != len(in_degree):
        raise ValueError("Graph contains a cycle")
    
    return result


def main():
    # Define the graph: task -> list of tasks that depend on it
    graph = {
        'A': ['B', 'C'],  # A must come before B and C
        'B': ['D'],       # B must come before D
        'C': ['D'],       # C must come before D
        'D': ['E'],       # D must come before E
        'E': []           # E has no dependents
    }
    
    try:
        ordering = topological_sort(graph)
        print(" ".join(ordering))
    except ValueError as e:
        print(f"Error: {e}")


if __name__ == "__main__":
    main()