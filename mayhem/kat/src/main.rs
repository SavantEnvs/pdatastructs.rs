//! mayhem/kat — known-answer probe for `mayhem/test.sh`'s behavioral oracle.
//!
//! This is a plain, dynamically linked binary (NOT `cargo test` — see the note in
//! `mayhem/test.sh` on why a statically-linked test harness is not a sufficient sole oracle).
//! It runs three fixed scenarios against pdatastructs's main data structures with known inputs,
//! asserts EXACT expected values, and prints one `<NAME>_OK ...` marker line per scenario on
//! success. A broken/no-op/stubbed-out implementation fails an assertion (panic -> nonzero exit,
//! no marker printed) instead of silently "passing"; `test.sh` requires all three markers to be
//! present verbatim.
//!
//! `pdatastructs`'s data structures use `BuildHasherDefault<DefaultHasher>` by default, and
//! `std::collections::hash_map::DefaultHasher` is seeded with FIXED constants (not randomized per
//! process, unlike `RandomState`) — confirmed by pdatastructs's own unit tests
//! (`hash_utils.rs::tests::hash_iter_builder_f` asserts two independently constructed default
//! hashers agree). So these exact-value assertions are reproducible run over run and rebuild over
//! rebuild for a fixed toolchain, not flaky.

use pdatastructs::countminsketch::CountMinSketch;
use pdatastructs::filters::Filter;
use pdatastructs::filters::bloomfilter::BloomFilter;
use pdatastructs::hyperloglog::HyperLogLog;

fn kat_bloomfilter() {
    // m, k chosen generously (200_000 bits, 8 hash functions) for only 5 inserted keys, so the
    // false-positive probability on a held-out key is astronomically small (~ (5/200000)^8) —
    // this is a genuine behavioral check, not a coin flip.
    let mut filter: BloomFilter<&str> = BloomFilter::with_params(200_000, 8);
    let inserted = ["alpha", "bravo", "charlie", "delta", "echo"];
    for k in inserted {
        filter.insert(&k).unwrap();
    }
    for k in inserted {
        assert!(filter.query(&k), "KAT FAIL: bloom filter lost key {k:?}");
    }
    let held_out = "zulu-never-inserted-marker-000";
    assert!(
        !filter.query(&held_out),
        "KAT FAIL: bloom filter false-positived on held-out key {held_out:?}"
    );
    println!("BLOOM_OK 5/5 inserted keys found, held-out key correctly absent");
}

fn kat_countminsketch() {
    // w, d chosen generously (200_000 columns, 5 rows) for only 3 keys, so every row is
    // collision-free with overwhelming probability -> exact counts, not just the lower bound.
    let mut cms: CountMinSketch<&str, u32> = CountMinSketch::with_params(200_000, 5);
    let ops: [(&str, u32); 3] = [("alpha", 3), ("bravo", 7), ("charlie", 1)];
    for (k, n) in ops {
        cms.add_n(&k, &n);
    }
    for (k, n) in ops {
        let got = cms.query_point(&k);
        assert_eq!(got, n, "KAT FAIL: count-min sketch counted {k:?} as {got}, expected exactly {n}");
    }
    println!("CMS_OK counts exact: alpha=3 bravo=7 charlie=1");
}

fn kat_hyperloglog() {
    let b = 10; // 1024 registers
    let mut hll: HyperLogLog<String> = HyperLogLog::new(b);
    assert_eq!(hll.count(), 0, "KAT FAIL: fresh HyperLogLog must count 0");
    assert!(hll.is_empty(), "KAT FAIL: fresh HyperLogLog must be empty");

    let n = 500usize;
    for i in 0..n {
        hll.add(&format!("item-{i}"));
    }
    let count = hll.count();
    // Standard error for b=10 is ~1.04/sqrt(1024) ~= 3.25%; use a generous 20% band so this isn't
    // sensitive to minor algorithmic variation, while still catching a broken/stubbed estimator
    // (e.g. one that always returns 0 or n unconditionally would fail this and the checks below).
    let lo = (n as f64 * 0.8) as usize;
    let hi = (n as f64 * 1.2) as usize;
    assert!(
        count >= lo && count <= hi,
        "KAT FAIL: HyperLogLog estimated {count} distinct elements, expected within [{lo}, {hi}] of {n}"
    );

    // Merging with a fresh, empty HLL (same b/hasher) must be a pointwise-max-with-zero no-op.
    let empty: HyperLogLog<String> = HyperLogLog::new(b);
    hll.merge(&empty);
    assert_eq!(
        hll.count(),
        count,
        "KAT FAIL: merging with an empty HyperLogLog changed count()"
    );

    hll.clear();
    assert!(hll.is_empty(), "KAT FAIL: HyperLogLog not empty after clear()");
    assert_eq!(hll.count(), 0, "KAT FAIL: HyperLogLog counts nonzero after clear()");

    println!("HLL_OK count={count} within [{lo},{hi}] of n={n}, merge/clear invariants hold");
}

fn main() {
    kat_bloomfilter();
    kat_countminsketch();
    kat_hyperloglog();
    println!("KAT_ALL_OK");
}
