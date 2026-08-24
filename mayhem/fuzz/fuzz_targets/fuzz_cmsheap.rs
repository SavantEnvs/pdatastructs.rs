//! Fuzz target for `pdatastructs::topk::cmsheap::CMSHeap`.
//!
//! Drives an arbitrary stream of byte-string keys (taken straight from the fuzzer input, no file
//! I/O) into a Top-K `CMSHeap` backed by a small CountMinSketch, tracking the distinct keys in a
//! HashSet, and checks the invariants the structure must always uphold:
//!   - it never remembers more than `k` entries, and exactly `min(k, distinct keys seen)` of them
//!     (new keys always fit while there is room, and an eviction replaces one entry with one),
//!   - every remembered entry is a key that was actually added, and no entry is reported twice,
//!   - `is_empty()` is false once anything was added, and `clear()` resets it.
#![no_main]

use std::collections::HashSet;

use libfuzzer_sys::{fuzz_target, Corpus};
use pdatastructs::countminsketch::CountMinSketch;
use pdatastructs::topk::cmsheap::CMSHeap;

fuzz_target!(|data: &[u8]| -> Corpus {
    if data.len() < 4 {
        return Corpus::Reject;
    }

    // k > 0, w > 0 and d > 0 are the constructors' documented preconditions.
    let k = 1 + (data[0] as usize % 16); // 1..16 remembered entries
    let w = 4 + (data[1] as usize % 61); // 4..64 columns: small enough that keys collide
    let d = 1 + (data[2] as usize % 4); // 1..4 rows

    let mut heap: CMSHeap<Vec<u8>> = CMSHeap::new(k, CountMinSketch::with_params(w, d));
    assert!(heap.is_empty(), "fresh CMSHeap must be empty");
    assert_eq!(heap.k(), k);

    let mut seen: HashSet<Vec<u8>> = HashSet::new();
    let mut rest = &data[3..];
    while !rest.is_empty() {
        let len = 1 + (rest[0] as usize % 8); // 1..8 byte keys
        rest = &rest[1..];
        let len = len.min(rest.len());
        if len == 0 {
            break;
        }
        let key = rest[..len].to_vec();
        rest = &rest[len..];

        heap.add(key.clone());
        seen.insert(key);

        assert!(!heap.is_empty(), "CMSHeap empty after add");
        let top: Vec<Vec<u8>> = heap.iter().collect();
        assert_eq!(
            top.len(),
            k.min(seen.len()),
            "CMSHeap holds {} entries, expected min(k={k}, distinct={})",
            top.len(),
            seen.len()
        );
        let uniq: HashSet<&Vec<u8>> = top.iter().collect();
        assert_eq!(uniq.len(), top.len(), "CMSHeap reported a key twice: {top:?}");
        for t in &top {
            assert!(seen.contains(t), "CMSHeap reported never-added key {t:?}");
        }
    }

    heap.clear();
    assert!(heap.is_empty(), "CMSHeap not empty after clear()");
    assert_eq!(heap.iter().count(), 0, "CMSHeap iterates entries after clear()");

    Corpus::Keep
});
