#include <iostream>
#include <vector>
#include <cstdlib>
#include <ctime>
#include <algorithm>
#include <iomanip>

// Function to partition the array for quicksort
int partition(std::vector<int>& arr, int low, int high) {
    int pivot = arr[high];
    int i = low - 1;
    
    for (int j = low; j < high; j++) {
        if (arr[j] <= pivot) {
            i++;
            std::swap(arr[i], arr[j]);
        }
    }
    
    std::swap(arr[i + 1], arr[high]);
    return i + 1;
}

// Recursive quicksort function
void quicksort(std::vector<int>& arr, int low, int high) {
    if (low < high) {
        int pivot_index = partition(arr, low, high);
        quicksort(arr, low, pivot_index - 1);
        quicksort(arr, pivot_index + 1, high);
    }
}

// Wrapper function for quicksort
void quicksort(std::vector<int>& arr) {
    if (arr.empty()) return;
    quicksort(arr, 0, arr.size() - 1);
}

// Function to generate random numbers
std::vector<int> generate_random_numbers(int count, int min_val = 1, int max_val = 1000) {
    std::vector<int> numbers;
    numbers.reserve(count);
    
    for (int i = 0; i < count; i++) {
        numbers.push_back(min_val + std::rand() % (max_val - min_val + 1));
    }
    
    return numbers;
}

// Function to print array in a grid format
void print_array_grid(const std::vector<int>& arr, const std::string& label = "", int columns = 10) {
    if (!label.empty()) {
        std::cout << label << std::endl;
    }
    
    std::cout << std::string(columns * 5, '-') << std::endl;
    
    for (size_t i = 0; i < arr.size(); i++) {
        std::cout << std::setw(4) << arr[i] << " ";
        if ((i + 1) % columns == 0) {
            std::cout << std::endl;
        }
    }
    
    if (arr.size() % columns != 0) {
        std::cout << std::endl;
    }
    
    std::cout << std::string(columns * 5, '-') << std::endl;
}

int main() {
    // Seed the random number generator
    std::srand(static_cast<unsigned int>(std::time(nullptr)));
    
    const int NUM_COUNT = 100;
    
    std::cout << "=== QUICKSORT DEMONSTRATION ===" << std::endl;
    std::cout << "Generating " << NUM_COUNT << " random numbers (1-1000)..." << std::endl;
    
    std::vector<int> numbers = generate_random_numbers(NUM_COUNT);
    
    std::cout << "\nORIGINAL RANDOM NUMBERS (100 total):" << std::endl;
    print_array_grid(numbers, "", 10);
    
    std::cout << "\nSorting using quicksort algorithm..." << std::endl;
    quicksort(numbers);
    
    std::cout << "\nSORTED NUMBERS (ascending order):" << std::endl;
    print_array_grid(numbers, "", 10);
    
    // Verify the array is sorted
    if (std::is_sorted(numbers.begin(), numbers.end())) {
        std::cout << "\n✓ VERIFICATION: Array is correctly sorted!" << std::endl;
    } else {
        std::cout << "\n✗ VERIFICATION: Array is NOT sorted correctly!" << std::endl;
    }
    
    // Print statistics
    if (!numbers.empty()) {
        std::cout << "\n=== STATISTICS ===" << std::endl;
        std::cout << "Total numbers: " << NUM_COUNT << std::endl;
        std::cout << "Minimum value: " << numbers[0] << std::endl;
        std::cout << "Maximum value: " << numbers[NUM_COUNT - 1] << std::endl;
        
        // Calculate average
        double sum = 0;
        for (int num : numbers) {
            sum += num;
        }
        std::cout << "Average value: " << std::fixed << std::setprecision(2) << sum / NUM_COUNT << std::endl;
    }
    
    std::cout << "\nProgram completed successfully!" << std::endl;
    
    return 0;
}