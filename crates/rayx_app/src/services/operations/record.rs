use std::sync::Arc;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use super::OperationError;

/// The largest encoded operation a [`DomainOperation`] accepts unless it opts out.
pub const MAX_OPERATION_PAYLOAD_BYTES: usize = 64 * 1024;

/// A target's id: what operations are submitted to (1 to 256 bytes, not blank).
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct OperationTargetId(String);

impl OperationTargetId {
    /// The id `value`, or [`OperationError::InvalidTargetId`] when it is blank or longer than 256
    /// bytes.
    pub fn new(value: impl Into<String>) -> Result<Self, OperationError> {
        let value = value.into();
        if value.trim().is_empty() || value.len() > 256 {
            return Err(OperationError::InvalidTargetId);
        }
        Ok(Self(value))
    }

    /// The id as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An operation of a domain: a serializable change a target applies, with its kind and version
/// on the wire.
pub trait DomainOperation: Clone + Serialize + DeserializeOwned + Send + Sync + 'static {
    const KIND: &'static str;
    const VERSION: u32;
    /// Maximum encoded payload for this domain. The default protects existing
    /// consumers; a domain owning larger authored documents may explicitly opt out.
    const MAX_PAYLOAD_BYTES: Option<usize> = Some(MAX_OPERATION_PAYLOAD_BYTES);
}

/// An operation as it crosses: its target, kind, version and the operation's JSON as bytes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OperationRecord {
    pub target_id: OperationTargetId,
    pub kind: String,
    pub version: u32,
    #[serde(
        serialize_with = "serialize_payload",
        deserialize_with = "deserialize_payload"
    )]
    payload: Arc<[u8]>,
}

impl OperationRecord {
    /// The record of `operation` for `target_id`, or [`OperationError::PayloadTooLarge`] past the
    /// operation's limit.
    pub fn from_operation<T: DomainOperation>(
        target_id: OperationTargetId,
        operation: &T,
    ) -> Result<Self, OperationError> {
        let payload = serde_json::to_vec(operation).map_err(|_| OperationError::Encode)?;
        if let Some(limit) = T::MAX_PAYLOAD_BYTES
            && payload.len() > limit
        {
            return Err(OperationError::PayloadTooLarge {
                size: payload.len(),
                limit,
            });
        }
        Ok(Self {
            target_id,
            kind: T::KIND.to_string(),
            version: T::VERSION,
            payload: payload.into(),
        })
    }

    /// The operation, when the record is of `T`'s kind and version.
    pub fn decode<T: DomainOperation>(&self) -> Result<T, OperationError> {
        if self.kind != T::KIND || self.version != T::VERSION {
            return Err(OperationError::UnsupportedOperation {
                kind: self.kind.clone(),
                version: self.version,
            });
        }
        serde_json::from_slice(&self.payload).map_err(|_| OperationError::Decode)
    }

    /// The encoded operation's length in bytes.
    pub fn payload_len(&self) -> usize {
        self.payload.len()
    }

    /// The encoded operation bytes, for identity comparison without decoding.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

// Preserve the existing byte-array wire representation while sharing immutable
// encoded payloads between receipts, journals, and read-only history snapshots.
fn serialize_payload<S: serde::Serializer>(
    payload: &Arc<[u8]>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    payload.as_ref().serialize(serializer)
}

fn deserialize_payload<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Arc<[u8]>, D::Error> {
    Vec::<u8>::deserialize(deserializer).map(Into::into)
}
