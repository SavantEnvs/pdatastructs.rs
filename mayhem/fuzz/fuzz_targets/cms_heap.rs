//! Fuzz target for `pdatastructs::topk::cmsheap::CMSHeap` — a reconstruction of the
//! mayhemheroes/pdatastructs.rs `cms-heap` target (the fork repo is gone from GitHub, so the
//! original source cannot be copied; see mayhem/Mayhemfile_cms_heap).
//!
//! The shape and the constants below are recovered from the DWARF of the ORIGINAL fuzz binary
//! (`/cms_heap` in ghcr.io/mayhemheroes/pdatastructs.rs@sha256:759e77f2…, the image run 16 was
//! built from):
//!   * `CMSHeap<u8>` / `TreeEntry<u8>` / `BuildHasherDefault<DefaultHasher>` in the type names,
//!     and `fuzz_targets/cms_heap.rs` as the only harness compilation unit;
//!   * the inlined `with_params` stores `w = 28`, `d = 2` (a 28*2*8 = 448-byte zeroed table
//!     allocation), which is exactly what `with_point_query_properties(0.1, 0.2)` computes —
//!     `ceil(e/0.1) = 28`, `ceil(ln(1/0.2)) = 2` — the crate's own doc-example parameters;
//!   * `k = 2` for the reservoir, the doc example's value.
//!
//! The bug: `CMSHeap::add` assumes that an object that is new to its reservoir, while the
//! reservoir still has room, must have a CountMinSketch count of exactly 1
//! (`debug_assert!(count == 1)`, cmsheap.rs:184). A Count-Min Sketch only guarantees that it
//! never UNDERcounts — it may legitimately OVERcount a brand-new key that collides with an
//! already-seen one in every row. With w = 28, d = 2 over the 256-value `u8` domain such a
//! double collision is reachable, and the assertion then fires inside the crate's own code.
#![no_main]

use libfuzzer_sys::fuzz_target;
use pdatastructs::countminsketch::CountMinSketch;
use pdatastructs::topk::cmsheap::CMSHeap;

fuzz_target!(|data: &[u8]| {
    let epsilon = 0.1;
    let delta = 0.2;
    let cms: CountMinSketch<u8> = CountMinSketch::with_point_query_properties(epsilon, delta);
    let mut tk = CMSHeap::new(2, cms);

    for b in data {
        tk.add(*b);
    }
});
