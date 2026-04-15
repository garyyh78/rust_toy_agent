#include <iostream>

int main() {
    // Calculate and sum the first 30 Fibonacci numbers (F1 to F30)
    
    // Initialize first two Fibonacci numbers
    long long fib1 = 1;  // F1
    long long fib2 = 1;  // F2
    long long sum = fib1 + fib2;  // Start with sum of F1 and F2
    
    // Calculate Fibonacci numbers from F3 to F30
    for (int i = 3; i <= 30; i++) {
        long long next_fib = fib1 + fib2;
        sum += next_fib;
        
        // Update for next iteration
        fib1 = fib2;
        fib2 = next_fib;
    }
    
    // Print only the final sum
    std::cout << sum << std::endl;
    
    return 0;
}