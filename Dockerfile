# syntax=docker/dockerfile:1

# The builder links the binary statically against musl, so it runs on a base with no C library and no shell.

FROM rust:1.96.1-trixie AS builder
# BuildKit sets TARGETARCH for each platform of the build, and a stage sees it only once it declares it.
ARG TARGETARCH
WORKDIR /src
COPY . .
RUN case "$TARGETARCH" in \
      amd64) target=x86_64-unknown-linux-musl ;; \
      arm64) target=aarch64-unknown-linux-musl ;; \
      *) echo "no static target for the architecture $TARGETARCH" >&2; exit 1 ;; \
    esac \
 && rustup target add "$target" \
 && cargo build --release --locked --target "$target" \
 && cp "target/$target/release/phoebis-proxy" /phoebis-proxy

# gcr.io/distroless/static-debian13:nonroot, resolved 2026-11-27
FROM gcr.io/distroless/static-debian13@sha256:8ce6dd87e9823c5b48dcfb2c8604714ccb351ce59607327bcc4957448da279b5
COPY --from=builder /phoebis-proxy /usr/local/bin/phoebis-proxy
COPY --from=builder /src/LICENSE /usr/share/doc/phoebis-proxy/LICENSE
# A container's loopback is its own, so the image listens on each interface, and the proxy then starts only with an
# allow list.
ENV PHOEBIS_PROXY_LISTEN=0.0.0.0:8318
USER 65532
EXPOSE 8318
ENTRYPOINT ["/usr/local/bin/phoebis-proxy"]
