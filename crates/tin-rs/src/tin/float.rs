use half::f16;

use crate::TinError;
use crate::tin::TinContent;
use crate::tin::label::{LABEL_PRIMITIVE_F16, LABEL_PRIMITIVE_F32, LABEL_PRIMITIVE_F64};

impl Into<TinContent> for f32 {
    fn into(self) -> TinContent {
        TinContent::new(LABEL_PRIMITIVE_F32, self.to_le_bytes().to_vec())
    }
}

impl TryInto<f32> for TinContent {
    type Error = TinError;

    fn try_into(self) -> Result<f32, Self::Error> {
        if self.label != LABEL_PRIMITIVE_F32 {
            return Err(TinError::LabelMismatch {
                expected: LABEL_PRIMITIVE_F32,
                actual: self.label,
            });
        }
        if self.payload.len() != 4 {
            return Err(TinError::InvalidPayloadLength {
                expected: 4,
                actual: self.payload.len(),
            });
        }
        let mut buf = [0u8; 4];
        buf.copy_from_slice(&self.payload);
        Ok(f32::from_le_bytes(buf))
    }
}

impl From<f16> for TinContent {
    fn from(value: f16) -> Self {
        Self::new(LABEL_PRIMITIVE_F16, value.to_le_bytes().to_vec())
    }
}

impl TryInto<f16> for TinContent {
    type Error = TinError;

    fn try_into(self) -> Result<f16, Self::Error> {
        if self.label != LABEL_PRIMITIVE_F16 {
            return Err(TinError::LabelMismatch {
                expected: LABEL_PRIMITIVE_F16,
                actual: self.label,
            });
        }
        if self.payload.len() != 2 {
            return Err(TinError::InvalidPayloadLength {
                expected: 2,
                actual: self.payload.len(),
            });
        }
        let mut buf = [0u8; 2];
        buf.copy_from_slice(&self.payload);
        Ok(half::f16::from_le_bytes(buf))
    }
}

impl From<f64> for TinContent {
    fn from(value: f64) -> Self {
        Self::new(LABEL_PRIMITIVE_F64, value.to_le_bytes().to_vec())
    }
}

impl TryInto<f64> for TinContent {
    type Error = TinError;

    fn try_into(self) -> Result<f64, Self::Error> {
        if self.label != LABEL_PRIMITIVE_F64 {
            return Err(TinError::LabelMismatch {
                expected: LABEL_PRIMITIVE_F64,
                actual: self.label,
            });
        }
        if self.payload.len() != 8 {
            return Err(TinError::InvalidPayloadLength {
                expected: 8,
                actual: self.payload.len(),
            });
        }
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&self.payload);
        Ok(f64::from_le_bytes(buf))
    }
}
