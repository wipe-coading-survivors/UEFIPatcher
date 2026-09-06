FROM registry.fedoraproject.org/fedora:44
RUN dnf install -y ca-certificates sqlite-libs && dnf clean all
