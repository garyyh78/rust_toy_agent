#include <iostream>
#include <vector>
#include <cstdlib>
#include <ctime>
#include <algorithm> // for std::is_sorted

// Function to partition the array for quicksort
int partition(std::vector<int>& arr, int low, int high) {
    int pivot = arr[high]; // Choose the last element as pivot
    int i = low - 1; // Index of smaller element
    
    for (int j = low; j < high; j++) {
        // If current element is smaller than or equal to pivot
        if (arr[j] <= pivot) {
            i++; // Increment index of smaller element
            std::swap(arr[i], arr[j]);
        }
    }
    std::swap(arr[i + 1], arr[high]);
    return i + 1;
}

// Quicksort function
void quicksort(std::vector<int>& arr, int low, int high) {
    if (low < high) {
        // Partition the array and get the pivot index
        int pivotIndex = partition(arr, low, high);
        
        // Recursively sort elements before and after partition
        quicksort(arr, low, pivotIndex - 1);
        quicksort(arr, pivotIndex + 1, high);
    }
}

// Wrapper function for easier use
void quicksort(std::vector<int>& arr) {
    if (!arr.empty()) {
        quicksort(arr, 0, arr.size() - 1);
    }
}

// Function to generate random numbers
std::vector<int> generateRandomNumbers(int count, int minVal = 1, int maxVal = 1000) {
    std::vector<int> numbers(count);
    for (int i = 0; i < count; i++) {
        numbers[i] = minVal + rand() % (maxVal - minVal + 1);
    }
    return numbers;
}

// Function to print array
void printArray(const std::vector<int>& arr, const std::string& label = "") {
    if (!label.empty()) {
        std::cout << label << ": ";
    }
    
    for (size_t i = 0; i < arr.size(); i++) {
        std::cout << arr[i];
        if (i < arr.size() - 1) {
            std::cout << " ";
        }
    }
    std::cout << std::endl;
}

int main() {
    // Seed the random number generator
    srand(static_cast<unsigned int>(time(nullptr)));
    
    const int NUM_COUNT = 100;
    
    // Generate 100 random numbers
    std::vector<int> numbers = generateRandomNumbers(NUM_COUNT);
    
    std::cout << "=== Quicksort for " << NUM_COUNT << " Random Numbers ===" << std::endl;
    std::cout << std::endl;
    
    // Print original array (first 20 elements to avoid clutter)
    std::cout << "Original array (first 20 elements):" << std::endl;
    for (int i = 0; i < std::min(20, NUM_COUNT); i++) {
        std::cout << numbers[i] << " ";
    }
    std::cout << std::endl;
    
    // Make a copy for verification
    std::vector<int> original = numbers;
    
    // Perform quicksort
    std::cout << "\nSorting..." << std::endl;
    quicksort(numbers);
    
    // Print sorted array (first 20 elements)
    std::cout << "\nSorted array (first 20 elements):" << std::endl;
    for (int i = 0; i < std::min(20, NUM_COUNT); i++) {
        std::cout << numbers[i] << " ";
    }
    std::cout << std::endl;
    
    // Verify the sort
    bool isSorted = std::is_sorted(numbers.begin(), numbers.end());
    
    std::cout << "\n=== Verification ===" << std::endl;
    if (isSorted) {
        std::cout << "✓ Array is correctly sorted!" << std::endl;
    } else {
        std::cout << "✗ Array is NOT sorted correctly!" << std::endl;
    }
    
    // Additional statistics
    std::cout << "\n=== Statistics ===" << std::endl;
    std::cout << "Total numbers: " << NUM_COUNT << std::endl;
    std::cout << "Minimum value: " << numbers[0] << std::endl;
    std::cout << "Maximum value: " << numbers[NUM_COUNT - 1] << std::endl;
    
    // Check if all original elements are preserved
    std::sort(original.begin(), original.end());
    bool allElementsPreserved = (original == numbers);
    std::cout << "All original elements preserved: " 
              << (allElementsPreserved ? "Yes" : "No") << std::endl;
    
    return 0;
}