FROM uefipatcher-rust-builder AS builder
RUN dnf install -y nodejs npm && dnf clean all
WORKDIR /app
COPY webui/package*.json ./
RUN npm install
COPY webui/ .
RUN npm run build

FROM uefipatcher-runtime-base
RUN dnf install -y nginx && dnf clean all
COPY --from=builder /app/build /usr/share/nginx/html
COPY docker/webui-nginx.conf /etc/nginx/conf.d/default.conf
EXPOSE 80
ENTRYPOINT ["nginx", "-g", "daemon off;"]
