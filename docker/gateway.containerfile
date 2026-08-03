# Builder переиспользует общий rust-builder.containerfile (Цикл 1: fedora:44 + rust/cargo/protobuf-compiler)
FROM uefipatcher-rust-builder AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p uefi-gateway

FROM registry.fedoraproject.org/fedora:44
RUN dnf install -y ca-certificates && dnf clean all
COPY --from=builder /app/target/release/uefi-gateway /usr/local/bin/
ENV UEFIPATCHER_GATEWAY_LISTEN=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["uefi-gateway"]
