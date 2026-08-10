#

CRI	?= $(shell echo -en podman ; [ -e /run/.containerenv ] && echo -en -remote)

# rust code build
build: check
		cargo build

check: fmt clippy
		cargo check

clippy:
		cargo clippy

fmt:
		cargo fmt

release: check
		cargo build --release

# container image build
engine: rust-builder
		$(CRI) build -f docker/$@.containerfile -t uefipatcher-$@ .

gateway: rust-builder
		$(CRI) build -f docker/$@.containerfile -t uefipatcher-$@ .

runtime-base:
		$(CRI) build -f docker/$@.containerfile -t uefipatcher-$@ .

rust-builder: runtime-base
		$(CRI) build -f docker/$@.containerfile -t uefipatcher-$@ .

webui: rust-builder runtime-base
		$(CRI) build -f docker/$@.containerfile -t uefipatcher-$@ .
