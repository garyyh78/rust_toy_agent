def is_palindrome(word):
    # Fixed: Use word[::-1] to reverse the string
    return word == word[::-1]

def main():
    words = ["racecar", "hello", "level", "world", "madam", "python"]
    palindromes = []
    
    for word in words:
        if is_palindrome(word):
            # Fixed: Append word, not words
            palindromes.append(word)
    
    # Fixed: Sort palindromes, not words
    sorted_palindromes = sorted(palindromes)
    
    # Should print count and sorted palindromes
    print(f"{len(palindromes)} {sorted_palindromes}")

if __name__ == "__main__":
    main()