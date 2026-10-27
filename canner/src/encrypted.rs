use crate::{CannerError, encrypted::high_entropy::NONCE_SIZE};

mod high_entropy;
mod low_entropy;

pub use high_entropy::{HighEntropyCiphertext, high_entropy_decrypt, high_entropy_encrypt};
pub use low_entropy::{LowEntropyCiphertext, low_entropy_decrypt, low_entropy_encrypt};

pub enum Encrypted {
    Plaintext(Vec<u8>),
    LowEntropy(LowEntropyCiphertext),
    HighEntropy(HighEntropyCiphertext),
}

pub enum Encryption {
    Plaintext,
    HighEntropy,
    LowEntropy,
}

pub const CONV_KEY_SIZE: usize = 32;
pub type ConvergentKey = [u8; CONV_KEY_SIZE];

pub fn get_convergent_key(original: &[u8]) -> ConvergentKey {
    blake3::hash(original).into()
}

impl Encrypted {
    pub fn opener(&self) -> u8 {
        match self {
            Self::Plaintext(_) => 0,
            Self::LowEntropy(_) => 1,
            Self::HighEntropy(_) => 2,
        }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        let mut bytes = vec![self.opener()];
        match self {
            Self::Plaintext(plaintext) => {
                bytes.extend(plaintext);
            }
            Self::LowEntropy(ciphertext) => {
                bytes.extend(ciphertext.nonce);
                bytes.extend(ciphertext.ciphertext);
            }
            Self::HighEntropy(ciphertext) => {
                bytes.extend(ciphertext.0);
            }
        }
        bytes
    }

    pub fn try_from_bytes(mut bytes: Vec<u8>) -> Result<Self, CannerError> {
        let (contents, opener) = if bytes.len() >= 1 {
            let contents = bytes.split_off(1);
            let opener = bytes.get(0).cloned().unwrap();
            Ok((contents, opener))
        } else {
            Err(CannerError::OpenerByteNotFound)
        }?;
        Ok(match opener {
            0 => Self::Plaintext(contents),
            1 => Self::LowEntropy({
                let mut contents = contents;
                (contents.len() >= NONCE_SIZE)
                    .then(|| {
                        // SAFETY: ifの条件式よりcontentsの長さはNONCE_SIZEと同じか等しい
                        let ciphertext = contents.split_off(NONCE_SIZE);
                        let nonce: [u8; NONCE_SIZE] = contents.try_into().unwrap();
                        let nonce = nonce.into();
                        LowEntropyCiphertext { nonce, ciphertext }
                    })
                    .ok_or(CannerError::NonceNotFound)?
            }),
            2 => Self::HighEntropy(HighEntropyCiphertext(contents)),
            _ => return Err(CannerError::UnknownEncryption(opener)),
        })
    }
}
