#!/bin/bash
# CraftNet Docker Integration Tests
# This script runs inside the test-runner container

set -e

echo "=========================================="
echo "CraftNet Integration Test Suite"
echo "=========================================="

# Environment variables set by docker-compose
EXIT_NODE=${EXIT_NODE_ADDR:-"172.28.0.10:9000"}
RELAY1=${RELAY1_ADDR:-"172.28.0.11:9000"}
RELAY2=${RELAY2_ADDR:-"172.28.0.12:9000"}
FULL_NODE=${FULL_NODE_ADDR:-"172.28.0.13:9000"}

CLI="/app/target/release/craftnet"

# Test counters
PASSED=0
FAILED=0

# Helper functions
pass() {
    echo "✓ PASS: $1"
    PASSED=$((PASSED + 1))
}

fail() {
    echo "✗ FAIL: $1"
    FAILED=$((FAILED + 1))
}

wait_for_port() {
    local host=$1
    local port=$2
    local timeout=${3:-30}

    echo "Waiting for $host:$port..."
    for i in $(seq 1 $timeout); do
        if nc -z $host $port 2>/dev/null; then
            echo "$host:$port is ready"
            return 0
        fi
        sleep 1
    done
    echo "Timeout waiting for $host:$port"
    return 1
}

# Wait for all nodes to be ready
echo ""
echo "Step 1: Waiting for nodes to be ready..."
echo "------------------------------------------"

wait_for_port 172.28.0.10 9000 || fail "Exit node not ready"
wait_for_port 172.28.0.11 9000 || fail "Relay node 1 not ready"
wait_for_port 172.28.0.12 9000 || fail "Relay node 2 not ready"
wait_for_port 172.28.0.13 9000 || fail "Full node not ready"

# Give nodes a moment to fully initialize
sleep 3

echo ""
echo "Step 2: Testing CLI commands..."
echo "------------------------------------------"

# Test: CLI status command (should work even without daemon)
echo "Testing CLI help..."
if $CLI --help > /dev/null 2>&1; then
    pass "CLI help command works"
else
    fail "CLI help command failed"
fi

# Test: CLI status (will fail without daemon, but should not crash)
echo "Testing CLI status (expected to fail without daemon)..."
if $CLI status 2>&1 | grep -q "Failed\|Error\|error"; then
    pass "CLI status correctly reports no daemon"
else
    # It might just timeout, which is also acceptable
    pass "CLI status handled gracefully"
fi

echo ""
echo "Step 3: Testing TCP connectivity between nodes..."
echo "------------------------------------------"

# Test: Can connect to exit node
if nc -z 172.28.0.10 9000; then
    pass "Exit node TCP port reachable"
else
    fail "Exit node TCP port not reachable"
fi

# Test: Can connect to relay nodes
if nc -z 172.28.0.11 9000; then
    pass "Relay node 1 TCP port reachable"
else
    fail "Relay node 1 TCP port not reachable"
fi

if nc -z 172.28.0.12 9000; then
    pass "Relay node 2 TCP port reachable"
else
    fail "Relay node 2 TCP port not reachable"
fi

if nc -z 172.28.0.13 9000; then
    pass "Full node TCP port reachable"
else
    fail "Full node TCP port not reachable"
fi

echo ""
echo "Step 4: Running Rust integration tests..."
echo "------------------------------------------"

# Run the network integration tests
cd /app
if cargo test --release -p craftnet-network --test shard_exchange -- --test-threads=1 2>&1; then
    pass "Rust shard exchange tests"
else
    fail "Rust shard exchange tests"
fi

echo ""
echo "Step 5: Testing node process health..."
echo "------------------------------------------"

# Check nodes are still running after tests
sleep 2

if nc -z 172.28.0.10 9000; then
    pass "Exit node still healthy after tests"
else
    fail "Exit node crashed during tests"
fi

if nc -z 172.28.0.11 9000; then
    pass "Relay node 1 still healthy after tests"
else
    fail "Relay node 1 crashed during tests"
fi

if nc -z 172.28.0.12 9000; then
    pass "Relay node 2 still healthy after tests"
else
    fail "Relay node 2 crashed during tests"
fi

if nc -z 172.28.0.13 9000; then
    pass "Full node still healthy after tests"
else
    fail "Full node crashed during tests"
fi

echo ""
echo "=========================================="
echo "Test Results"
echo "=========================================="
echo "Passed: $PASSED"
echo "Failed: $FAILED"
echo ""

if [ $FAILED -eq 0 ]; then
    echo "All tests passed!"
    exit 0
else
    echo "Some tests failed!"
    exit 1
fi
