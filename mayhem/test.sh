#!/usr/bin/env bash
#
# mayhem/test.sh — RUN pdatastructs.rs's own functional test suite AND the KAT probe (already
# built by mayhem/build.sh), emit a combined CTRF summary. exit 0 iff nothing failed.
#
# PATCH-grade oracle, in two layers:
#
#  1. `cargo test --all-features` — pdatastructs.rs's own ~200 #[test] unit tests (bloom filter,
#     count-min sketch, hyperloglog, top-k, t-digest, reservoir sampling, ...) plus its doctests,
#     which assert exact values via assert_eq!/assert!. This is a real, substantial suite.
#
#  2. The `/mayhem/kat` probe — REQUIRED in addition to (1), not merely a fallback. Per the net-new
#     porting brief, "go test / cargo test alone" is a FORBIDDEN sole oracle: cargo emits a
#     statically-linked test harness binary that the gate's LD_PRELOAD sabotage shim cannot
#     neuter, so a suite driven purely by `cargo test` can survive "the program does nothing"
#     unchanged and prove nothing behavioral. `/mayhem/kat` is a small, ordinary, DYNAMICALLY
#     LINKED release binary (verified by build.sh via `file | grep 'dynamically linked'`) that
#     runs three known-answer scenarios (bloom filter membership, count-min sketch exact counts,
#     hyperloglog cardinality-within-tolerance + merge/clear invariants) and prints one
#     `<NAME>_OK` marker line per scenario plus a final `KAT_ALL_OK`. Under the sabotage shim this
#     binary is `_exit(0)`'d before it prints anything, so the markers vanish and this script
#     fails — making the combined oracle genuinely behavioral.
#
# Does NOT build — build.sh already compiled the fuzz targets and the KAT probe with the project's
# normal flags is compiled here via `cargo build` only if missing (defensive; build.sh should have
# produced it). `cargo test` itself both compiles and runs (there is nothing to "pre-build" for
# it — Rust has no separate test-runner artifact step the way ctest/gtest do), and this only
# happens at commit-image BUILD time (online); the offline PATCH tier re-runs build.sh, not this
# script.
set -uo pipefail
[ -n "${SOURCE_DATE_EPOCH:-}" ] || unset SOURCE_DATE_EPOCH

: "${SRC:=/mayhem}"
: "${MAYHEM_JOBS:=$(nproc)}"
cd "$SRC"

# emit_ctrf <tool> <passed> <failed> [skipped] [pending] [other]
emit_ctrf() {
  local tool="$1" passed="$2" failed="$3" skipped="${4:-0}" pending="${5:-0}" other="${6:-0}"
  local tests=$(( passed + failed + skipped + pending + other ))
  cat > "${CTRF_REPORT:-$SRC/ctrf-report.json}" <<JSON
{
  "results": {
    "tool": { "name": "$tool" },
    "summary": {
      "tests": $tests,
      "passed": $passed,
      "failed": $failed,
      "pending": $pending,
      "skipped": $skipped,
      "other": $other
    }
  }
}
JSON
  printf 'CTRF {"results":{"tool":{"name":"%s"},"summary":{"tests":%d,"passed":%d,"failed":%d,"pending":%d,"skipped":%d,"other":%d}}}\n' \
    "$tool" "$tests" "$passed" "$failed" "$pending" "$skipped" "$other"
  [ "$failed" -eq 0 ]
}

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not available — cannot run the test suite" >&2
  emit_ctrf "cargo-test+kat" 0 1 0
  exit 2
fi

# ── 1. pdatastructs.rs's own suite ──────────────────────────────────────────────────────────
# Run on the STABLE toolchain, not the pinned ASan nightly build.sh uses for the fuzz targets:
# pdatastructs.rs's dev-dependency graph (chacha20/rand_distr -> zerocopy 0.8.x) hits zerocopy's
# `stdarch_x86_avx512` unstable-feature gate on the pinned nightly snapshot when compiling the
# test suite's dev-deps (E0658) — a nightly-vs-transitive-dep trap unrelated to the sanitized
# fuzz build. `cargo +stable` is installed for exactly this in mayhem/Dockerfile. Keep this
# choice CONSISTENT with what build.sh would use if it ever built the oracle (it doesn't here —
# cargo test both compiles and runs, see header) so nothing silently rebuilds under a different
# toolchain between runs.
echo "=== running cargo +stable test --all-features (pdatastructs.rs's own unit tests + doctests) ==="
out="$(RUSTFLAGS="" cargo +stable test --all-features --no-fail-fast --jobs "$MAYHEM_JOBS" 2>&1)"; cargo_rc=$?
echo "$out"

CARGO_PASSED=0; CARGO_FAILED=0; CARGO_IGNORED=0
while read -r p f i; do
  CARGO_PASSED=$(( CARGO_PASSED + p )); CARGO_FAILED=$(( CARGO_FAILED + f )); CARGO_IGNORED=$(( CARGO_IGNORED + i ))
done < <(printf '%s\n' "$out" \
  | sed -n 's/^test result:.* \([0-9][0-9]*\) passed; \([0-9][0-9]*\) failed; \([0-9][0-9]*\) ignored.*/\1 \2 \3/p')

if [ "$(( CARGO_PASSED + CARGO_FAILED + CARGO_IGNORED ))" -eq 0 ]; then
  echo "could not parse any 'test result:' lines; using cargo exit code $cargo_rc" >&2
  if [ "$cargo_rc" -eq 0 ]; then CARGO_PASSED=1; CARGO_FAILED=0; else CARGO_PASSED=0; CARGO_FAILED=1; fi
fi

# ── 2. KAT probe (the sabotage-proof half of the oracle — see header) ──────────────────────
KAT_BIN="$SRC/kat"
[ -x "$KAT_BIN" ] || KAT_BIN="/mayhem/kat"
echo "=== running KAT probe: $KAT_BIN ==="
KAT_PASSED=0
KAT_FAILED=4   # unconditional: pessimistic until proven otherwise (never a silent skip)
if [ -x "$KAT_BIN" ]; then
  kat_out="$("$KAT_BIN" 2>&1)"; kat_rc=$?
  echo "$kat_out"
  markers_found=0
  for m in BLOOM_OK CMS_OK HLL_OK KAT_ALL_OK; do
    if printf '%s\n' "$kat_out" | grep -q "^${m}"; then
      markers_found=$(( markers_found + 1 ))
    else
      echo "KAT probe missing expected marker: $m" >&2
    fi
  done
  if [ "$kat_rc" -eq 0 ] && [ "$markers_found" -eq 4 ]; then
    KAT_PASSED=4; KAT_FAILED=0
  else
    echo "KAT probe FAILED (rc=$kat_rc, markers_found=$markers_found/4)" >&2
  fi
else
  echo "KAT probe binary not found at $KAT_BIN — build.sh should have produced it" >&2
fi

PASSED=$(( CARGO_PASSED + KAT_PASSED ))
FAILED=$(( CARGO_FAILED + KAT_FAILED ))
SKIPPED=$CARGO_IGNORED

emit_ctrf "cargo-test+kat" "$PASSED" "$FAILED" "$SKIPPED"
