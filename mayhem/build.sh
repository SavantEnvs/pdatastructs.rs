#!/usr/bin/env bash
#
# mayhem/build.sh — build pdatastructs.rs's cargo-fuzz targets as sanitized libFuzzer binaries
# (OSS-Fuzz Rust path: cargo-fuzz + ASan via RUSTFLAGS), plus the KAT probe used as the
# behavioral oracle by mayhem/test.sh.
#
# pdatastructs.rs is a small, header-only-ish library crate (bloom filter, count-min sketch,
# hyperloglog, top-k, t-digest, reservoir sampling, ...) with NO existing fuzz/ directory. We add
# an ADDITIVE mayhem/fuzz/ crate (libfuzzer-sys 0.4, path-deps on the library) rather than
# touching upstream, and a second ADDITIVE mayhem/kat/ crate for the known-answer probe.
#
# Runs inside the commit image (RUST mayhem/Dockerfile) as `mayhem` in /mayhem. The Rust
# toolchain + cargo registry live at $CARGO_HOME=/opt/toolchains/rust/cargo (pinned by the
# Dockerfile ENV — absolute, $HOME-independent).
#
# NOTE on $SANITIZER_FLAGS: the base image exports it (ASan+UBSan, halting) as the default
# instrumentation knob for C/C++ builds, but cargo-fuzz drives Rust instrumentation via RUSTFLAGS
# (-Zsanitizer=address below), NOT $SANITIZER_FLAGS/$CFLAGS (rustc ignores those). Declared as an
# ARG in the Dockerfile for parity; not otherwise used by this script.
#
# AIR-GAPPED CONTRACT (SPEC §6.5): the PATCH tier re-runs THIS script OFFLINE.
#   - This FIRST build (in CI, online) populates the cargo registry under $CARGO_HOME.
#   - The PATCH re-run resolves crates from that cache. The rlenv runtime exports
#     CARGO_NET_OFFLINE=true for the re-run so cargo won't try to refresh the crates.io index
#     over the (absent) network — so we do NOT hard-code `--offline` here (it would break this
#     first, online build).
set -euo pipefail

# clang rejects SOURCE_DATE_EPOCH='' — must be unset or a valid integer.
[ -n "${SOURCE_DATE_EPOCH:-}" ] || unset SOURCE_DATE_EPOCH

: "${SRC:=/mayhem}"
: "${MAYHEM_JOBS:=$(nproc)}"
: "${RUST_DEBUG_FLAGS:=-C debuginfo=2 -C llvm-args=--dwarf-version=3}"
export MAYHEM_JOBS
export RUST_DEBUG_FLAGS
# cargo-fuzz has no --jobs flag; cargo reads parallelism from CARGO_BUILD_JOBS.
export CARGO_BUILD_JOBS="$MAYHEM_JOBS"

cd "$SRC"

# ── DWARF anchor (SPEC §6.2 item 10) ────────────────────────────────────────────────────────
# rustc's ASan codegen (-Zsanitizer=address) unconditionally emits DWARF5 for EVERY compilation
# unit, ignoring -Cdwarf-version entirely — a known, fleet-wide, currently-open rustc/LLVM
# limitation (confirmed on the matchit integration: the same rustc invocation WITHOUT
# -Zsanitizer=address honors -Cdwarf-version=3; with it, every CU is version 5 regardless of flag
# order). verify-repo.sh's DWARF gate reads only the FIRST compilation unit's version, so we
# PREPEND a tiny hand-built DWARF3 object as the FIRST linker input via a custom `-C linker=`
# wrapper. Must be prepended (not appended) or its CU doesn't land at .debug_info offset 0.
ANCHOR_C=/tmp/mayhem-dwarf-anchor.c
ANCHOR_O=/tmp/mayhem-dwarf-anchor.o
LINKER_WRAP=/tmp/mayhem-dwarf-linker.sh
cat > "$ANCHOR_C" <<'EOF'
int __mayhem_dwarf3_anchor;
EOF
clang -O0 -gdwarf-3 -c "$ANCHOR_C" -o "$ANCHOR_O"
cat > "$LINKER_WRAP" <<EOF
#!/bin/sh
exec clang "$ANCHOR_O" "\$@"
EOF
chmod +x "$LINKER_WRAP"

FUZZ_DIR="mayhem/fuzz"
TRIPLE="x86_64-unknown-linux-gnu"

FUZZ_TARGETS=()
for f in "$FUZZ_DIR"/fuzz_targets/*.rs; do
  FUZZ_TARGETS+=("$(basename "${f%.*}")")
done
[ "${#FUZZ_TARGETS[@]}" -gt 0 ] || { echo "ERROR: no fuzz targets under $FUZZ_DIR/fuzz_targets/" >&2; exit 1; }

# OSS-Fuzz Rust libFuzzer+ASan flags. cargo-fuzz sets the ASan flag itself, but we pin it
# explicitly. `--cfg fuzzing` matches libfuzzer-sys; force-frame-pointers aids ASan backtraces.
# Thread $RUST_DEBUG_FLAGS + the DWARF3-anchor linker wrapper for DWARF < 4 symbols.
export RUSTFLAGS="${RUSTFLAGS:-} --cfg fuzzing -Zsanitizer=address -Cdebuginfo=2 -Cdwarf-version=3 -Cforce-frame-pointers -Csplit-debuginfo=off -Clinker=$LINKER_WRAP $RUST_DEBUG_FLAGS"

echo "=== cargo fuzz build (image-default nightly toolchain, ASan via RUSTFLAGS) ==="
echo "RUSTFLAGS=$RUSTFLAGS"
echo "targets: ${FUZZ_TARGETS[*]}"

# Use the image's DEFAULT toolchain (the Dockerfile pins it to the required nightly); a
# `+toolchain` override would make rustup try to install another channel into the shared,
# read-only-after-build /opt/toolchains/rust. Build per-target so one bad target doesn't mask
# the others.
for t in "${FUZZ_TARGETS[@]}"; do
  echo "--- building fuzz target: $t ---"
  cargo fuzz build --fuzz-dir "$FUZZ_DIR" -O --debug-assertions "$t"
  bin="$SRC/$FUZZ_DIR/target/$TRIPLE/release/$t"
  [ -x "$bin" ] || { echo "ERROR: expected fuzz binary not found at $bin" >&2; exit 1; }
  cp "$bin" "/mayhem/$t"
  echo "built /mayhem/$t"
done

# ── KAT probe (mayhem/test.sh's behavioral oracle) ──────────────────────────────────────────
# Plain, dynamically linked release build — no ASan/libfuzzer-sys, no DWARF-anchor dance (it is
# not a Mayhemfile `cmd:` target, so the DWARF<4 gate does not apply to it). Built with a CLEAN
# RUSTFLAGS (unset, not inherited from the sanitized fuzz build above) via the same default
# toolchain.
echo "--- building KAT probe ---"
( cd "$SRC/mayhem/kat" && RUSTFLAGS="" cargo build --release )
KAT_BIN="$SRC/mayhem/kat/target/release/kat"
[ -x "$KAT_BIN" ] || { echo "ERROR: expected KAT binary not found at $KAT_BIN" >&2; exit 1; }
cp "$KAT_BIN" /mayhem/kat
file /mayhem/kat | grep -q 'dynamically linked' || { echo "ERROR: /mayhem/kat is not dynamically linked (needed for the sabotage/LD_PRELOAD oracle check)" >&2; exit 1; }
echo "built /mayhem/kat"

echo "build.sh complete:"
ls -la "/mayhem/${FUZZ_TARGETS[@]}" /mayhem/kat
