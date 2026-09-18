FROM uefipatcher-runtime-base
RUN dnf install -y git nasm gcc make python3 libuuid-devel python3-capstone && dnf clean all
ENV EDK2_BUILDER_CONTAINER=1
WORKDIR /work
