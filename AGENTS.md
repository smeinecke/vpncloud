# Agent notes

Build, test and verification commands for this repository.

## Build and test

```sh
cargo build --release
cargo test --release            # 105 tests, 1 ignored
cargo test --all-features
```

The ignored test `tests::peers::update_primary_address` needs simulator
support for rebinding a node's socket address and stays ignored until that
exists.

## End-to-end suite

```sh
docker build -f contrib/e2e/Dockerfile -t vpncloud-e2e .
docker run --rm --privileged vpncloud-e2e
```

Requires a privileged container (netns, bridge, TUN/TAP, `tc netem`).
`THROUGHPUT_FLOOR` (Mbit/s, default 30) sets the iperf3 gate. See
`contrib/e2e/README.md` for the test list.

## Performance gate

```sh
cargo bench --bench perf_gate                   # compare against baseline
PERF_UPDATE=1 cargo bench --bench perf_gate     # rewrite baseline
PERF_BASELINE=/tmp/base.txt cargo bench --bench perf_gate
PERF_TOLERANCE_PCT=30 cargo bench --bench perf_gate
```

`benches/perf_baseline.txt` is machine-specific; fair comparisons are
same-run. `cargo bench --bench criterion` runs the criterion benches.
Both bench crates include sources via `benches/.code.rs`, so they see
the `#[cfg(test)]` simulator internals.

## Fuzzing

```sh
cargo +nightly fuzz list
cargo +nightly fuzz run payload -- -max_total_time=600
```

Runs must happen inside `fuzz/` semantics; `cargo fuzz run` does not
accept `--manifest-path` (the CI workflow cds into the directory).

## Audit

```sh
cargo audit
```

Expected: 0 vulnerabilities, warnings for `daemonize` (unmaintained) and
`unsafe-libyaml` (serde_yaml_ng backend).

## Conventions and pitfalls

- `Simulator::simulate_time(t)` takes an absolute end time, not a duration.
- Peer removal happens in `Cloud::housekeep`, not in the crypto layer.
- `MsgBuffer::new` zeroes a 64 KB array; it is already allocated once per
  call site, not per peer/packet.
- Plain (`--mode normal` unencrypted) transport still requires
  `--password`/`--private-key` for peer authentication.
- Commit messages: imperative, no emoji; use `-` not em dashes.
- Known harmless warnings: `cfg(target_os = "bitrig")`,
  `cfg(feature = "bench")`, unused `clap::Subcommand` import, deprecated
  `f32::INFINITY`/`u32::max_value`.
