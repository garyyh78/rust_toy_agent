#include <iostream>
#include <vector>
#include <algorithm>
#include <random>
#include <chrono>

int main() {
    // Seed the random number generator with current time
    unsigned seed = std::chrono::system_clock::now().time_since_epoch().count();
    std::mt19937 generator(seed);
    std::uniform_int_distribution<int> distribution(1, 1000);
    
    // Create a vector to store 100 random numbers
    std::vector<int> numbers;
    numbers.reserve(100);
    
    std::cout << "Generating 100 random numbers between 1 and 1000...\n\n";
    
    // Generate 100 random numbers
    for (int i = 0; i < 100; ++i) {
        numbers.push_back(distribution(generator));
    }
    
    // Display original numbers
    std::cout << "Original numbers:\n";
    for (size_t i = 0; i < numbers.size(); ++i) {
        std::cout << numbers[i] << " ";
        if ((i + 1) % 10 == 0) {
            std::cout << "\n";
        }
    }
    std::cout << "\n\n";
    
    // Make a copy for sorting
    std::vector<int> sorted_numbers = numbers;
    
    // Sort the numbers
    std::sort(sorted_numbers.begin(), sorted_numbers.end());
    
    // Display sorted numbers
    std::cout << "Sorted numbers (ascending order):\n";
    for (size_t i = 0; i < sorted_numbers.size(); ++i) {
        std::cout << sorted_numbers[i] << " ";
        if ((i + 1) % 10 == 0) {
            std::cout << "\n";
        }
    }
    std::cout << "\n\n";
    
    // Display some statistics
    std::cout << "Statistics:\n";
    std::cout << "Minimum value: " << sorted_numbers[0] << "\n";
    std::cout << "Maximum value: " << sorted_numbers[sorted_numbers.size() - 1] << "\n";
    
    // Calculate average
    double sum = 0;
    for (int num : sorted_numbers) {
        sum += num;
    }
    double average = sum / sorted_numbers.size();
    std::cout << "Average value: " << average << "\n";
    
    // Median
    double median;
    if (sorted_numbers.size() % 2 == 0) {
        median = (sorted_numbers[sorted_numbers.size() / 2 - 1] + 
                  sorted_numbers[sorted_numbers.size() / 2]) / 2.0;
    } else {
        median = sorted_numbers[sorted_numbers.size() / 2];
    }
    std::cout << "Median value: " << median << "\n";
    
    return 0;
}