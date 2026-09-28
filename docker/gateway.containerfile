FROM uefipatcher-rust-builder AS rust-builder
WORKDIR /app
COPY . .
RUN cargo build --release -p uefi-gateway

FROM registry.fedoraproject.org/fedora:44 AS webui-builder
RUN dnf install -y nodejs npm && dnf clean all
WORKDIR /app
COPY webui/package*.json ./
RUN npm ci
COPY webui/ .
RUN npm run build

FROM uefipatcher-runtime-base
RUN dnf install -y ca-certificates && dnf clean all
COPY --from=rust-builder /app/target/release/uefi-gateway /usr/local/bin/
COPY --from=webui-builder /app/build /opt/webui
ENV UEFIPATCHER_GATEWAY_LISTEN=0.0.0.0:8080
ENV UEFIPATCHER_WEBUI_DIR=/opt/webui
EXPOSE 8080
ENTRYPOINT ["uefi-gateway"]
