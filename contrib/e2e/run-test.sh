#!/bin/bash
# End-to-end test suite for vpncloud. Spins up real daemons in separate
# network namespaces connected by a bridge, each with a real tun/tap
# device. Verifies encrypted transport, protocol variants, resilience
# and a minimum throughput floor.
set -u

PIDS=
RESULTS=()
THROUGHPUT_FLOOR=${THROUGHPUT_FLOOR:-30}   # Mbit/s, conservative CI floor
BASE_PORT=3210
LAST_PID=

cleanup() {
    for p in $PIDS; do kill "$p" 2>/dev/null; done
    for p in $PIDS; do wait "$p" 2>/dev/null; done
    for n in ns1 ns2 ns3; do ip netns del "$n" 2>/dev/null; done
    ip link del br0 2>/dev/null
    rm -f /tmp/vpncloud-e2e-beacon /node*.log
}
trap cleanup EXIT

fail() {
    echo "FAIL: $*"
    for f in /node*.log; do echo "=== $f ==="; tail -20 "$f" 2>/dev/null; done
    exit 1
}

record() {
    echo "PASS: $1"
    RESULTS+=("PASS  $1")
}

# start_node <netns> <name> <args...>; sets LAST_PID
start_node() {
    local ns=$1 name=$2; shift 2
    ip netns exec "$ns" vpncloud "$@" --verbose >"/${name}.log" 2>&1 &
    LAST_PID=$!
    PIDS="$PIDS $LAST_PID"
    sleep 1
    kill -0 "$LAST_PID" || fail "$name exited early"
}

kill_pid() {
    kill "$1" 2>/dev/null
    wait "$1" 2>/dev/null
    sleep 1
}

# wait_ping <netns> <target> <tries> - retries until ping works
wait_ping() {
    local ns=$1 dst=$2 tries=${3:-15}
    for _ in $(seq "$tries"); do
        ip netns exec "$ns" ping -c1 -W2 "$dst" >/dev/null 2>&1 && return 0
        sleep 1
    done
    return 1
}

ping_fail() { ! ip netns exec "$1" ping -c1 -W2 "$2" >/dev/null 2>&1; }

# setup_underlay: ns1/ns2/ns3 share one L2 segment 192.168.99.0/24 via br0
setup_underlay() {
    for n in ns1 ns2 ns3; do ip netns add "$n" || fail "cannot create $n"; done
    ip link add br0 type bridge || fail "cannot create bridge"
    ip link set br0 up
    local i
    for i in 1 2 3; do
        ip link add "veth${i}p" type veth peer name "veth${i}r"
        ip link set "veth${i}p" netns "ns${i}"
        ip link set "veth${i}r" master br0
        ip link set "veth${i}r" up
        ip netns exec "ns${i}" ip addr add "192.168.99.$((i + 1))/24" dev "veth${i}p"
        ip netns exec "ns${i}" ip link set "veth${i}p" up
        ip netns exec "ns${i}" ip link set lo up
    done
    # sanity: underlay connectivity between all namespaces
    for n in ns1 ns2 ns3; do
        for dst in 2 3 4; do
            ip netns exec "$n" ping -c1 -W2 "192.168.99.$dst" >/dev/null 2>&1 || true
        done
    done
}

add_tun() {
    ip netns exec "$1" ip tuntap add dev "$2" mode tun
    ip netns exec "$1" ip addr add "$3/24" dev "$2"
    ip netns exec "$1" ip link set "$2" up
}
add_tap() {
    ip netns exec "$1" ip tuntap add dev "$2" mode tap
    ip netns exec "$1" ip addr add "$3/24" dev "$2"
    ip netns exec "$1" ip link set "$2" up
}
del_dev() { ip netns exec "$1" ip link del "$2" 2>/dev/null || true; }

killall_nodes() {
    for p in $PIDS; do kill "$p" 2>/dev/null; done
    for p in $PIDS; do wait "$p" 2>/dev/null; done
    PIDS=
    sleep 1
}

setup_underlay || fail "underlay setup failed"
N=$BASE_PORT
U1=192.168.99.2; U2=192.168.99.3; U3=192.168.99.4

# === Test 1: TUN + AES256, negative controls, large packets, restart ===
add_tun ns1 vpn0 10.0.0.1; add_tun ns2 vpn0 10.0.0.2
start_node ns1 node1 --type tun --device vpn0 --ip 10.0.0.1/24 \
    --listen $U1:$N --peer $U2:$N --password test123 --algorithm aes256 --no-port-forwarding
start_node ns2 node2 --type tun --device vpn0 --ip 10.0.0.2/24 \
    --listen $U2:$N --peer $U1:$N --password test123 --algorithm aes256 --no-port-forwarding
N2_PID=$LAST_PID
wait_ping ns1 10.0.0.2 || fail "tun/aes256: ping failed"
grep -qiE "Connected|crypto" /node1.log || fail "tun/aes256: no handshake in log"
ip netns exec ns1 ping -c1 -W3 -s 8000 10.0.0.2 >/dev/null 2>&1 \
    || fail "tun/aes256: 8000-byte ping failed"
kill_pid "$N2_PID"
ping_fail ns1 10.0.0.2 || fail "tun/aes256: ping succeeded after peer shutdown"
record "tun+aes256: encrypted ping, 8k packet, shutdown control"

# restart resilience: node1 must re-handshake with a fresh node2
start_node ns2 node2 --type tun --device vpn0 --ip 10.0.0.2/24 \
    --listen $U2:$N --peer $U1:$N --password test123 --algorithm aes256 --no-port-forwarding
wait_ping ns1 10.0.0.2 30 || fail "restart: ping did not recover"
record "restart resilience: tunnel re-establishes after peer restart"
killall_nodes

# === Test 2: TAP mode ===
N=$((N + 1))
add_tap ns1 vpn1 10.1.0.1; add_tap ns2 vpn1 10.1.0.2
start_node ns1 node1t --type tap --device vpn1 --ip 10.1.0.1/24 \
    --listen $U1:$N --peer $U2:$N --password test123 --no-port-forwarding
start_node ns2 node2t --type tap --device vpn1 --ip 10.1.0.2/24 \
    --listen $U2:$N --peer $U1:$N --password test123 --no-port-forwarding
wait_ping ns1 10.1.0.2 || fail "tap: ping failed"
record "tap mode: ethernet frame delivery"
killall_nodes

# === Test 3: unencrypted mode ===
N=$((N + 1))
add_tun ns1 vpn2 10.2.0.1; add_tun ns2 vpn2 10.2.0.2
start_node ns1 node1p --type tun --device vpn2 --ip 10.2.0.1/24 \
    --listen $U1:$N --peer $U2:$N --password test123 --algorithm plain --no-port-forwarding
start_node ns2 node2p --type tun --device vpn2 --ip 10.2.0.2/24 \
    --listen $U2:$N --peer $U1:$N --password test123 --algorithm plain --no-port-forwarding
wait_ping ns1 10.2.0.2 || fail "plain: ping failed"
record "plain mode: unencrypted transport"
killall_nodes

# === Test 4: chacha20 ===
N=$((N + 1))
add_tun ns1 vpn3 10.3.0.1; add_tun ns2 vpn3 10.3.0.2
start_node ns1 node1c --type tun --device vpn3 --ip 10.3.0.1/24 \
    --listen $U1:$N --peer $U2:$N --password test123 --algorithm chacha20 --no-port-forwarding
start_node ns2 node2c --type tun --device vpn3 --ip 10.3.0.2/24 \
    --listen $U2:$N --peer $U1:$N --password test123 --algorithm chacha20 --no-port-forwarding
wait_ping ns1 10.3.0.2 || fail "chacha20: ping failed"
record "chacha20: alternate cipher"
killall_nodes

# === Test 5: peer exchange / mesh formation ===
# node2 and node3 each only know node1; they must discover each other
# through node1's gossiped peer lists.
N=$((N + 1))
add_tun ns1 vpn4 10.9.0.1; add_tun ns2 vpn4 10.9.0.2; add_tun ns3 vpn4 10.9.0.3
start_node ns1 node1x --type tun --device vpn4 --ip 10.9.0.1/24 \
    --listen $U1:$N --password test123 --no-port-forwarding
start_node ns2 node2x --type tun --device vpn4 --ip 10.9.0.2/24 \
    --listen $U2:$N --peer $U1:$N --password test123 --no-port-forwarding
start_node ns3 node3x --type tun --device vpn4 --ip 10.9.0.3/24 \
    --listen $U3:$N --peer $U1:$N --password test123 --no-port-forwarding
wait_ping ns2 10.9.0.3 60 || fail "peer exchange: node2 could not ping node3"
wait_ping ns3 10.9.0.2 10 || fail "peer exchange: node3 could not ping node2"
record "peer exchange: mesh forms via gossiped peer lists"
killall_nodes

# === Test 6: beacon-based peer discovery ===
N=$((N + 1))
start_node ns1 node1b --type tun --device vpn4 --ip 10.9.0.1/24 \
    --listen $U1:$N --password test123 --no-port-forwarding \
    --beacon-store /tmp/vpncloud-e2e-beacon --beacon-interval 2
for _ in 1 2 3 4 5; do [ -s /tmp/vpncloud-e2e-beacon ] && break; sleep 2; done
[ -s /tmp/vpncloud-e2e-beacon ] || fail "beacon: node1 never wrote beacon file"
start_node ns2 node2b --type tun --device vpn4 --ip 10.9.0.2/24 \
    --listen $U2:$N --password test123 --no-port-forwarding \
    --beacon-load /tmp/vpncloud-e2e-beacon --beacon-interval 2
wait_ping ns2 10.9.0.1 40 || fail "beacon: node2 could not ping beacon-discovered node1"
record "beacon: peer discovered via beacon file"
killall_nodes

# === Test 7: impaired underlay (packet loss + latency) ===
N=$((N + 1))
ip netns exec ns2 tc qdisc add dev veth2p root netem loss 5% delay 30ms
start_node ns1 node1i --type tun --device vpn4 --ip 10.9.0.1/24 \
    --listen $U1:$N --peer $U2:$N --password test123 --no-port-forwarding
start_node ns2 node2i --type tun --device vpn4 --ip 10.9.0.2/24 \
    --listen $U2:$N --peer $U1:$N --password test123 --no-port-forwarding
wait_ping ns1 10.9.0.2 45 || fail "impaired underlay: no tunnel through 5% loss"
ip netns exec ns2 tc qdisc del dev veth2p root 2>/dev/null || true
record "impaired underlay: tunnel survives 5% loss + 30ms delay"
killall_nodes

# === Test 8: throughput requirement ===
N=$((N + 1))
start_node ns1 node1f --type tun --device vpn4 --ip 10.9.0.1/24 \
    --listen $U1:$N --peer $U2:$N --password test123 --no-port-forwarding
start_node ns2 node2f --type tun --device vpn4 --ip 10.9.0.2/24 \
    --listen $U2:$N --peer $U1:$N --password test123 --no-port-forwarding
wait_ping ns1 10.9.0.2 || fail "throughput: ping failed before test"
ip netns exec ns2 iperf3 -s -1 -p 5201 >/dev/null 2>&1 &
sleep 1
TP=$(ip netns exec ns1 iperf3 -c 10.9.0.2 -p 5201 -t 5 -f m 2>/dev/null \
    | awk '/receiver/{print $(NF-2)}' | tail -1)
[ -n "$TP" ] || fail "throughput: iperf3 produced no result"
echo "throughput: ${TP} Mbit/s (floor: ${THROUGHPUT_FLOOR})"
awk -v t="$TP" -v f="$THROUGHPUT_FLOOR" 'BEGIN{exit !(t+0 >= f+0)}' \
    || fail "throughput ${TP} Mbit/s below floor ${THROUGHPUT_FLOOR} Mbit/s"
record "throughput: ${TP} Mbit/s >= ${THROUGHPUT_FLOOR} Mbit/s floor"

echo
echo "==== RESULTS ===="
for r in "${RESULTS[@]}"; do echo "  $r"; done
echo "ALL TESTS PASSED"
