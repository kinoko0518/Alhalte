use crate::{
    LABEL_ALHALTE_TIN_ID, LABEL_PRIMITIVE_BOOL, TinError, TinId,
    tin::{TinContent, label::LABEL_PRIMITIVE_STRING},
};

impl From<String> for TinContent {
    fn from(value: String) -> Self {
        Self::new(LABEL_PRIMITIVE_STRING, value.into_bytes())
    }
}

impl From<bool> for TinContent {
    fn from(value: bool) -> Self {
        Self::new(LABEL_PRIMITIVE_BOOL, vec![if value { 1 } else { 0 }])
    }
}

impl TryInto<bool> for TinContent {
    type Error = TinError;

    fn try_into(self) -> Result<bool, Self::Error> {
        if self.label != LABEL_PRIMITIVE_BOOL {
            return Err(TinError::LabelMismatch {
                expected: LABEL_PRIMITIVE_BOOL,
                actual: self.label,
            });
        }
        if self.payload.len() != 1 {
            return Err(TinError::InvalidPayloadLength {
                expected: 1,
                actual: self.payload.len(),
            });
        }
        match self.payload[0] {
            0 => Ok(false),
            1 => Ok(true),
            b => Err(TinError::InvalidBool(b)),
        }
    }
}

impl TryInto<String> for TinContent {
    type Error = TinError;

    fn try_into(self) -> Result<String, Self::Error> {
        if self.label != LABEL_PRIMITIVE_STRING {
            return Err(TinError::LabelMismatch {
                expected: LABEL_PRIMITIVE_STRING,
                actual: self.label,
            });
        }
        String::from_utf8(self.payload).map_err(|err| TinError::InvalidUtf8(err))
    }
}

impl Into<TinContent> for TinId {
    fn into(self) -> TinContent {
        TinContent::new(LABEL_ALHALTE_TIN_ID, self.to_vec())
    }
}

impl TryFrom<TinContent> for TinId {
    type Error = TinError;

    fn try_from(value: TinContent) -> Result<Self, Self::Error> {
        if value.label != LABEL_ALHALTE_TIN_ID {
            return Err(TinError::LabelMismatch {
                expected: LABEL_ALHALTE_TIN_ID,
                actual: value.label,
            });
        }
        if value.payload.len() != 32 {
            return Err(TinError::InvalidPayloadLength {
                expected: 32,
                actual: value.payload.len(),
            });
        }
        let mut buf = [0u8; 32];
        buf.copy_from_slice(&value.payload);
        Ok(buf)
    }
}
