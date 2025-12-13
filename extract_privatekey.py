#!/usr/bin/env python3
"""
Extract private key hex from starcoin_client.key file.

Usage:
    ./extract_privatekey.py <key_file_path>

The key file should be base64 encoded with format: flag || privkey
For Ed25519: flag is 0x00 (1 byte), followed by 32 bytes of private key
"""

import base64
import sys
import os

def extract_private_key(key_file_path):
    """Extract private key hex from base64 encoded key file."""
    try:
        # Read and decode the key file
        with open(key_file_path, 'r') as f:
            key_data = f.read().strip()
        
        # Decode base64
        data = base64.b64decode(key_data)
        
        # Validate length
        if len(data) < 33:
            print(f"ERROR: Decoded data too short: {len(data)} bytes (expected at least 33)", file=sys.stderr)
            sys.exit(1)
        
        # Extract private key (skip flag byte)
        private_key_bytes = data[1:33]
        
        # Convert to hex and print to stdout (only output)
        private_key_hex = private_key_bytes.hex()
        print(private_key_hex)
        
    except FileNotFoundError:
        print(f"ERROR: Key file not found: {key_file_path}", file=sys.stderr)
        sys.exit(1)
    except Exception as e:
        print(f"ERROR: {e}", file=sys.stderr)
        sys.exit(1)

if __name__ == '__main__':
    if len(sys.argv) != 2:
        print(f"Usage: {sys.argv[0]} <key_file_path>", file=sys.stderr)
        sys.exit(1)
    
    key_file_path = sys.argv[1]
    extract_private_key(key_file_path)
