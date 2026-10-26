pub type BlobID = blake3::Hash;

pub struct Blob {
    bytes: Vec<u8>,
}

impl Blob {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    pub fn from_bytes(t: u64, bytes: Vec<u8>) -> Self {
        let mut contents = t.to_le_bytes().to_vec();
        contents.extend(bytes.into_iter());

        Self { bytes: contents }
    }

    pub fn get_label(&self) -> u64 {
        (0..4).fold(0, |acc, i| {
            // リトルエンディアンとして値を取り出し
            acc | ((self.bytes.get(i).cloned().unwrap() as u64) << i * 8)
        })
    }

    pub fn deconstruct(mut self) -> (u64, Vec<u8>) {
        (self.get_label(), self.bytes.split_off(4))
    }

    pub fn id(&self) -> BlobID {
        blake3::hash(&self.bytes)
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}
