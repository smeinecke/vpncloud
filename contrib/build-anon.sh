#!/usr/bin/env bash
# Build an anonymized vpncloud release binary inside Docker.
#
# - compiles inside a clean container, no host paths can leak in
# - --remap-path-prefix removes /src and cargo registry paths from
#   embedded file!() strings and panic locations
# - release profile already strips symbols (strip = true, lto = true)
# - output: dist-anon/vpncloud
#
# Usage: contrib/build-anon.sh
set -euo pipefail

OUT_DIR=dist-anon
IMAGE=rust:1-bookworm
REGISTRY_VOL=vpncloud-cargo-registry

mkdir -p "$OUT_DIR"

docker run --rm \
  --volume "$PWD":/src:ro \
  --volume "$PWD/$OUT_DIR":/out \
  --volume "$REGISTRY_VOL":/usr/local/cargo/registry \
  --workdir /src \
  --env CARGO_TARGET_DIR=/build/target \
  --env RUSTFLAGS="--remap-path-prefix=/src=vpncloud --remap-path-prefix=/usr/local/cargo/registry=cargo" \
  "$IMAGE" \
  sh -c 'cargo build --release --locked && cp /build/target/release/vpncloud /out/vpncloud'

# Verify the binary: symbols stripped, no local paths embedded.
echo "== verification =="
if command -v file >/dev/null; then
  file "$OUT_DIR/vpncloud"
fi
# /rustc/... and /rust/deps/... paths come from the precompiled stdlib and
# are present in every Rust binary; they identify no user. What must not
# appear is the build host's paths ($HOME, /home/*, /Users/*, the checkout).
leaks=$(strings "$OUT_DIR/vpncloud" | grep -aE "$HOME|/home/|/Users/|/usr/local/cargo|$PWD" | head -5 || true)
if [ -n "$leaks" ]; then
  echo "WARNING: binary still contains local paths:" >&2
  echo "$leaks" >&2
  exit 1
fi
if command -v nm >/dev/null && nm "$OUT_DIR/vpncloud" 2>/dev/null | head -1 | grep -q .; then
  echo "WARNING: binary still contains symbols" >&2
  exit 1
fi
echo "OK: $OUT_DIR/vpncloud is stripped and contains no local paths"
