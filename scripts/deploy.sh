#!/usr/bin/env bash
set -euo pipefail

# CraftNet Bootstrap/Exit Node Deployment
# Usage: ./scripts/deploy.sh [host]

HOST="${1:-64.225.12.79}"
SSH_KEY="$HOME/.ssh/craftnet-deploy"
SSH_USER="root"
SSH="ssh -i $SSH_KEY $SSH_USER@$HOST"
SCP="scp -i $SSH_KEY"
TARGET="x86_64-unknown-linux-gnu"
BINARY="target/$TARGET/release/craftnet"
REMOTE_BIN="/usr/local/bin/craftnet"

echo "==> Building release for $TARGET..."
cargo zigbuild --release --target "$TARGET" -p craftnet-cli

echo "==> Binary size: $(du -h "$BINARY" | cut -f1)"

echo "==> Uploading to $HOST..."
$SCP "$BINARY" "$SSH_USER@$HOST:${REMOTE_BIN}-new"

echo "==> Swapping binary and restarting services..."
$SSH "chmod +x ${REMOTE_BIN}-new \
  && mv $REMOTE_BIN ${REMOTE_BIN}-old \
  && mv ${REMOTE_BIN}-new $REMOTE_BIN \
  && systemctl restart craftnet-bootstrap \
  && sleep 2 \
  && systemctl restart craftnet-exit"

echo "==> Checking status..."
$SSH "systemctl status craftnet-bootstrap craftnet-exit --no-pager -l" 2>&1 | tail -20

echo "==> Deploy complete."
