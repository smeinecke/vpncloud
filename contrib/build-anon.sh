#!/usr/bin/env bash
# Build anonymized vpncloud release binaries inside Docker.
#
# - compiles inside a clean container, no host paths can leak in
# - --remap-path-prefix removes /src and cargo registry paths from
#   embedded file!() strings and panic locations
# - release profile already strips symbols (strip = true, lto = true)
#
# Variants (arg 1, default: all):
#   glibc - dynamic binary for Debian bookworm and newer (built against
#           bookworm glibc 2.36; runs on trixie via glibc forward compat)
#   musl  - fully static binary, runs on any Linux (no libc dependency)
#   all   - build both
#
# Output: dist-anon/vpncloud, dist-anon/vpncloud-static
#
# Usage: contrib/build-anon.sh [glibc|musl|all]
set -euo pipefail

OUT_DIR=dist-anon
IMAGE=rust:1-bookworm
REGISTRY_VOL=vpncloud-cargo-registry
VARIANT=${1:-all}

mkdir -p "$OUT_DIR"

common_args=(
  --rm
  --volume "$PWD":/src:ro
  --volume "$PWD/$OUT_DIR":/out
  --volume "$REGISTRY_VOL":/usr/local/cargo/registry
  --workdir /src
  --env "RUSTFLAGS=--remap-path-prefix=/src=vpncloud --remap-path-prefix=/usr/local/cargo/registry=cargo"
)

build_glibc() {
  docker run "${common_args[@]}" \
    --env CARGO_TARGET_DIR=/build/target-glibc \
    "$IMAGE" \
    sh -c 'cargo build --release --locked &&
             cp /build/target-glibc/release/vpncloud /out/vpncloud'
}

build_musl() {
  docker run "${common_args[@]}" \
    --env CARGO_TARGET_DIR=/build/target-musl \
    "$IMAGE" \
    sh -c 'apt-get update -qq &&
             apt-get install -y --no-install-recommends musl-tools >/dev/null &&
             rustup target add x86_64-unknown-linux-musl >/dev/null &&
             cargo build --release --locked --target x86_64-unknown-linux-musl &&
             cp /build/target-musl/x86_64-unknown-linux-musl/release/vpncloud /out/vpncloud-static'
}

case "$VARIANT" in
  glibc) build_glibc ;;
  musl)  build_musl ;;
  all)   build_glibc; build_musl ;;
  *) echo "usage: $0 [glibc|musl|all]" >&2; exit 2 ;;
esac

# Verify the binaries: symbols stripped, no local paths embedded.
# /rustc/... and /rust/deps/... paths come from the precompiled stdlib and
# are present in every Rust binary; they identify no user. What must not
# appear is the build host's paths ($HOME, /home/*, /Users/*, the checkout).
echo "== verification =="
rc=0
for bin in "$OUT_DIR"/vpncloud*; do
  [ -f "$bin" ] || continue
  echo "-- $bin"
  command -v file >/dev/null && file "$bin"
  leaks=$(strings "$bin" | grep -aE "$HOME|/home/|/Users/|/usr/local/cargo|$PWD" | head -5 || true)
  if [ -n "$leaks" ]; then
    echo "WARNING: binary still contains local paths:" >&2
    echo "$leaks" >&2
    rc=1
  fi
  if command -v nm >/dev/null && nm "$bin" 2>/dev/null | head -1 | grep -q .; then
    echo "WARNING: binary still contains symbols" >&2
    rc=1
  fi
done
[ "$rc" -eq 0 ] && echo "OK: binaries are stripped and contain no local paths"
exit "$rc"
