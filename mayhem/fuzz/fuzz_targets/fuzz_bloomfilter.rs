//! Fuzz target for `pdatastructs::filters::bloomfilter::BloomFilter`.
//!
//! Drives an arbitrary sequence of insert/query operations (byte-string keys derived straight
//! from the fuzzer input, no file I/O) against a single BloomFilter and asserts the ONE property
//! a Bloom filter must always uphold: zero false negatives. If a key was ever inserted, every
//! later `query()` for that exact key MUST return `true` — a violation is a real correctness bug
//! in the hashing/bit-addressing logic, not a probabilistic false positive (those are allowed and
//! not asserted against).
#![no_main]

use std::collections::HashSet;

use libfuzzer_sys::fuzz_target;
use pdatastructs::filters::Filter;
use pdatastructs::filters::bloomfilter::BloomFilter;

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }

    // Keep m/k in a sane, cheap-to-allocate range; the constructor panics/UB with m==0, which is
    // an uninteresting parameter-validation gap, not the code path this target exercises.
    let m = 64 + (data[0] as usize % 512); // 64..576 bits
    let k = 1 + (data[1] as usize % 8); // 1..8 hash functions

    let mut filter: BloomFilter<Vec<u8>> = BloomFilter::with_params(m, k);
    let mut inserted: HashSet<Vec<u8>> = HashSet::new();

    let mut rest = &data[2..];
    while rest.len() >= 2 {
        let op = rest[0];
        let len = 1 + (rest[1] as usize % 32); // 1..32 byte keys
        rest = &rest[2..];
        if rest.len() < len {
            break;
        }
        let key = rest[..len].to_vec();
        rest = &rest[len..];

        if op % 2 == 0 {
            filter.insert(&key).unwrap();
            inserted.insert(key.clone());
            assert!(
                filter.query(&key),
                "false negative: key {key:?} was just inserted but query() returned false"
            );
        } else {
            let found = filter.query(&key);
            if inserted.contains(&key) {
                assert!(
                    found,
                    "false negative: previously-inserted key {key:?} no longer found"
                );
            }
            // found == false or found == true is both fine for a never-inserted key
            // (false positives are an accepted property of the data structure).
        }
    }
});
