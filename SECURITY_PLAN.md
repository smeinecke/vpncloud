# VpnCloud security analysis and testing plan

## Purpose and scope

Assess whether an attacker can join a VPN without authorization, alter or expose tunneled traffic, disrupt peers, abuse the WebSocket relay, or cross the boundary from network input into privileged host operations. Produce reproducible findings and regression tests, followed by fixes prioritized by impact and reachability.

This is a plan, not a completed security assessment. The inventory below comes from source inspection of the working checkout on 2026-11-06; the commands and campaigns have not been run as part of writing this document. Review questions are not confirmed vulnerabilities.

Cover the Rust application, its wire protocol, dependencies, configuration, service units, packaging, and CI. Include default features (`nat`, `websocket`, `wizard`), the minimal build, and the optional `installer` feature. Start on Linux, where the current TUN/TAP integration harness runs; add other supported release targets after establishing the baseline.

## 1. Establish a reproducible baseline

Record the commit, working-tree diff, untracked source files, Rust/Cargo versions, feature flags, target architecture, and both dependency lockfiles. This checkout contains existing local changes, including an untracked `fuzz/` workspace and `src/lib.rs`; preserve them and identify which artifacts belong in the assessed revision. Do not describe local fuzzing infrastructure as already integrated into upstream CI.

Existing foundations:

| Area | Current starting point | Assessment gap |
| --- | --- | --- |
| Unit and simulation tests | Inline tests and `src/tests/{peers,payload,nat}.rs`; mock devices, sockets, and time in `src/tests/common.rs` | Add hostile packet sequences and assertions about rejected state changes. Several peer lifecycle and switch tests are ignored. |
| Cryptography tests | Tests in `src/crypto/{core,init,rotate}.rs` cover tampering, negotiation, nonce pinning, rotation, loss, and duplication | Map each security requirement to a test; examine boundary cases and cross-session behavior. |
| Fuzzing | Six local targets: `init_msg`, `node_info`, `beacon`, `payload`, `decrypt`, `range` | Validate harness correctness and coverage before long campaigns; add authenticated and stateful inputs. |
| End-to-end tests | `contrib/e2e/` runs two TUN nodes and a ping inside one container | Put peers in separate network namespaces and prove that delivery depends on the tunnel. |
| CI | Check/test and integration workflows on pushes; scheduled dependency audit | Add pull-request security gates, feature coverage, reproducible tool versions, and retained test artifacts. |

Baseline commands, run separately from the repository root:

```sh
git rev-parse HEAD
git status --short
rustc -Vv
cargo -V
cargo test --locked
cargo test --locked --no-default-features
cargo test --locked --all-features
cargo test --locked --release --all-features
cargo clippy --locked --all-targets --all-features
cargo tree --locked --all-features
```

Inspect every ignored test before enabling it; record whether it is a placeholder, known failure, or environmental test. Record build failures and pre-existing lint warnings separately from security findings. The manifest's toolchain metadata and CI's floating stable toolchain are not equivalent to a pinned, validated toolchain. Establish supported versions and pin the assessment environment.

**Deliverable:** baseline report with exact revision, build matrix, test results, ignored-test inventory, and reproducible environment instructions.

## 2. Define the threat model and security requirements

Map these paths and mark where authentication, validation, allocation, and privileged operations occur:

```text
Untrusted UDP -> socket -> handshake / authenticated decryption -> control or payload
                                                               -> peer / route state
                                                               -> TUN/TAP -> host network
Untrusted TCP/WebSocket -> proxy -> destination-selected UDP traffic
Beacon file / command output -> discovery -> candidate peer -> authentication
Local configuration -> startup / hooks / files / device setup -> privilege drop
Source / dependencies / CI -> release package -> installed service
```

Distinguish attackers who can send unauthenticated packets, intercept or replay traffic, operate an authenticated malicious peer, supply beacon contents, connect to the proxy, or modify local files as an unprivileged account. Treat administrators who intentionally configure shell hooks separately: executing an administrator-supplied command is expected behavior; an untrusted value reaching that command may cross a security boundary.

Agree on these requirements before assigning findings:

- A peer becomes trusted only after successful authentication; failed authentication must not install routes or deliver payloads.
- Encrypted configurations must not silently negotiate plaintext. Explicitly configured plaintext mode must have a documented, separate security contract.
- Nonces must not repeat under the same key. Specify the permitted reordering and replay window, including behavior after reconnect and rotation.
- Malformed input must not panic, corrupt memory, cause uncontrolled allocation, or prevent healthy peers from making progress.
- Define what authenticated peers may claim: identities, IP ranges, MAC addresses, and forwarded peers. Authentication alone must not be mistaken for route ownership enforcement.
- Discovery data must not bypass peer authentication. Define proxy client and destination restrictions for each supported deployment.
- Private keys, passwords, and plaintext traffic must not leak through logs, stats, command arguments, generated files, or release artifacts.
- Network input must not acquire shell execution or filesystem access beyond the intended service privileges.

**Deliverable:** a trust-boundary diagram, supported deployment assumptions, and a requirement-to-test checklist.

## 3. Conduct a prioritized manual review

Trace each candidate from an attacker-controlled entry point to its effect, including validation and configuration prerequisites. Review the deployed path, not only isolated helper functions.

| Priority | Code / surface | Review questions |
| --- | --- | --- |
| P0 | `src/crypto/{common,init,core,rotate}.rs` | Trusted-key selection, password-derived identity, handshake transcript binding, signature coverage, algorithm negotiation, key derivation, nonce separation and exhaustion, rotation state, replay handling, reflection, and cross-session confusion. Verify actual behavior against protocol comments. |
| P0 | `src/messages.rs`, `src/types.rs`, `src/payload.rs`, `src/util.rs` | Length/count validation, truncated fields, invalid prefixes, arithmetic and slice bounds, `MsgBuffer` headroom/tailroom, nested records, and allocations before authentication. |
| P0 | `src/cloud.rs`, `src/table.rs`, `src/net.rs` | Admission of peers, routing claims, address changes, MAC learning, peer exchange, reconnects, expiry, and control-message authorization. Check bounded work and memory for hostile peers and spoofed source addresses. |
| P1 | `src/wsproxy.rs` | Relay destination policy, private/loopback destinations, WebSocket frame limits, incomplete upgrades, stalled clients, thread/socket limits, backpressure, cleanup, and deployed transport protection. Review proxy exposure independently of VPN peer authentication. |
| P1 | `src/beacon.rs`, `src/port_forwarding.rs` | Beacon integrity and freshness, timestamp rollover, size limits, attacker-supplied addresses, command output handling, timeouts, and UPnP trust assumptions. |
| P1 | `src/main.rs`, `src/config.rs`, `src/device.rs`, `src/poll/` | Privilege-drop ordering and failure handling, supplementary groups, capabilities, hooks, ioctl/FFI safety, descriptor ownership, log/stats/PID paths, symlinks, and file permissions. |
| P2 | `src/wizard.rs`, `src/installer.rs`, `src/oldconfig.rs`, `assets/`, `.github/workflows/`, `.github/actions/` | Configuration migration and defaults, generated secrets, installation permissions, service confinement, dependency provenance, action permissions/pinning, downloaded build tools, and release integrity. |

P0/P1/P2 describe review order, not vulnerability severity. Request a cryptography specialist review of the custom handshake and rotation design after documenting the protocol and tests. Primitive selection and successful round trips do not establish protocol security. If authentication or forward-secrecy claims remain unclear, model the handshake in a protocol verification tool as a separate follow-up, with explicit assumptions and correspondence to the implementation.

## 4. Run complementary security tests

### A. Dependency and static analysis

Use `cargo-audit` against the application and fuzz lockfiles, recording the advisory database revision/date. It checks known advisories; `cargo-deny` can additionally enforce dependency source and license policy. See the [RustSec tooling documentation](https://rustsec.org/).

```sh
cargo audit --file Cargo.lock
cargo audit --file fuzz/Cargo.lock
```

Triage each advisory by resolved version, enabled features, reachable code, impact, and available upgrade. Do not automatically dismiss transitive dependencies or equate an advisory hit with an exploitable application flaw. Introduce a reviewed `deny.toml` before making `cargo deny check` a gate. Exceptions need an owner, reason, and expiry.

Use Clippy and targeted source searches to guide manual inspection of `unsafe`, `unwrap`, `expect`, unchecked indexing, shell execution, file creation, deserialization, and network reads. Scan the current tree and Git history for accidentally committed credentials; retain redacted evidence. Include container packages and build tools in the dependency inventory.

### B. Negative unit tests and property tests

Extend the existing tests with table-driven cases, then add generated properties where the input space is large:

- Truncate every valid message at every byte boundary; vary lengths/counts independently of actual data; test empty, maximum-size, and one-byte-over-limit inputs.
- For valid values, check encode/decode round trips and agreement between serialized lengths and buffer use. For malformed values, require a controlled error and no panic.
- Test all enabled AEAD algorithms with modified tags, ciphertext, nonce bytes, key IDs, wrong keys, and wrong sender direction. Ensure failed decryption never reaches the device or changes accepted peer/route state.
- Check wrong passwords, untrusted keys, invalid signatures, conflicting/repeated handshake fields, unexpected stages, and incompatible algorithm sets.
- Exercise nonce pinning immediately before and after housekeeping, duplicates inside and outside the accepted window, large nonce jumps, old keys, key-slot reuse, reconnects, and near-exhaustion counters. Document existing replay semantics before deciding whether stricter duplicate rejection is required.
- Verify invalid configuration, malformed CIDRs, unusual interface names, and conflicting CLI/file settings fail predictably without exposing secrets.

Add deterministic seeds and small reference models for address/range matching and state transitions. A property test needs a meaningful oracle, not just a second copy of the implementation.

### C. Coverage-guided fuzzing

First review the local harnesses:

- `init_msg` passes an empty trusted-key list; parsing exits at the trust lookup before most fields are processed. Add a known trusted key and matching header, plus valid signed messages to exercise later validation and state transitions.
- `decrypt` currently checks length against a buffer with zero headroom but writes into one with 100 bytes of headroom. Align the guard with the actual writable slice before attributing large-input crashes to production code.
- Random ciphertext generally fails authentication. Add valid encrypt-then-mutate inputs and stateful sequences for decryption, replay checks, rotation, and reconnects.
- Reset mutable state between fuzz cases, use deterministic time where practical, and impose step/queue limits in simulation harnesses. Distinguish an intentional harness budget stop from an application hang.

Use the existing targets and corpora after that review. Install and record a compatible nightly toolchain and `cargo-fuzz` version; confirm target names with `cargo +nightly fuzz list`. Example bounded campaign from the repository root:

```sh
cargo +nightly fuzz run payload -- -max_total_time=600 -timeout=10 -rss_limit_mb=2048
cargo +nightly fuzz run node_info -- -max_total_time=600 -timeout=10 -rss_limit_mb=2048
cargo +nightly fuzz run range -- -max_total_time=600 -timeout=10 -rss_limit_mb=2048
cargo +nightly fuzz run beacon -- -max_total_time=600 -timeout=10 -rss_limit_mb=2048
cargo +nightly fuzz run init_msg -- -max_total_time=600 -timeout=10 -rss_limit_mb=2048
cargo +nightly fuzz run decrypt -- -max_total_time=600 -timeout=10 -rss_limit_mb=2048
```

These are starting budgets, not evidence of adequate coverage. Set input-size limits appropriate to each protocol surface, including boundary sizes. Run longer campaigns of 8–24 hours per important target once coverage demonstrates that meaningful paths are reached. Preserve corpus growth, execution count, coverage, tool versions, crashes, timeouts, and OOM inputs. Minimize failures and replay them against production entry points. The [Rust Fuzz Book](https://rust-fuzz.github.io/book/cargo-fuzz/tutorial.html) documents the cargo-fuzz workflow.

Proposed new targets: rotation messages; full handshake event sequences; peer admission and control messages through `GenericCloud`; WebSocket address/frame decoding; and configuration parsing. Keep unauthenticated rejection and authenticated malicious-peer campaigns separate.

### D. Stateful simulation and fault injection

Extend `Simulator` in `src/tests/common.rs` to capture, mutate, duplicate, delay, reorder, inject, and replay messages. Bound simulation steps so a message loop cannot hang the test runner.

Run two- and three-peer scenarios with simultaneous connection attempts, lost handshake/rotation messages, peer restarts, NAT address changes, inconsistent peer lists, overlapping claims, stale announcements, and time jumps. Assert both security and recovery: rejected traffic produces no unauthorized delivery or route changes, stale state expires, and healthy peers resume communication within an explicit timeout. Record a deterministic event trace for each failure.

### E. Real network integration and adversarial traffic

The current smoke test can be reproduced with Docker on a disposable Linux test host:

```sh
docker build -f contrib/e2e/Dockerfile -t vpncloud-e2e .
docker run --rm --privileged vpncloud-e2e
```

It requires Docker and host TUN support. Its privileged container and root-level log paths make it unsuitable for running directly on a normal workstation shell. The current ping targets addresses in the same network namespace, so local routing may satisfy it without traversing the VPN. Treat it as a starting harness, not proof of encrypted delivery.

Build a replacement topology with peers in separate namespaces/containers, a controlled underlay, and an attacker node. Use only the capabilities needed for TUN/TAP and network setup once established. Verify routing and packet counters, capture the underlay, and require the same delivery test to fail when the VPN is stopped or credentials differ.

Exercise TUN and TAP, hub/switch/router modes, VLAN handling, IPv4/IPv6 underlays, direct UDP and WebSocket transport. Add malformed datagrams, corrupted/replayed captures, spoofed sources, MTU boundaries, loss, latency, duplication, reordering, and link outages. Check that known test plaintext is absent from encrypted underlay payloads and that tampered traffic is never delivered. Packet captures support specific observations; they do not prove cryptographic security.

For the proxy, test slow upgrades, incomplete/oversized/fragmented frames, connection churn, stalled readers, and destinations represented by isolated local fixtures. Verify the intended relay policy and cleanup after disconnects.

### F. Denial-of-service and resource tests

Measure CPU, RSS, threads, file descriptors, peer/route table size, response bytes per request byte, log growth, and legitimate peer latency under increasing hostile traffic. Cover unauthenticated handshakes, random UDP, many apparent source addresses, malicious authenticated announcements, beacon growth, and idle WebSocket clients.

Before each run, set a packet rate, duration, machine resource cap, and abort threshold. Establish acceptable resource and recovery budgets from the baseline and supported deployment size. Require bounded growth, continued healthy-peer progress, and return toward baseline after the load stops. Run stress and spoofing scenarios only inside the isolated test topology.

### G. Memory safety, coverage, and deployment tests

Run sanitizer-enabled fuzzing and targeted AddressSanitizer builds where supported, focusing on buffer boundaries, FFI, ioctl structures, and descriptor lifetimes. Native dependencies may need separate instrumentation; document what is covered. Apply [Miri](https://github.com/rust-lang/miri) to isolated compatible Rust tests for undefined behavior; full network/FFI/cryptography execution may require extracting a pure test harness. Unsupported execution is a coverage limitation, not a pass.

Generate a coverage report using [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov), with its required LLVM tools installed:

```sh
cargo llvm-cov --locked --all-features --html
```

Review uncovered authentication failures, state transitions, parser boundaries, and cleanup paths. Supplement line coverage with a requirement checklist and targeted test mutations: a test should fail if its relevant authentication or validation check is removed in an isolated test branch.

In disposable VMs, test installed packages and `assets/vpncloud@.service` / `assets/vpncloud-wsproxy.service`. Check effective UID/GID/groups/capabilities after startup and after daemonization, privilege-drop failures, filesystem confinement, config/key modes, symlink handling for output paths, hook environment handling, restart behavior, and resource limits. Confirm systemd restrictions still allow ordinary VPN operation.

## 5. Validate findings and turn them into regressions

For each candidate, retain:

1. Affected revision, features, deployment, attacker access, and trust boundary.
2. Source path from input to effect, with relevant checks and prerequisites.
3. Minimal reproducer, expected/observed behavior, logs or captures, and resource measurements where relevant.
4. Impact and severity rationale, distinguishing remote unauthenticated attacks from authenticated-peer or local-only attacks.
5. Proposed fix, compatibility considerations, and a regression that fails before the fix and passes afterward.

Classify outcomes as confirmed, rejected, or unresolved with a reason. Separate harness defects, intentional configuration behavior, hardening opportunities, and application vulnerabilities. Re-run the reproducer and affected feature/integration tests after each fix. Preserve minimized fuzz inputs as regressions without committing secrets or large generated build trees.

## 6. Execution order and ongoing gates

| Phase | Work | Completion evidence |
| --- | --- | --- |
| 1 — Baseline | Snapshot checkout, build/test matrix, dependency inventory, threat model | Reproducible environment and agreed security requirements |
| 2 — Highest-risk paths | Review crypto, parsers, peer admission, and routing; add negative tests; repair fuzz harnesses | Input-to-effect traces and tests for each P0 requirement |
| 3 — Dynamic campaigns | Stateful fuzzing, isolated network tests, resource tests, proxy and privilege checks | Saved campaign metadata, coverage gaps, minimized reproducers |
| 4 — Remediation | Validate and rank findings, fix, add regressions, obtain specialist protocol review | Fix evidence and explicitly recorded unresolved risks |
| 5 — Continuous checks | Integrate practical checks into CI | Required jobs with owners and retained artifacts |

Suggested CI split:

- **Each pull request:** default/minimal/all-feature tests, Clippy against an agreed baseline, dependency audit, security regressions, short bounded fuzz smoke tests, and isolated integration tests when relevant.
- **Nightly:** longer fuzzing, release-mode tests, dependency database refresh, coverage reports, and resource/recovery scenarios.
- **Before release:** sustained campaigns, supported platform/package tests, privilege checks, protocol interoperability, specialist review for crypto changes, and review of outstanding findings.

Pin tool and action versions, grant CI minimal permissions, and keep untrusted PR jobs away from release credentials and privileged persistent runners. Preserve logs and crash artifacts even on job failure.

The first assessment is complete when every scoped trust boundary has review evidence, all priority requirements have tests or an explicit limitation, confirmed findings have reproducible records, and critical/high findings are fixed or explicitly accepted by the responsible maintainer. Publish an assessment summary containing the exact scope, results, remaining gaps, and follow-up owners. Passing tests or a crash-free fuzz run alone is not a security certification.
