# cvmpay payment service. Build context: server/.
FROM rust:1.99-slim-trixie AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked --bin cvmpay \
 && strip target/release/cvmpay

FROM debian:trixie-slim
# The service needs no system certificates (relay TLS uses the bundled
# webpki roots; Electrum TLS is pinned or checked against the same roots).
RUN useradd --system --uid 10001 --home-dir /data --shell /usr/sbin/nologin cvmpay \
 && install -d -o cvmpay -g cvmpay -m 0700 /data \
 && install -d -o cvmpay -g cvmpay -m 0755 /public
COPY --from=build /src/target/release/cvmpay /usr/local/bin/cvmpay
COPY --chmod=0755 entrypoint.sh /usr/local/bin/cvmpay-entrypoint
USER cvmpay
ENV CVMPAY_DATA_DIR=/data
VOLUME ["/data"]
ENTRYPOINT ["/usr/local/bin/cvmpay-entrypoint"]
