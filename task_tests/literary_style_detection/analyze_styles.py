#!/usr/bin/env python3
import re
import os

def analyze_style(text):
    """Analyze literary style features and guess the author"""
    
    # Convert to lowercase for analysis
    text_lower = text.lower()
    
    # Feature 1: Check for Homeric epithets (compound adjectives)
    epithet_patterns = [
        r'\bswift-footed\b', r'\bgrey-eyed\b', r'\bwine-dark\b', r'\blion-hearted\b',
        r'\bbronze-clad\b', r'\brosey-fingered\b', r'\bbreaker of horses\b'
    ]
    epithet_count = sum(len(re.findall(pattern, text_lower)) for pattern in epithet_patterns)
    
    # Feature 2: Check for Shakespearean thee/thou/thy
    shakespeare_words = len(re.findall(r'\b(thee|thou|thy|dost|doth|hath|\'tis)\b', text_lower))
    
    # Feature 3: Check for Whitman's cataloging style (long lists, repetition)
    # Count lines that start with "The" followed by noun phrases (cataloging)
    lines = text.split('\n')
    catalog_lines = 0
    for line in lines:
        stripped = line.strip()
        if stripped.startswith('The ') and len(stripped.split()) > 3:
            catalog_lines += 1
    
    # Feature 4: Check for Milton's Latinate vocabulary
    latinate_words = [
        'disobedience', 'forbidden', 'mortal', 'restore', 'blissful', 'inspire',
        'chaos', 'adventurous', 'unattempted', 'providence', 'justify', 'transgress',
        'infernal', 'seduced', 'revolt', 'deceived', 'aspiring', 'ambitious',
        'monarchy', 'impious', 'combustion', 'perdition', 'adamantine', 'omnipotent'
    ]
    latinate_count = sum(text_lower.count(word) for word in latinate_words)
    
    # Feature 5: Check for iambic rhythm (Shakespeare/Milton)
    # Simple check for lines with 10 syllables (iambic pentameter)
    iambic_lines = 0
    for line in lines:
        words = line.split()
        if 8 <= len(words) <= 12:  # Approximate iambic pentameter
            iambic_lines += 1
    
    # Feature 6: Check for epic invocations (Homer/Milton)
    invocations = len(re.findall(r'\b(sing|o muse|heavenly muse|invoke|invocation)\b', text_lower, re.IGNORECASE))
    
    # Feature 7: Check for free verse (Whitman) - irregular line lengths
    line_lengths = [len(line.split()) for line in lines if line.strip()]
    avg_line_length = sum(line_lengths) / len(line_lengths) if line_lengths else 0
    line_length_variance = sum((length - avg_line_length) ** 2 for length in line_lengths) / len(line_lengths) if line_lengths else 0
    
    # Feature 8: Check for nature references (Whitman)
    nature_words = ['grass', 'soil', 'air', 'tree', 'leaf', 'flower', 'bird', 'sky', 'sea', 'river', 'mountain']
    nature_count = sum(text_lower.count(word) for word in nature_words)
    
    # Feature 9: Check for theological terms (Milton)
    theological_words = ['god', 'heaven', 'hell', 'angel', 'devil', 'satan', 'eden', 'sin', 'paradise', 'divine', 'eternal']
    theological_count = sum(text_lower.count(word) for word in theological_words)
    
    # Score each author based on features
    scores = {
        "Homer": 0,
        "Shakespeare": 0,
        "Whitman": 0,
        "Milton": 0
    }
    
    # Homer scoring
    scores["Homer"] += epithet_count * 5
    scores["Homer"] += invocations * 4
    scores["Homer"] += (theological_count > 0) * -2  # Homer has gods but not theological in Milton's sense
    
    # Shakespeare scoring
    scores["Shakespeare"] += shakespeare_words * 5
    scores["Shakespeare"] += iambic_lines * 2
    scores["Shakespeare"] += (latinate_count > 3) * -2  # Less Latinate than Milton
    
    # Whitman scoring
    scores["Whitman"] += catalog_lines * 3
    scores["Whitman"] += nature_count * 2
    scores["Whitman"] += (line_length_variance > 10) * 4  # High variance = free verse
    scores["Whitman"] += (shakespeare_words > 0) * -3  # No thee/thou
    scores["Whitman"] += (iambic_lines > 5) * -2  # Not iambic
    
    # Milton scoring
    scores["Milton"] += latinate_count * 3
    scores["Milton"] += theological_count * 3
    scores["Milton"] += iambic_lines * 2
    scores["Milton"] += invocations * 2
    scores["Milton"] += (epithet_count > 2) * -2  # Not Homeric epithets
    scores["Milton"] += (catalog_lines > 5) * -2  # Not Whitman cataloging
    
    # Return the author with highest score
    return max(scores.items(), key=lambda x: x[1])[0]

def read_answers():
    """Read the correct answers from answers.txt"""
    answers = {}
    try:
        with open("answers.txt", 'r') as f:
            for line in f:
                line = line.strip()
                if '->' in line:
                    parts = line.split('->')
                    if len(parts) == 2:
                        filename = parts[0].strip()
                        author = parts[1].strip()
                        answers[filename] = author
    except FileNotFoundError:
        print("Error: answers.txt not found")
    return answers

def main():
    # Get list of text files
    text_files = [f for f in os.listdir('.') if f.startswith('file') and f.endswith('.txt')]
    text_files.sort()  # Sort to get file1.txt, file2.txt, etc.
    
    # Read correct answers
    correct_answers = read_answers()
    
    # Analyze each file
    results = []
    for filename in text_files:
        try:
            with open(filename, 'r') as f:
                text = f.read()
            
            guessed_author = analyze_style(text)
            correct_author = correct_answers.get(filename, "Unknown")
            
            is_correct = "correct" if guessed_author == correct_author else "wrong"
            result_line = f"{filename} -> {guessed_author} -> {is_correct}"
            results.append(result_line)
            
        except FileNotFoundError:
            print(f"Error: {filename} not found")
    
    # Print all results
    for result in results:
        print(result)

if __name__ == "__main__":
    main()