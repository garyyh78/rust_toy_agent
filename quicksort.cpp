#include <iostream>
#include <vector>
#include <random>
#include <chrono>
#include <algorithm>

using namespace std;

// Function to partition the array
int partition(vector<int>& arr, int low, int high) {
    int pivot = arr[high];  // Choose the last element as pivot
    int i = low - 1;  // Index of smaller element
    
    for (int j = low; j < high; j++) {
        // If current element is smaller than or equal to pivot
        if (arr[j] <= pivot) {
            i++;  // Increment index of smaller element
            swap(arr[i], arr[j]);
        }
    }
    swap(arr[i + 1], arr[high]);
    return i + 1;
}

// Quicksort function
void quicksort(vector<int>& arr, int low, int high) {
    if (low < high) {
        // Partition the array
        int pi = partition(arr, low, high);
        
        // Recursively sort elements before and after partition
        quicksort(arr, low, pi - 1);
        quicksort(arr, pi + 1, high);
    }
}

// Wrapper function for easier use
void quicksort(vector<int>& arr) {
    if (!arr.empty()) {
        quicksort(arr, 0, arr.size() - 1);
    }
}

// Function to print array
void printArray(const vector<int>& arr, const string& label = "") {
    if (!label.empty()) {
        cout << label << ": ";
    }
    for (size_t i = 0; i < arr.size(); i++) {
        cout << arr[i];
        if (i < arr.size() - 1) cout << " ";
    }
    cout << endl;
}

int main() {
    // Set up random number generation
    random_device rd;
    mt19937 gen(rd());
    uniform_int_distribution<> dis(1, 1000);
    
    // Generate 100 random numbers
    const int SIZE = 100;
    vector<int> numbers(SIZE);
    
    cout << "Generating " << SIZE << " random numbers..." << endl;
    for (int i = 0; i < SIZE; i++) {
        numbers[i] = dis(gen);
    }
    
    // Print original array (first 20 elements to avoid too much output)
    cout << "\nFirst 20 elements of original array:" << endl;
    for (int i = 0; i < min(20, SIZE); i++) {
        cout << numbers[i] << " ";
    }
    cout << endl;
    
    // Check if array is sorted before sorting
    vector<int> original = numbers;
    
    // Sort using quicksort
    cout << "\nSorting using quicksort..." << endl;
    auto start = chrono::high_resolution_clock::now();
    quicksort(numbers);
    auto end = chrono::high_resolution_clock::now();
    
    // Calculate sorting time
    chrono::duration<double> elapsed = end - start;
    
    // Print sorted array (first 20 elements)
    cout << "\nFirst 20 elements of sorted array:" << endl;
    for (int i = 0; i < min(20, SIZE); i++) {
        cout << numbers[i] << " ";
    }
    cout << endl;
    
    // Verify sorting is correct
    vector<int> sorted_copy = original;
    sort(sorted_copy.begin(), sorted_copy.end());
    
    bool is_correct = (numbers == sorted_copy);
    
    // Print results
    cout << "\n=== Results ===" << endl;
    cout << "Array size: " << SIZE << " elements" << endl;
    cout << "Sorting time: " << elapsed.count() << " seconds" << endl;
    cout << "Sorting correct: " << (is_correct ? "YES" : "NO") << endl;
    
    // Print min and max values
    if (!numbers.empty()) {
        cout << "Minimum value: " << numbers[0] << endl;
        cout << "Maximum value: " << numbers[SIZE - 1] << endl;
    }
    
    return 0;
}