//! Fuzz target for `pdatastructs::countminsketch::CountMinSketch`.
//!
//! Drives an arbitrary sequence of `add_n` calls (byte-string keys + small counts, derived
//! directly from the fuzzer input, no file I/O) against a CountMinSketch while tracking the EXACT
//! counts in a HashMap, then differentially checks the sketch's fundamental correctness property:
//! a Count-Min Sketch NEVER undercounts (`query_point(x) >= real_count(x)` always holds; it may
//! only ever overestimate due to hash collisions). A violation is a genuine bug in the sketch's
//! add/query logic.
#![no_main]

use std::collections::HashMap;

use libfuzzer_sys::fuzz_target;
use pdatastructs::countminsketch::CountMinSketch;

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }

    // Keep w/d in a sane, cheap-to-allocate range; w==0 is an uninteresting parameter-validation
    // gap (division-by-zero in the hash addressing), not the code path this target exercises.
    let w = 8 + (data[0] as usize % 256); // 8..264 columns
    let d = 1 + (data[1] as usize % 8); // 1..8 rows

    let mut cms: CountMinSketch<Vec<u8>, u32> = CountMinSketch::with_params(w, d);
    let mut actual: HashMap<Vec<u8>, u32> = HashMap::new();

    let mut rest = &data[2..];
    while rest.len() >= 2 {
        let len = 1 + (rest[0] as usize % 32); // 1..32 byte keys
        let n = 1 + (rest[1] as u32 % 64); // 1..64 per-step increment
        rest = &rest[2..];
        if rest.len() < len {
            break;
        }
        let key = rest[..len].to_vec();
        rest = &rest[len..];

        cms.add_n(&key, &n);
        let real = {
            let e = actual.entry(key.clone()).or_insert(0);
            *e = e.saturating_add(n);
            *e
        };

        let est = cms.query_point(&key);
        assert!(
            est >= real,
            "count-min sketch undercounted: key={key:?} estimate={est} real={real}"
        );
    }
});
