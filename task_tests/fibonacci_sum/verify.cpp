#include <iostream>
#include <vector>

int main() {
    // Let's manually calculate to verify
    std::vector<long long> fib(31);
    fib[1] = 1;  // F1
    fib[2] = 1;  // F2
    
    long long manual_sum = fib[1] + fib[2];
    
    for (int i = 3; i <= 30; i++) {
        fib[i] = fib[i-1] + fib[i-2];
        manual_sum += fib[i];
    }
    
    std::cout << "Manual calculation of sum of first 30 Fibonacci numbers: " << manual_sum << std::endl;
    
    // Also print individual Fibonacci numbers for verification
    std::cout << "\nFirst 10 Fibonacci numbers for reference:" << std::endl;
    for (int i = 1; i <= 10; i++) {
        std::cout << "F" << i << " = " << fib[i] << std::endl;
    }
    
    return 0;
}