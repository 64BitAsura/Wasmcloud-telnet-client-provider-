#!/bin/bash
set -e

# Ensure cargo and wash are in PATH
. "$HOME/.cargo/env" 2>/dev/null || true
export PATH="/usr/local/bin:$PATH"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}=== Telnet Provider Integration Test ===${NC}"
echo ""

# Check prerequisites
echo -e "${YELLOW}Checking prerequisites...${NC}"

if ! command -v wash &> /dev/null; then
    echo -e "${RED}Error: wash CLI not found${NC}"
    exit 1
fi

if ! command -v python3 &> /dev/null; then
    echo -e "${RED}Error: python3 not found${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Prerequisites OK${NC}"
echo ""

# Cleanup function
cleanup() {
    echo ""
    echo -e "${YELLOW}Cleaning up...${NC}"
    
    # Stop Telnet server
    if [ ! -z "$TELNET_SERVER_PID" ]; then
        kill $TELNET_SERVER_PID 2>/dev/null || true
        echo "✓ Stopped Telnet server"
    fi
    
    # Stop wasmCloud
    wash down 2>/dev/null || true
    if [ ! -z "$WASH_PID" ]; then
        kill $WASH_PID 2>/dev/null || true
    fi
    echo "✓ Stopped wasmCloud"
    
    echo -e "${GREEN}Cleanup complete${NC}"
}

# Set up trap to cleanup on exit
trap cleanup EXIT

# Start Telnet test server
echo -e "${YELLOW}Starting Telnet test server...${NC}"
python3 tests/telnet_server.py > /tmp/telnet_server.log 2>&1 &
TELNET_SERVER_PID=$!
sleep 2

if ! kill -0 $TELNET_SERVER_PID 2>/dev/null; then
    echo -e "${RED}Error: Failed to start Telnet server${NC}"
    cat /tmp/telnet_server.log
    exit 1
fi

echo -e "${GREEN}✓ Telnet server started (PID: $TELNET_SERVER_PID)${NC}"
echo "  Listening on 127.0.0.1:2323"
echo ""

# Build the provider
echo -e "${YELLOW}Building provider...${NC}"
wash build 2>&1 | grep -E "(Compiling|Finished|error|Built)" || true

# Find the built provider archive
PROVIDER_PATH=$(find build -name "*.par.gz" 2>/dev/null | head -1)
if [ -z "$PROVIDER_PATH" ]; then
    echo -e "${RED}Error: Provider build failed - no .par.gz file found${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Provider built: $PROVIDER_PATH${NC}"
echo ""

# Build the component
echo -e "${YELLOW}Building component...${NC}"
wash build -p ./component 2>&1 | grep -E "(Compiling|Finished|error|Built)" || true

# Find the built component
COMPONENT_PATH=$(find component/target -name "*_s.wasm" 2>/dev/null | head -1)
if [ -z "$COMPONENT_PATH" ]; then
    echo -e "${RED}Error: Component build failed${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Component built: $COMPONENT_PATH${NC}"
echo ""

# Start wasmCloud
echo -e "${YELLOW}Starting wasmCloud host...${NC}"

# Run wash up in the background, capturing logs to a file
WASMCLOUD_LOG="/tmp/wasmcloud_host.log"
wash up > "$WASMCLOUD_LOG" 2>&1 &
WASH_PID=$!

# Wait for host to be ready
echo "Waiting for host to be ready..."
for i in {1..30}; do
    if wash get hosts 2>/dev/null | grep -qE "^  [A-Z0-9]{56}"; then
        break
    fi
    sleep 1
done

echo -e "${GREEN}✓ wasmCloud host started${NC}"
echo ""

# Deploy provider
echo -e "${YELLOW}Deploying provider...${NC}"
# wash start may return timeout error even when provider starts successfully, so don't fail on it
wash start provider "file://./$PROVIDER_PATH" telnet-provider --timeout-ms 30000 2>&1 || true
sleep 5

# Verify provider is actually running
if wash get inventory 2>&1 | grep -q "telnet-provider"; then
    echo -e "${GREEN}✓ Provider deployed and running${NC}"
else
    echo -e "${RED}Error: Provider failed to start${NC}"
    wash get inventory 2>&1
    exit 1
fi
echo ""

# Deploy component
echo -e "${YELLOW}Deploying component...${NC}"
wash start component "file://./$COMPONENT_PATH" test-component --timeout-ms 30000 2>&1 || true
sleep 3

# Verify component is running
if wash get inventory 2>&1 | grep -q "test-component"; then
    echo -e "${GREEN}✓ Component deployed and running${NC}"
else
    echo -e "${RED}Error: Component failed to start${NC}"
    wash get inventory 2>&1
    exit 1
fi
echo ""

# Create link
echo -e "${YELLOW}Creating link between component and provider...${NC}"

# First, create the named config with the Telnet connection settings
wash config put telnet-config \
  telnet_host=127.0.0.1 \
  telnet_port=2323 \
  max_reconnect_attempts=0 \
  initial_reconnect_delay_ms=1000

# Then create the link referencing that config
wash link put test-component telnet-provider \
  wasmcloud messaging \
  --interface handler \
  --target-config telnet-config

sleep 2
echo -e "${GREEN}✓ Link created${NC}"
echo ""

# Monitor logs
echo -e "${GREEN}=== Test Running ===${NC}"
echo "Monitoring host logs for 30 seconds..."
echo "Press Ctrl+C to stop early"
echo ""

# Wait and show progress, checking logs from the host output
for i in {1..30}; do
    echo -ne "\rTime: ${i}s / 30s  "
    sleep 1
done

echo ""
echo ""

# Analyze logs from the wasmCloud host output
echo -e "${GREEN}=== Test Results ===${NC}"
echo ""

PROVIDER_CONNECTED=$(grep -c "Telnet connection established" "$WASMCLOUD_LOG" 2>/dev/null || echo "0")
MESSAGES_RECEIVED=$(grep -c "Received message" "$WASMCLOUD_LOG" 2>/dev/null || echo "0")
MESSAGES_SENT=$(grep -c "Message successfully sent to component" "$WASMCLOUD_LOG" 2>/dev/null || echo "0")

echo "Provider connections: $PROVIDER_CONNECTED"
echo "Messages received by provider: $MESSAGES_SENT"
echo "Messages handled by component: $MESSAGES_RECEIVED"
echo ""

if [ "$PROVIDER_CONNECTED" -gt "0" ] && [ "$MESSAGES_RECEIVED" -gt "0" ]; then
    echo -e "${GREEN}✓ Integration test PASSED${NC}"
    echo ""
    echo "The provider successfully:"
    echo "  - Connected to the Telnet server"
    echo "  - Received messages from the server"
    echo "  - Forwarded messages to the component"
    echo "  - Component processed the messages"
    exit 0
else
    echo -e "${RED}✗ Integration test FAILED${NC}"
    echo ""
    echo "Last 50 lines of host logs:"
    tail -50 "$WASMCLOUD_LOG" 2>/dev/null || echo "(no logs available)"
    exit 1
fi
