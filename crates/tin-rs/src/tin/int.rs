use crate::TinError;
use crate::tin::TinContent;
use crate::tin::label::{
    LABEL_PRIMITIVE_I8, LABEL_PRIMITIVE_I16, LABEL_PRIMITIVE_I32, LABEL_PRIMITIVE_I64,
    LABEL_PRIMITIVE_I128, LABEL_PRIMITIVE_U8, LABEL_PRIMITIVE_U16, LABEL_PRIMITIVE_U32,
    LABEL_PRIMITIVE_U64, LABEL_PRIMITIVE_U128,
};

macro_rules! impl_int_codec {
    ($type:ty, $from_fn:ident, $as_fn:ident, $label:ident, $size:expr) => {
        #[doc = concat!("Creates a new `TinContent` containing a `", stringify!($type), "` value encoded as little-endian.")]
        pub fn $from_fn(val: $type) -> Self {
            Self::new($label, val.to_le_bytes().to_vec())
        }

        #[doc = concat!("Decodes the payload as a little-endian `", stringify!($type), "` value.")]
        pub fn $as_fn(&self) -> Result<$type, TinError> {
            if self.label != $label {
                return Err(TinError::LabelMismatch {
                    expected: $label,
                    actual: self.label,
                });
            }
            if self.payload.len() != $size {
                return Err(TinError::InvalidPayloadLength {
                    expected: $size,
                    actual: self.payload.len(),
                });
            }
            let mut buf = [0u8; $size];
            buf.copy_from_slice(&self.payload);
            Ok(<$type>::from_le_bytes(buf))
        }
    };
}

impl TinContent {
    impl_int_codec!(u8, from_u8, as_u8, LABEL_PRIMITIVE_U8, 1);
    impl_int_codec!(u16, from_u16, as_u16, LABEL_PRIMITIVE_U16, 2);
    impl_int_codec!(u32, from_u32, as_u32, LABEL_PRIMITIVE_U32, 4);
    impl_int_codec!(u64, from_u64, as_u64, LABEL_PRIMITIVE_U64, 8);
    impl_int_codec!(u128, from_u128, as_u128, LABEL_PRIMITIVE_U128, 16);

    impl_int_codec!(i8, from_i8, as_i8, LABEL_PRIMITIVE_I8, 1);
    impl_int_codec!(i16, from_i16, as_i16, LABEL_PRIMITIVE_I16, 2);
    impl_int_codec!(i32, from_i32, as_i32, LABEL_PRIMITIVE_I32, 4);
    impl_int_codec!(i64, from_i64, as_i64, LABEL_PRIMITIVE_I64, 8);
    impl_int_codec!(i128, from_i128, as_i128, LABEL_PRIMITIVE_I128, 16);
}
