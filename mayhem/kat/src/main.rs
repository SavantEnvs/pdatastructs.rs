//! mayhem/kat — known-answer probe for `mayhem/test.sh`'s behavioral oracle.
//!
//! This is a plain, dynamically linked binary (NOT `cargo test` — see the note in
//! `mayhem/test.sh` on why a statically-linked test harness is not a sufficient sole oracle).
//! It runs fixed scenarios against pdatastructs's main data structures with known inputs,
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
use pdatastructs::rand::SeedableRng;
use pdatastructs::rand::rngs::StdRng;
use pdatastructs::reservoirsampling::ReservoirSampling;
use pdatastructs::topk::cmsheap::CMSHeap;
use pdatastructs::topk::lossycounter::LossyCounter;

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

fn kat_cmsheap() {
    // Huge sketch, so counts are exact: top-2 of a:5, b:3, c:1 must be exactly {a, b}.
    let mut heap: CMSHeap<&str> = CMSHeap::new(2, CountMinSketch::with_params(200_000, 5));
    for k in ["a", "b", "a", "c", "a", "b", "a", "b", "a"] {
        heap.add(k);
    }
    let mut top: Vec<&str> = heap.iter().collect();
    top.sort();
    assert_eq!(top, ["a", "b"], "KAT FAIL: CMSHeap top-2 was {top:?}, expected [a, b]");
    heap.clear();
    assert!(heap.is_empty(), "KAT FAIL: CMSHeap not empty after clear()");
    println!("CMSHEAP_OK top-2 of a:5 b:3 c:1 is exactly {{a, b}}");
}

fn kat_lossycounter() {
    // width 4 (epsilon 0.25): "x" is 8 of 12 elements, every other key is a one-off.
    let mut c: LossyCounter<&str> = LossyCounter::with_width(4);
    let stream = ["x", "y", "x", "x", "z", "x", "x", "w", "x", "x", "v", "x"];
    for k in stream {
        c.add(k);
    }
    assert_eq!(c.n(), 12, "KAT FAIL: LossyCounter n() != 12");
    let hot: Vec<&str> = c.query(0.5).collect();
    assert_eq!(hot, ["x"], "KAT FAIL: LossyCounter query(0.5) was {hot:?}, expected [x]");
    c.clear();
    assert_eq!(c.n(), 0, "KAT FAIL: LossyCounter n() != 0 after clear()");
    println!("LOSSY_OK query(0.5) over 12 elements is exactly [x]");
}

fn kat_reservoirsampling() {
    let mut rs: ReservoirSampling<u32, StdRng> = ReservoirSampling::new(5, StdRng::seed_from_u64(1));
    for v in 0..3u32 {
        rs.add(v);
    }
    assert_eq!(rs.reservoir(), &vec![0, 1, 2], "KAT FAIL: reservoir must hold the stream while i<=k");
    for v in 3..1000u32 {
        rs.add(v);
    }
    assert_eq!(rs.i(), 1000, "KAT FAIL: i() != 1000");
    assert_eq!(rs.reservoir().len(), 5, "KAT FAIL: reservoir size != k");
    assert!(rs.reservoir().iter().all(|&v| v < 1000), "KAT FAIL: reservoir holds a never-added value");
    rs.clear();
    assert!(rs.is_empty() && rs.reservoir().is_empty(), "KAT FAIL: sampler not empty after clear()");
    println!("RESERVOIR_OK size 5 of 1000 values, initial fill exact, clear() resets");
}

fn main() {
    kat_bloomfilter();
    kat_countminsketch();
    kat_hyperloglog();
    kat_cmsheap();
    kat_lossycounter();
    kat_reservoirsampling();
    println!("KAT_ALL_OK");
}
