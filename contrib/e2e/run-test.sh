#!/bin/bash
# End-to-end test: two vpncloud nodes in separate network namespaces,
# peering over UDP across a veth pair, each with a real tun device.
# Verified by pinging across the tunnel and by checking that the ping
# fails once the remote peer is stopped (i.e. delivery really depends
# on the VPN, not on local routing).
set -u

N1=
N2=

cleanup() {
    kill ${N1:-0} ${N2:-0} 2>/dev/null
    wait ${N1:-0} ${N2:-0} 2>/dev/null
    ip netns del ns1 2>/dev/null
    ip netns del ns2 2>/dev/null
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

# Underlay: veth pair between two namespaces
ip netns add ns1 || fail "cannot create ns1 (need privileges)"
ip netns add ns2 || fail "cannot create ns2"
ip link add veth0 type veth peer name veth1 || fail "cannot create veth pair"
ip link set veth0 netns ns1
ip link set veth1 netns ns2
ip -n ns1 addr add 192.168.99.1/24 dev veth0
ip -n ns1 link set veth0 up
ip -n ns1 link set lo up
ip -n ns2 addr add 192.168.99.2/24 dev veth1
ip -n ns2 link set veth1 up
ip -n ns2 link set lo up

# Sanity check of the underlay
ip netns exec ns1 ping -c1 -W2 192.168.99.2 >/dev/null || fail "underlay veth ping failed"

ip netns exec ns1 vpncloud --type tun --device vpn0 --ip 10.0.0.1/24 \
    --listen 192.168.99.1:3210 --peer 192.168.99.2:3210 \
    --password s3cret --no-port-forwarding --verbose >/node1.log 2>&1 &
N1=$!

ip netns exec ns2 vpncloud --type tun --device vpn0 --ip 10.0.0.2/24 \
    --listen 192.168.99.2:3210 --peer 192.168.99.1:3210 \
    --password s3cret --no-port-forwarding --verbose >/node2.log 2>&1 &
N2=$!

sleep 5
kill -0 $N1 || fail "node1 exited"
kill -0 $N2 || fail "node2 exited"

ip netns exec ns1 ip link show vpn0 >/dev/null || fail "vpn0 in ns1 missing"
ip netns exec ns2 ip link show vpn0 >/dev/null || fail "vpn0 in ns2 missing"

# 10.0.0.2 is not local to ns1: the ping can only be delivered if daemon1
# forwards it through the encrypted tunnel to daemon2's tun device.
ip netns exec ns1 ping -c3 -W2 10.0.0.2 || fail "ping across tunnel failed"

# Negative control: stop the remote peer; the ping must now fail. If it
# still succeeded, delivery would not depend on the VPN at all.
kill $N2
wait $N2 2>/dev/null
sleep 1
if ip netns exec ns1 ping -c1 -W2 10.0.0.2 >/dev/null 2>&1; then
    fail "ping succeeded after peer shutdown - delivery does not require the tunnel"
fi

# Auth boundary: a peer presenting the wrong password must never receive
# tunneled traffic.
ip netns exec ns2 vpncloud --type tun --device vpn0 --ip 10.0.0.2/24 \
    --listen 192.168.99.2:3210 --peer 192.168.99.1:3210 \
    --password WRONG --no-port-forwarding --verbose >/node2.log 2>&1 &
N2=$!
sleep 4
kill -0 $N2 || fail "node2 (wrong password) exited"
if ip netns exec ns1 ping -c1 -W2 10.0.0.2 >/dev/null 2>&1; then
    fail "ping succeeded with mismatched password - unauthenticated peer received traffic"
fi

echo "PASS: encrypted tunnel verified (delivery requires tunnel and matching credentials)"
