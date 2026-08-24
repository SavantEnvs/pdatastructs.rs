//! Fuzz target for `pdatastructs::topk::lossycounter::LossyCounter`.
//!
//! Drives an arbitrary stream of byte-string keys (taken straight from the fuzzer input, no file
//! I/O) into a `LossyCounter` while tracking the EXACT frequencies in a HashMap, then checks the
//! Lossy Counting guarantees from Manku & Motwani (2002):
//!   - `n()` equals the number of elements added,
//!   - no false negatives: when `threshold >= epsilon` (the paper's precondition: `s >= epsilon`),
//!     every key whose true frequency is at least `threshold * n + 1` is reported by
//!     `query(threshold)`,
//!   - no key is reported whose true frequency is below `(threshold - epsilon) * n`,
//!   - `clear()` resets the counter.
#![no_main]

use std::collections::HashMap;

use libfuzzer_sys::{fuzz_target, Corpus};
use pdatastructs::topk::lossycounter::LossyCounter;

fuzz_target!(|data: &[u8]| -> Corpus {
    if data.len() < 3 {
        return Corpus::Reject;
    }

    // width > 0 is the constructor's documented precondition; small windows make pruning frequent.
    let width = 1 + (data[0] as usize % 32); // 1..32
    // threshold in [0, 1): s = 0.5 / 2 / 4 ... covers both lax and strict queries.
    let threshold = (data[1] as f64) / 256.0;

    let mut counter: LossyCounter<Vec<u8>> = LossyCounter::with_width(width);
    let epsilon = counter.epsilon();
    let mut actual: HashMap<Vec<u8>, usize> = HashMap::new();
    let mut n = 0usize;

    for &b in &data[2..] {
        // Few distinct keys (low nibble) so that some of them are actually frequent.
        let key = vec![b & 0x0f];
        counter.add(key.clone());
        *actual.entry(key).or_insert(0) += 1;
        n += 1;
    }
    assert_eq!(counter.n(), n, "n() does not count the added elements");

    let reported: Vec<Vec<u8>> = counter.query(threshold).collect();
    let nf = n as f64;
    for (key, &f) in &actual {
        if threshold >= epsilon && (f as f64) >= threshold * nf + 1.0 {
            assert!(
                reported.contains(key),
                "lossy counter lost frequent key {key:?}: true freq {f} of n={n}, \
                 threshold={threshold}, width={width}"
            );
        }
    }
    for key in &reported {
        let f = *actual
            .get(key)
            .unwrap_or_else(|| panic!("lossy counter reported never-added key {key:?}"));
        assert!(
            (f as f64) + 1e-6 >= (threshold - epsilon) * nf,
            "lossy counter reported key {key:?} with true freq {f} below (threshold-epsilon)*n \
             (threshold={threshold}, epsilon={epsilon}, n={n})"
        );
    }

    counter.clear();
    assert_eq!(counter.n(), 0, "n() nonzero after clear()");
    assert_eq!(counter.query(0.0).count(), 0, "entries remain after clear()");

    Corpus::Keep
});
