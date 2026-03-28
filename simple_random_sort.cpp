#include <iostream>
#include <vector>
#include <algorithm>
#include <random>
#include <chrono>

int main() {
    // Seed random number generator
    unsigned seed = std::chrono::system_clock::now().time_since_epoch().count();
    std::mt19937 generator(seed);
    std::uniform_int_distribution<int> distribution(1, 1000);
    
    // Generate 100 random numbers
    std::vector<int> numbers(100);
    for (int i = 0; i < 100; ++i) {
        numbers[i] = distribution(generator);
    }
    
    std::cout << "Generated 100 random numbers:\n";
    for (int i = 0; i < 100; ++i) {
        std::cout << numbers[i] << " ";
        if ((i + 1) % 10 == 0) std::cout << "\n";
    }
    
    // Sort the numbers
    std::sort(numbers.begin(), numbers.end());
    
    std::cout << "\n\nSorted numbers:\n";
    for (int i = 0; i < 100; ++i) {
        std::cout << numbers[i] << " ";
        if ((i + 1) % 10 == 0) std::cout << "\n";
    }
    
    return 0;
}