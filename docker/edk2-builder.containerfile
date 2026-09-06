FROM uefipatcher-runtime-base
RUN dnf install -y git nasm gcc make python3 libuuid-devel && dnf clean all
ENV EDK2_BUILDER_CONTAINER=1
WORKDIR /work
