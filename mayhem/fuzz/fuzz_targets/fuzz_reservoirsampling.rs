//! Fuzz target for `pdatastructs::reservoirsampling::ReservoirSampling`.
//!
//! The fuzzer input chooses the reservoir size `k`, the RNG seed and the stream length, so every
//! input is a deterministic, self-contained run (no state is kept between inputs). It streams the
//! distinct values `0..n` through the sampler (crossing all three phases of `add`: initial fill,
//! normal reservoir sampling and fast skipping) and checks that:
//!   - `i()` equals the number of values seen and `is_empty()` matches,
//!   - the reservoir holds exactly `min(i, k)` values, all distinct and all from the stream,
//!   - while `i <= k` the reservoir is exactly the stream so far,
//!   - `clear()` resets the sampler.
#![no_main]

use std::collections::HashSet;

use libfuzzer_sys::{fuzz_target, Corpus};
use pdatastructs::rand::SeedableRng;
use pdatastructs::rand::rngs::StdRng;
use pdatastructs::reservoirsampling::ReservoirSampling;

fuzz_target!(|data: &[u8]| -> Corpus {
    if data.len() < 11 {
        return Corpus::Reject;
    }

    // k > 0 is the constructor's documented precondition.
    let k = 1 + (data[0] as usize % 64); // 1..64
    let seed = u64::from_le_bytes(data[1..9].try_into().unwrap());
    let n = u16::from_le_bytes([data[9], data[10]]) as usize; // 0..65535 values

    let mut rs: ReservoirSampling<usize, StdRng> =
        ReservoirSampling::new(k, StdRng::seed_from_u64(seed));
    assert!(rs.is_empty(), "fresh sampler must be empty");
    assert_eq!(rs.k(), k);

    for v in 0..n {
        rs.add(v);
        assert_eq!(rs.i(), v + 1, "i() does not count the added values");
        if v < k {
            let expected: Vec<usize> = (0..=v).collect();
            assert_eq!(rs.reservoir(), &expected, "initial fill must keep the stream as is");
        }
    }

    assert_eq!(rs.is_empty(), n == 0);
    let res = rs.reservoir();
    assert_eq!(res.len(), n.min(k), "reservoir size must be min(i, k)");
    let uniq: HashSet<&usize> = res.iter().collect();
    assert_eq!(uniq.len(), res.len(), "reservoir holds a value twice: {res:?}");
    assert!(res.iter().all(|&x| x < n), "reservoir holds a value never added: {res:?}");

    rs.clear();
    assert!(rs.is_empty(), "sampler not empty after clear()");
    assert_eq!(rs.i(), 0);
    assert!(rs.reservoir().is_empty(), "reservoir not empty after clear()");

    Corpus::Keep
});
