#!/usr/bin/env python3

def sum_1_to_n_iterative(n):
    """Sum integers from 1 to n using iterative approach."""
    total = 0
    for i in range(1, n + 1):
        total += i
    return total

def sum_1_to_n_formula(n):
    """Sum integers from 1 to n using formula n*(n+1)/2."""
    return n * (n + 1) // 2

def main():
    N = 10000
    
    # Calculate sum using iterative method
    iterative_sum = sum_1_to_n_iterative(N)
    
    # Calculate sum using formula
    formula_sum = sum_1_to_n_formula(N)
    
    # Verify they match and print only the final verified answer
    if iterative_sum == formula_sum:
        print(iterative_sum)
    else:
        print(f"Error: Mismatch! Iterative: {iterative_sum}, Formula: {formula_sum}")

if __name__ == "__main__":
    main()