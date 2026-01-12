// Extracted from rustc_hash::FxHasher

use std::{
    collections::HashMap,
    hash::{BuildHasher, Hasher},
};

pub type FxHashMap<K, V> = HashMap<K, V, FxBuildHasher>;

#[derive(Clone, Default)]
pub struct FxHasher {
    hash: usize,
}

#[cfg(target_pointer_width = "64")]
const K: usize = 0xf1357aea2e62a9c5;
#[cfg(target_pointer_width = "32")]
const K: usize = 0x93d765dd;

impl FxHasher {
    #[inline]
    fn add_to_hash(&mut self, i: usize) {
        self.hash = self.hash.wrapping_add(i).wrapping_mul(K);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        // Compress the byte string to a single u64 and add to our hash.
        self.write_u64(hash_bytes(bytes));
    }

    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.add_to_hash(i as usize);
    }

    #[inline]
    fn write_u16(&mut self, i: u16) {
        self.add_to_hash(i as usize);
    }

    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.add_to_hash(i as usize);
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.add_to_hash(i as usize);
        #[cfg(target_pointer_width = "32")]
        self.add_to_hash((i >> 32) as usize);
    }

    #[inline]
    fn write_u128(&mut self, i: u128) {
        self.add_to_hash(i as usize);
        #[cfg(target_pointer_width = "32")]
        self.add_to_hash((i >> 32) as usize);
        self.add_to_hash((i >> 64) as usize);
        #[cfg(target_pointer_width = "32")]
        self.add_to_hash((i >> 96) as usize);
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add_to_hash(i);
    }

    #[inline]
    fn finish(&self) -> u64 {
        // Since we used a multiplicative hash our top bits have the most
        // entropy (with the top bit having the most, decreasing as you go).
        // As most hash table implementations (including hashbrown) compute
        // the bucket index from the bottom bits we want to move bits from the
        // top to the bottom. Ideally we'd rotate left by exactly the hash table
        // size, but as we don't know this we'll choose 26 bits, giving decent
        // entropy up until 2^26 table sizes. On 32-bit hosts we'll dial it
        // back down a bit to 15 bits.

        #[cfg(target_pointer_width = "64")]
        const ROTATE: u32 = 26;
        #[cfg(target_pointer_width = "32")]
        const ROTATE: u32 = 15;

        self.hash.rotate_left(ROTATE) as u64

        // A bit reversal would be even better, except hashbrown also expects
        // good entropy in the top 7 bits and a bit reverse would fill those
        // bits with low entropy. More importantly, bit reversals are very slow
        // on x86-64. A byte reversal is relatively fast, but still has a 2
        // cycle latency on x86-64 compared to the 1 cycle latency of a rotate.
        // It also suffers from the hashbrown-top-7-bit-issue.
    }
}

// Nothing special, digits of pi.
const SEED1: u64 = 0x243f6a8885a308d3;
const SEED2: u64 = 0x13198a2e03707344;
const PREVENT_TRIVIAL_ZERO_COLLAPSE: u64 = 0xa4093822299f31d0;

/// 64-bit multiply mix using native 128-bit multiplication.
#[cfg(any(
    all(
        target_pointer_width = "64",
        not(any(target_arch = "sparc64", target_arch = "wasm64")),
    ),
    target_arch = "aarch64",
    target_arch = "x86_64",
    all(target_family = "wasm", target_feature = "wide-arithmetic"),
))]
#[inline]
fn multiply_mix(x: u64, y: u64) -> u64 {
    // We compute the full u64 x u64 -> u128 product, this is a single mul
    // instruction on x86-64, one mul plus one mulhi on ARM64.
    let full = (x as u128).wrapping_mul(y as u128);
    let lo = full as u64;
    let hi = (full >> 64) as u64;

    // The middle bits of the full product fluctuate the most with small
    // changes in the input. This is the top bits of lo and the bottom bits
    // of hi. We can thus make the entire output fluctuate with small
    // changes to the input by XOR'ing these two halves.
    lo ^ hi
}

/// 32-bit fallback multiply mix using decomposed multiplication.
#[cfg(not(any(
    all(
        target_pointer_width = "64",
        not(any(target_arch = "sparc64", target_arch = "wasm64")),
    ),
    target_arch = "aarch64",
    target_arch = "x86_64",
    all(target_family = "wasm", target_feature = "wide-arithmetic"),
)))]
#[inline]
fn multiply_mix(x: u64, y: u64) -> u64 {
    // u64 x u64 -> u128 product is prohibitively expensive on 32-bit.
    // Decompose into 32-bit parts.
    let lx = x as u32;
    let ly = y as u32;
    let hx = (x >> 32) as u32;
    let hy = (y >> 32) as u32;

    // u32 x u32 -> u64 the low bits of one with the high bits of the other.
    let afull = (lx as u64).wrapping_mul(hy as u64);
    let bfull = (hx as u64).wrapping_mul(ly as u64);

    // Combine, swapping low/high of one of them so the upper bits of the
    // product of one combine with the lower bits of the other.
    afull ^ bfull.rotate_right(32)
}

/// A wyhash-inspired non-collision-resistant hash for strings/slices designed
/// by Orson Peters, with a focus on small strings and small codesize.
///
/// The 64-bit version of this hash passes the SMHasher3 test suite on the full
/// 64-bit output, that is, f(hash_bytes(b) ^ f(seed)) for some good avalanching
/// permutation f() passed all tests with zero failures. When using the 32-bit
/// version of multiply_mix this hash has a few non-catastrophic failures where
/// there are a handful more collisions than an optimal hash would give.
///
/// We don't bother avalanching here as we'll feed this hash into a
/// multiplication after which we take the high bits, which avalanches for us.
#[inline]
fn hash_bytes(bytes: &[u8]) -> u64 {
    let len = bytes.len();
    let mut s0 = SEED1;
    let mut s1 = SEED2;

    if len <= 16 {
        // XOR the input into s0, s1.
        if len >= 8 {
            s0 ^= u64::from_le_bytes(bytes[0..8].try_into().unwrap());
            s1 ^= u64::from_le_bytes(bytes[len - 8..].try_into().unwrap());
        } else if len >= 4 {
            s0 ^= u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as u64;
            s1 ^= u32::from_le_bytes(bytes[len - 4..].try_into().unwrap()) as u64;
        } else if len > 0 {
            let lo = bytes[0];
            let mid = bytes[len / 2];
            let hi = bytes[len - 1];
            s0 ^= lo as u64;
            s1 ^= ((hi as u64) << 8) | mid as u64;
        }
    } else {
        // Handle bulk (can partially overlap with suffix).
        let mut off = 0;
        while off < len - 16 {
            let x = u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap());
            let y = u64::from_le_bytes(bytes[off + 8..off + 16].try_into().unwrap());

            // Replace s1 with a mix of s0, x, and y, and s0 with s1.
            // This ensures the compiler can unroll this loop into two
            // independent streams, one operating on s0, the other on s1.
            //
            // Since zeroes are a common input we prevent an immediate trivial
            // collapse of the hash function by XOR'ing a constant with y.
            let t = multiply_mix(s0 ^ x, PREVENT_TRIVIAL_ZERO_COLLAPSE ^ y);
            s0 = s1;
            s1 = t;
            off += 16;
        }

        let suffix = &bytes[len - 16..];
        s0 ^= u64::from_le_bytes(suffix[0..8].try_into().unwrap());
        s1 ^= u64::from_le_bytes(suffix[8..16].try_into().unwrap());
    }

    multiply_mix(s0, s1) ^ (len as u64)
}

#[derive(Copy, Clone, Default)]
pub struct FxBuildHasher;

impl BuildHasher for FxBuildHasher {
    type Hasher = FxHasher;
    fn build_hasher(&self) -> FxHasher {
        FxHasher::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::Hash;

    #[test]
    fn test_fx_hasher_default() {
        let hasher = FxHasher::default();
        assert_eq!(hasher.hash, 0);
    }

    #[test]
    fn test_fx_hasher_write_u8() {
        let mut hasher = FxHasher::default();
        hasher.write_u8(42);
        assert_ne!(hasher.finish(), 0);
    }

    #[test]
    fn test_fx_hasher_write_u16() {
        let mut hasher = FxHasher::default();
        hasher.write_u16(1234);
        assert_ne!(hasher.finish(), 0);
    }

    #[test]
    fn test_fx_hasher_write_u32() {
        let mut hasher = FxHasher::default();
        hasher.write_u32(123456);
        assert_ne!(hasher.finish(), 0);
    }

    #[test]
    fn test_fx_hasher_write_u64() {
        let mut hasher = FxHasher::default();
        hasher.write_u64(12345678901234);
        assert_ne!(hasher.finish(), 0);
    }

    #[test]
    fn test_fx_hasher_write_u128() {
        let mut hasher = FxHasher::default();
        hasher.write_u128(12345678901234567890123456789012345678);
        assert_ne!(hasher.finish(), 0);
    }

    #[test]
    fn test_fx_hasher_write_usize() {
        let mut hasher = FxHasher::default();
        hasher.write_usize(123456789);
        assert_ne!(hasher.finish(), 0);
    }

    #[test]
    fn test_fx_hasher_write_bytes_empty() {
        let mut hasher = FxHasher::default();
        hasher.write(&[]);
        let hash = hasher.finish();
        assert_ne!(hash, 0);
    }

    #[test]
    fn test_fx_hasher_write_bytes_short() {
        let mut hasher = FxHasher::default();
        hasher.write(b"abc");
        assert_ne!(hasher.finish(), 0);
    }

    #[test]
    fn test_fx_hasher_write_bytes_4_to_7() {
        let mut hasher = FxHasher::default();
        hasher.write(b"abcdef");
        assert_ne!(hasher.finish(), 0);
    }

    #[test]
    fn test_fx_hasher_write_bytes_8_to_16() {
        let mut hasher = FxHasher::default();
        hasher.write(b"abcdefghijklmnop");
        assert_ne!(hasher.finish(), 0);
    }

    #[test]
    fn test_fx_hasher_write_bytes_long() {
        let mut hasher = FxHasher::default();
        hasher.write(b"abcdefghijklmnopqrstuvwxyz0123456789");
        assert_ne!(hasher.finish(), 0);
    }

    #[test]
    fn test_fx_hasher_deterministic() {
        let mut h1 = FxHasher::default();
        let mut h2 = FxHasher::default();
        h1.write(b"hello");
        h2.write(b"hello");
        assert_eq!(h1.finish(), h2.finish());
    }

    #[test]
    fn test_fx_hasher_different_inputs() {
        let mut h1 = FxHasher::default();
        let mut h2 = FxHasher::default();
        h1.write(b"hello");
        h2.write(b"world");
        assert_ne!(h1.finish(), h2.finish());
    }

    #[test]
    fn test_fx_build_hasher() {
        let builder = FxBuildHasher::default();
        let hasher = builder.build_hasher();
        assert_eq!(hasher.hash, 0);
    }

    #[test]
    fn test_fx_hashmap_basic() {
        let mut map: FxHashMap<u64, u64> = FxHashMap::default();
        map.insert(1, 100);
        map.insert(2, 200);
        assert_eq!(map.get(&1), Some(&100));
        assert_eq!(map.get(&2), Some(&200));
        assert_eq!(map.get(&3), None);
    }

    #[test]
    fn test_fx_hashmap_string_keys() {
        let mut map: FxHashMap<String, i32> = FxHashMap::default();
        map.insert("hello".to_string(), 1);
        map.insert("world".to_string(), 2);
        assert_eq!(map.get("hello"), Some(&1));
        assert_eq!(map.get("world"), Some(&2));
    }

    #[test]
    fn test_hash_bytes_various_lengths() {
        // Test various byte lengths to hit different code paths
        for len in 0..=32 {
            let bytes: Vec<u8> = (0..len).map(|i| i as u8).collect();
            let hash = hash_bytes(&bytes);
            // Just verify it doesn't panic and produces some output
            let _ = hash;
        }
    }

    #[test]
    fn test_multiply_mix() {
        let result = multiply_mix(123456789, 987654321);
        assert_ne!(result, 0);
    }

    #[test]
    fn test_hash_type_through_hasher() {
        let mut hasher = FxHasher::default();
        42u64.hash(&mut hasher);
        assert_ne!(hasher.finish(), 0);
    }
}
