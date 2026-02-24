/// Serialize/deserialize `Option<Vec<u8>>` as CBOR byte strings.
pub(crate) mod opt_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(val: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match val {
            Some(bytes) => s.serialize_some(&serde_bytes::Bytes::new(bytes)),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        Option::<serde_bytes::ByteBuf>::deserialize(d).map(|o| o.map(|b| b.into_vec()))
    }
}
