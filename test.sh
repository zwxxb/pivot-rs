#!/bin/bash

# Set up the environment
echo "Setting up environment..."
cargo build

# Run existing tests
echo "Running existing tests..."
cargo test

# Test the Reuse mode with timeout and optional fallback address
echo "Testing Reuse mode with timeout and optional fallback address..."

# Start the Reuse mode with a specific timeout and without fallback address
echo "Starting Reuse mode without fallback address..."
timeout 10 cargo run -- Reuse --local 127.0.0.1:8000 --remote 127.0.0.1:9000 --external 127.0.0.1

# Start the Reuse mode with a specific timeout and with fallback address
echo "Starting Reuse mode with fallback address..."
timeout 10 cargo run -- Reuse --local 127.0.0.1:8000 --remote 127.0.0.1:9000 --fallback 127.0.0.1:10000 --external 127.0.0.1

echo "Tests completed."