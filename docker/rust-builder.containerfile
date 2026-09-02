FROM registry.fedoraproject.org/fedora:44
RUN dnf install -y rust cargo protobuf-compiler make gcc xz-devel && dnf clean all
WORKDIR /app
