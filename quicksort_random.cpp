#include <iostream>
#include <vector>
#include <cstdlib>
#include <ctime>
#include <algorithm>

// Function to partition the array for quicksort
int partition(std::vector<int>& arr, int low, int high) {
    int pivot = arr[high];  // Choose the last element as pivot
    int i = low - 1;  // Index of smaller element
    
    for (int j = low; j < high; j++) {
        // If current element is smaller than or equal to pivot
        if (arr[j] <= pivot) {
            i++;  // Increment index of smaller element
            std::swap(arr[i], arr[j]);
        }
    }
    std::swap(arr[i + 1], arr[high]);
    return i + 1;
}

// Quicksort function
void quicksort(std::vector<int>& arr, int low, int high) {
    if (low < high) {
        // Partition the array
        int pi = partition(arr, low, high);
        
        // Recursively sort elements before and after partition
        quicksort(arr, low, pi - 1);
        quicksort(arr, pi + 1, high);
    }
}

// Function to generate random numbers
std::vector<int> generateRandomNumbers(int count, int minVal = 1, int maxVal = 1000) {
    std::vector<int> numbers;
    numbers.reserve(count);
    
    for (int i = 0; i < count; i++) {
        int randomNum = minVal + (rand() % (maxVal - minVal + 1));
        numbers.push_back(randomNum);
    }
    
    return numbers;
}

// Function to print array
void printArray(const std::vector<int>& arr, const std::string& label) {
    std::cout << label << ":\n";
    for (size_t i = 0; i < arr.size(); i++) {
        std::cout << arr[i] << " ";
        if ((i + 1) % 10 == 0) {  // Print 10 numbers per line
            std::cout << std::endl;
        }
    }
    if (arr.size() % 10 != 0) {
        std::cout << std::endl;
    }
    std::cout << std::endl;
}

int main() {
    // Seed the random number generator
    srand(time(nullptr));
    
    const int NUM_COUNT = 100;
    
    // Generate 100 random numbers
    std::vector<int> numbers = generateRandomNumbers(NUM_COUNT);
    
    // Print original array
    printArray(numbers, "Original random numbers");
    
    // Make a copy for verification with std::sort
    std::vector<int> numbersCopy = numbers;
    
    // Sort using our quicksort implementation
    quicksort(numbers, 0, numbers.size() - 1);
    
    // Sort the copy using std::sort for verification
    std::sort(numbersCopy.begin(), numbersCopy.end());
    
    // Print sorted array
    printArray(numbers, "Sorted using quicksort");
    
    // Verify that our quicksort matches std::sort
    bool isCorrect = (numbers == numbersCopy);
    
    if (isCorrect) {
        std::cout << "✓ Quicksort implementation is correct!" << std::endl;
    } else {
        std::cout << "✗ Quicksort implementation has errors!" << std::endl;
    }
    
    // Print some statistics
    std::cout << "\nStatistics:" << std::endl;
    std::cout << "Minimum value: " << numbers[0] << std::endl;
    std::cout << "Maximum value: " << numbers[NUM_COUNT - 1] << std::endl;
    
    // Calculate median
    double median;
    if (NUM_COUNT % 2 == 0) {
        median = (numbers[NUM_COUNT/2 - 1] + numbers[NUM_COUNT/2]) / 2.0;
    } else {
        median = numbers[NUM_COUNT/2];
    }
    std::cout << "Median: " << median << std::endl;
    
    return 0;
}