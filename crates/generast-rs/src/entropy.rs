//! Entropy traits and seed-based ChaCha20 implementation.

use rand_chacha::ChaCha20Rng;
use rand_core::{RngCore, SeedableRng};

use crate::error::EntropyError;

/// Integer alias (`u64`) for face counts and roll results.
pub type Integer = u64;

/// Abstraction for dice entropy sources.
pub trait Entropy {
    /// Takes a single roll from a die with `faces` faces, returning a value in `1..=faces`.
    ///
    /// Returns [`EntropyError::ZeroFaces`] if `faces == 0`.
    fn take(&mut self, faces: Integer) -> Result<Integer, EntropyError>;
}

/// Deterministic entropy source initialized from a 256-bit seed using ChaCha20.
///
/// Rolls are drawn from ChaCha20 via `next_u64` using unbiased rejection sampling.
#[derive(Debug, Clone)]
pub struct SeededEntropy {
    rng: ChaCha20Rng,
}

impl SeededEntropy {
    /// Creates a new `SeededEntropy` initialized with a 256-bit (32-byte) seed.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self {
            rng: ChaCha20Rng::from_seed(seed),
        }
    }
}

impl Entropy for SeededEntropy {
    fn take(&mut self, faces: Integer) -> Result<Integer, EntropyError> {
        if faces == 0 {
            return Err(EntropyError::ZeroFaces);
        }
        let range = faces as u128;
        let max_multiple = (1u128 << 64) / range * range;
        loop {
            let r = self.rng.next_u64() as u128;
            if r < max_multiple {
                return Ok(((r % range) as u64) + 1);
            }
        }
    }
}
