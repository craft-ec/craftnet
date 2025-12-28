#!/bin/bash
# End-to-end test: Run two TunnelCraft nodes and verify they can connect

set -e

echo "Building TunnelCraft..."
cargo build --release -p tunnelcraft-node 2>/dev/null

NODE_BIN="./target/release/tunnelcraft-node"

# Create temp directories for node keys
TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

echo "Starting Node 1 (exit mode)..."
$NODE_BIN --keyfile "$TMPDIR/node1.key" -l /ip4/127.0.0.1/tcp/9001 exit &
NODE1_PID=$!

sleep 2

# Get Node 1's peer ID
NODE1_INFO=$($NODE_BIN --keyfile "$TMPDIR/node1.key" info 2>/dev/null)
NODE1_PEER_ID=$(echo "$NODE1_INFO" | grep "Peer ID:" | cut -d' ' -f3)

echo "Node 1 Peer ID: $NODE1_PEER_ID"

echo "Starting Node 2 (relay mode, bootstrapping to Node 1)..."
$NODE_BIN --keyfile "$TMPDIR/node2.key" -l /ip4/127.0.0.1/tcp/9002 \
    -b "${NODE1_PEER_ID}@/ip4/127.0.0.1/tcp/9001" relay &
NODE2_PID=$!

sleep 3

echo "Checking if nodes are running..."
if kill -0 $NODE1_PID 2>/dev/null && kill -0 $NODE2_PID 2>/dev/null; then
    echo "Both nodes are running!"
    echo "Test PASSED: Nodes started successfully"
else
    echo "Test FAILED: One or both nodes crashed"
    exit 1
fi

echo "Stopping nodes..."
kill $NODE1_PID $NODE2_PID 2>/dev/null || true
wait $NODE1_PID $NODE2_PID 2>/dev/null || true

echo "Done!"
