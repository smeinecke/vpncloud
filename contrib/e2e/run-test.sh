#!/bin/bash
# End-to-end test: two vpncloud nodes peering over UDP on localhost,
# each with a real tun device. Verified by pinging across the tunnel.
set -u

cleanup() {
    kill ${N1:-} ${N2:-} 2>/dev/null
    wait ${N1:-} ${N2:-} 2>/dev/null
}
trap cleanup EXIT

fail() {
    echo "FAIL: $*"
    echo "=== node1 log ==="
    cat /node1.log 2>/dev/null
    echo "=== node2 log ==="
    cat /node2.log 2>/dev/null
    exit 1
}

vpncloud --type tun --device vpncloud0 --ip 10.0.0.1/24 \
    --listen 127.0.0.1:3210 --peer 127.0.0.1:3211 \
    --password s3cret --no-port-forwarding --verbose >/node1.log 2>&1 &
N1=$!

vpncloud --type tun --device vpncloud1 --ip 10.0.0.2/24 \
    --listen 127.0.0.1:3211 --peer 127.0.0.1:3210 \
    --password s3cret --no-port-forwarding --verbose >/node2.log 2>&1 &
N2=$!

sleep 5
kill -0 $N1 || fail "node1 exited"
kill -0 $N2 || fail "node2 exited"

ip link show vpncloud0 || fail "vpncloud0 missing"
ip link show vpncloud1 || fail "vpncloud1 missing"

ping -c3 -W2 10.0.0.2 || fail "ping across tunnel failed"

echo "PASS: ping across encrypted tunnel succeeded"
