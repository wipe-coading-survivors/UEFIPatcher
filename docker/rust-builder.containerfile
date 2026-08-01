FROM registry.fedoraproject.org/fedora:44
RUN dnf install -y rust cargo protobuf-compiler make gcc && dnf clean all
WORKDIR /app
