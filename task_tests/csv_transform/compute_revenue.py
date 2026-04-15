#!/usr/bin/env python3
import csv
import os
from decimal import Decimal

def compute_west_revenue():
    # Get the path to the CSV file
    csv_path = os.path.join(os.path.dirname(__file__), 'sales_data.csv')
    
    total_revenue = Decimal('0')
    
    try:
        with open(csv_path, 'r') as csvfile:
            reader = csv.DictReader(csvfile)
            
            for row in reader:
                # Check if the region is 'West'
                if row['region'] == 'West':
                    # Convert revenue to Decimal for precise arithmetic
                    revenue_value = Decimal(row['revenue'])
                    total_revenue += revenue_value
        
        # Print the total revenue as an integer (no decimals, no commas)
        # Using int() on Decimal truncates towards zero, which is what we want
        print(int(total_revenue))
        
    except FileNotFoundError:
        print(f"Error: File '{csv_path}' not found.")
        return 1
    except Exception as e:
        print(f"Error: {e}")
        return 1
    
    return 0

if __name__ == "__main__":
    exit(compute_west_revenue())