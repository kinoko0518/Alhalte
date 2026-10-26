use crate::compressed::Compressed;

mod high_entropy;
mod low_entropy;

pub use high_entropy::{HighEntropyCiphertext, high_entropy_decrypt, high_entropy_encrypt};
pub use low_entropy::{LowEntropyCiphertext, low_entropy_decrypt, low_entropy_encrypt};

pub enum Tin {
    None(Compressed),
    LowEntropy(LowEntropyCiphertext),
    HighEntropy(HighEntropyCiphertext),
}
