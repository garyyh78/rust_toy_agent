#include <iostream>
#include <vector>
#include <cstdlib>
#include <ctime>
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

// QuickSort function
void quickSort(vector<int>& arr, int low, int high) {
    if (low < high) {
        // Partition the array
        int pi = partition(arr, low, high);
        
        // Recursively sort elements before and after partition
        quickSort(arr, low, pi - 1);
        quickSort(arr, pi + 1, high);
    }
}

// Function to print the array
void printArray(const vector<int>& arr) {
    for (int num : arr) {
        cout << num << " ";
    }
    cout << endl;
}

// Function to generate random numbers
vector<int> generateRandomNumbers(int count, int minVal = 1, int maxVal = 100) {
    vector<int> numbers;
    for (int i = 0; i < count; i++) {
        numbers.push_back(minVal + rand() % (maxVal - minVal + 1));
    }
    return numbers;
}

int main() {
    // Seed the random number generator
    srand(time(0));
    
    // Generate 10 random numbers
    vector<int> numbers = generateRandomNumbers(10);
    
    cout << "Original array: ";
    printArray(numbers);
    
    // Sort the array using quicksort
    quickSort(numbers, 0, numbers.size() - 1);
    
    cout << "Sorted array: ";
    printArray(numbers);
    
    // Verify the sort is correct
    vector<int> sortedCopy = numbers;
    sort(sortedCopy.begin(), sortedCopy.end());
    
    if (numbers == sortedCopy) {
        cout << "✓ Array is correctly sorted!" << endl;
    } else {
        cout << "✗ Array is not correctly sorted!" << endl;
    }
    
    return 0;
}