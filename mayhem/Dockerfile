# syntax=docker/dockerfile:1.7
#
# Commit-image Dockerfile (RUST target — cargo / cargo-fuzz). Builds ON TOP of the org base image
# (ghcr.io/savantenvs/base). pdatastructs.rs is a small probabilistic-data-structures crate
# (bloom filter, count-min sketch, hyperloglog, top-k, t-digest, reservoir sampling). The base
# ships NO Rust toolchain, so this image installs rustup (a pinned nightly — cargo-fuzz needs
# `-Z` flags) + cargo-fuzz as root into a SHARED location (/opt/toolchains/rust) usable by the
# non-root `mayhem` user, then runs build.sh (cargo fuzz build + KAT probe) and test.sh
# (pdatastructs's own cargo test suite + the KAT probe).
#
# Build (from the repo root, on the `mayhem` branch):
#   DOCKER_BUILDKIT=1 docker build -f mayhem/Dockerfile \
#       --build-arg COMMIT=$(git rev-parse HEAD) \
#       [--build-arg SOURCE_DATE_EPOCH=$(date +%s)] \
#       -t pdatastructs.rs-commit:test .

FROM ghcr.io/savantenvs/base:20260621

ARG COMMIT
ARG REPO_URL
# SOURCE_DATE_EPOCH stays an ARG (available to build.sh); never bake it as ENV — an empty value
# breaks clang. build.sh unsets it when empty.
ARG SOURCE_DATE_EPOCH

# Sanitizer override: the base exports SANITIZER_FLAGS (ASan+UBSan, halting) as the default, but
# cargo-fuzz drives Rust instrumentation via RUSTFLAGS (-Zsanitizer=address), NOT $SANITIZER_FLAGS.
# Declared here for parity; build.sh wires ASan into RUSTFLAGS the cargo-fuzz way.
ARG SANITIZER_FLAGS
ARG MAYHEM_JOBS=

# Pin the nightly toolchain (cargo-fuzz needs nightly for `-Z` flags / -Zsanitizer=address).
# nightly-2025-06-01 is a fleet-proven-good pin (nightly-2025-01-15 is known to ICE compiling the
# `log` crate under -Zsanitizer=address); pdatastructs.rs uses `edition = "2024"` which needs a
# rustc new enough to have stabilized it (>= 1.85, well before this nightly date).
ARG RUST_TOOLCHAIN=nightly-2025-06-01
ARG CARGO_FUZZ_VERSION=0.12.0
# A second, STABLE toolchain for the ORACLE path only (mayhem/test.sh's `cargo +stable test`).
# The pinned nightly builds the fuzz targets fine, but pdatastructs.rs's dev-dependency graph
# (chacha20/rand_distr -> zerocopy 0.8.x) hits zerocopy's `stdarch_x86_avx512` unstable-feature
# gate on this specific nightly snapshot (E0658) when compiling `cargo test`'s dev-deps — a
# known nightly-vs-transitive-runtime-dep trap, not a project bug. Keep fuzz targets pinned to
# the ASan-capable nightly; run the oracle on stable instead of fighting the transitive dep.
ARG RUST_STABLE_TOOLCHAIN=stable

# Shared Rust install: CARGO_HOME/RUSTUP_HOME under /opt/toolchains/rust, world-readable, so the
# non-root `mayhem` user can invoke cargo/rustc/cargo-fuzz without owning the toolchain.
ENV RUSTUP_HOME=/opt/toolchains/rust/rustup \
    CARGO_HOME=/opt/toolchains/rust/cargo \
    PATH=/opt/toolchains/rust/cargo/bin:$PATH

USER root
RUN set -eux; \
    mkdir -p "$RUSTUP_HOME" "$CARGO_HOME"; \
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --no-modify-path --profile minimal --default-toolchain "$RUST_TOOLCHAIN"; \
    rustup component add rust-src --toolchain "$RUST_TOOLCHAIN"; \
    rustup toolchain install "$RUST_STABLE_TOOLCHAIN" --profile minimal; \
    cargo install cargo-fuzz --version "$CARGO_FUZZ_VERSION" --locked; \
    # The non-root `mayhem` user runs `cargo fuzz build` / `cargo build`, which WRITE into
    # CARGO_HOME (registry cache, git checkouts). Toolchain dirs stay read-only; the cargo home
    # must be group/other writable so the build can populate the registry.
    chmod -R a+rX /opt/toolchains/rust; \
    mkdir -p "$CARGO_HOME/registry" "$CARGO_HOME/git" && chmod -R a+rwX "$CARGO_HOME/registry" "$CARGO_HOME/git"; \
    rustc --version; cargo --version; cargo fuzz --version
USER mayhem

# Source enters from the build CONTEXT (the CI checkout): the FULL repo (incl. .git history) is
# copied to /mayhem as `mayhem`; build.sh builds in place.
WORKDIR /mayhem
COPY --chown=2000:2000 . /mayhem

RUN bash mayhem/build.sh

# Run pdatastructs.rs's real test suite + the KAT probe at build time; a failing suite fails the
# image build.
RUN if [ -f mayhem/test.sh ]; then bash mayhem/test.sh; else echo "no mayhem/test.sh"; fi

LABEL org.opencontainers.image.source="${REPO_URL}" \
      org.opencontainers.image.revision="${COMMIT}" \
      rlenv.tier="commit" \
      rlenv.source_date_epoch="${SOURCE_DATE_EPOCH}" \
      rlenv.user="mayhem"

USER mayhem
WORKDIR /mayhem
