FROM uefipatcher-rust-builder AS builder
WORKDIR /app
COPY . .
RUN cargo build --release --bin engine -p uefi-engine

FROM uefipatcher-runtime-base
COPY --from=builder /app/target/release/engine /usr/local/bin/uefi-engine
VOLUME ["/data", "/run/uefipatcher"]
ENV UEFIPATCHER_DATA=/data
ENV UEFIPATCHER_SOCK=/run/uefipatcher/uefipatcher.sock
ENV UEFIPATCHER_PURGE_ARTIFACTS=false
ENTRYPOINT ["uefi-engine"]
