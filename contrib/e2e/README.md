# End-to-end test suite

Runs real `vpncloud` daemons in separate Linux network namespaces inside a
privileged Docker container. The nodes share an L2 underlay (a bridge with
veth pairs, `192.168.99.0/24`) and each node gets a real TUN or TAP device.

## Running

```sh
docker build -f contrib/e2e/Dockerfile -t vpncloud-e2e .
docker run --rm --privileged vpncloud-e2e
```

`--privileged` is required for `ip netns`, bridge setup, TUN/TAP creation and
`tc netem`. The container is Debian-based and carries `iproute2`, `ping`,
`nc` and `iperf3`; the script is `/run-test.sh` inside the image.

## Coverage

| Test | What it verifies |
| --- | --- |
| TUN + AES256 | Encrypted ping between namespaces, 8 KB packets, delivery fails when the peer is stopped, handshake log check |
| Restart resilience | Tunnel re-establishes after killing and restarting a peer |
| TAP mode | Ethernet frame delivery |
| Plain mode | Unencrypted transport (still password-authenticated) |
| ChaCha20 | Alternate cipher suite |
| Peer exchange | Three nodes gossip peer lists; edge nodes learn each other through a hub |
| Beacon discovery | A node discovers a peer via a shared beacon file |
| Impaired underlay | `tc netem` 5% packet loss + 30 ms delay |
| Throughput gate | `iperf3` over the tunnel must reach `THROUGHPUT_FLOOR` |

## Environment

| Variable | Default | Meaning |
| --- | --- | --- |
| `THROUGHPUT_FLOOR` | `30` | Minimum acceptable iperf3 throughput in Mbit/s |

The floor is intentionally conservative: shared CI runners vary a lot, so it
is meant to catch gross regressions rather than small drifts. The measured
value is always printed to the log.
