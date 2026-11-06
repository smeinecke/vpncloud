FROM rust:1-bookworm AS builder
WORKDIR /src
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends iproute2 iputils-ping procps netcat-openbsd iperf3 \
 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /src/target/release/vpncloud /usr/local/bin/vpncloud
COPY contrib/e2e/run-test.sh /run-test.sh
ENTRYPOINT ["/run-test.sh"]
