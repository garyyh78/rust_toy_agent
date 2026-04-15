#!/usr/bin/env python3
import json

def main():
    # Read the JSON file
    with open('data.json', 'r') as file:
        data = json.load(file)
    
    # Filter objects where score > 80 and collect names
    names = []
    for item in data:
        if isinstance(item, dict) and 'score' in item and 'name' in item:
            if item['score'] > 80:
                names.append(item['name'])
    
    # Sort names alphabetically
    names.sort()
    
    # Print comma-separated on one line
    print(','.join(names))

if __name__ == "__main__":
    main()