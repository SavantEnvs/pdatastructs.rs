//! Fuzz target for `pdatastructs::hyperloglog::HyperLogLog`.
//!
//! HyperLogLog is an APPROXIMATE cardinality estimator, so its `count()` has no single exact
//! expected value to differential-check against arbitrary input (that would make the harness
//! itself flaky). Instead this drives an arbitrary sequence of `add()`s (byte-string elements
//! derived directly from the fuzzer input, no file I/O) and asserts the EXACT, hash-independent
//! invariants the data structure must always uphold:
//!   - a fresh HyperLogLog is empty and reports count() == 0.
//!   - merging any HyperLogLog with a genuinely EMPTY one (same b/hasher) must leave its
//!     registers, and therefore its count(), UNCHANGED (merge takes a pointwise max against an
//!     all-zero register set).
//!   - clear() resets it back to empty (count() == 0, is_empty() == true).
//! Any panic reachable from add/merge/clear/count on arbitrary input is itself a finding.
#![no_main]

use libfuzzer_sys::fuzz_target;
use pdatastructs::hyperloglog::HyperLogLog;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    // b must be in [4, 18] (constructor panics otherwise) — pick a cheap subrange.
    let b = 4 + (data[0] as usize % 11); // 4..14

    let mut hll: HyperLogLog<Vec<u8>> = HyperLogLog::new(b);
    assert!(hll.is_empty(), "fresh HyperLogLog must be empty");
    assert_eq!(hll.count(), 0, "fresh HyperLogLog must count 0");

    let mut rest = &data[1..];
    while rest.len() >= 1 {
        let len = 1 + (rest[0] as usize % 32); // 1..32 byte elements
        rest = &rest[1..];
        if rest.len() < len {
            break;
        }
        let elem = rest[..len].to_vec();
        rest = &rest[len..];
        hll.add(&elem);
    }

    let count_before = hll.count();

    // Merging with a fresh, empty HLL (same b => same register count, same default hasher type)
    // must be a no-op on the registers, hence on count().
    let empty: HyperLogLog<Vec<u8>> = HyperLogLog::new(b);
    hll.merge(&empty);
    assert_eq!(
        hll.count(),
        count_before,
        "merging with an empty HyperLogLog must not change count()"
    );

    hll.clear();
    assert!(hll.is_empty(), "HyperLogLog must be empty after clear()");
    assert_eq!(hll.count(), 0, "HyperLogLog must count 0 after clear()");
});
