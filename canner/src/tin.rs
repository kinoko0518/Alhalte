use crate::{CellarError, compressed::Compressed, tin::high_entropy::NONCE_SIZE};

mod high_entropy;
mod low_entropy;

pub use high_entropy::{HighEntropyCiphertext, high_entropy_decrypt, high_entropy_encrypt};
pub use low_entropy::{LowEntropyCiphertext, low_entropy_decrypt, low_entropy_encrypt};

pub enum Tin {
    Plaintext(Compressed),
    LowEntropy(LowEntropyCiphertext),
    HighEntropy(HighEntropyCiphertext),
}

impl Tin {
    pub fn opener(&self) -> u8 {
        match self {
            Self::Plaintext(_) => 0,
            Self::LowEntropy(_) => 1,
            Self::HighEntropy(_) => 2,
        }
    }

    pub fn to_bytes(self) -> Vec<u8> {
        let mut bytes = vec![self.opener()];
        match self {
            Self::Plaintext(compressed) => {
                bytes.extend(compressed.into_bytes());
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

    pub fn from_bytes(mut bytes: Vec<u8>) -> Result<Self, CellarError> {
        let (contents, opener) = if bytes.len() >= 1 {
            let contents = bytes.split_off(1);
            let opener = bytes.get(0).cloned().unwrap();
            Ok((contents, opener))
        } else {
            Err(CellarError::OpenerByteNotFound)
        }?;
        Ok(match opener {
            0 => Self::Plaintext(Compressed::try_from_bytes(contents)?),
            1 => Self::LowEntropy({
                let mut contents = contents;
                if contents.len() >= NONCE_SIZE {
                    // SAFETY: ifの条件式よりcontentsの長さはNONCE_SIZEと同じか等しい
                    let ciphertext = contents.split_off(NONCE_SIZE);
                    let nonce: [u8; NONCE_SIZE] = contents.try_into().unwrap();
                    let nonce = nonce.into();
                    Ok(LowEntropyCiphertext { nonce, ciphertext })
                } else {
                    Err(CellarError::NonceNotFound)
                }?
            }),
            2 => Self::HighEntropy(HighEntropyCiphertext(contents)),
            _ => return Err(CellarError::UnknownEncryption(opener)),
        })
    }
}
