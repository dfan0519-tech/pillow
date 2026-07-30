use serde::{Deserialize, Deserializer, Serialize};

use super::types::ArgumentsDigest;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct ToolProposalFingerprint(String);

impl ToolProposalFingerprint {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(super) fn from_digest(value: &ArgumentsDigest) -> Self {
        Self(value.as_str().to_owned())
    }
}

impl<'de> Deserialize<'de> for ToolProposalFingerprint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        let digest = ArgumentsDigest::parse(value).map_err(serde::de::Error::custom)?;
        Ok(Self::from_digest(&digest))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct ToolCallReservationKey(String);

impl ToolCallReservationKey {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(super) fn from_digest(value: &ArgumentsDigest) -> Self {
        Self(value.as_str().to_owned())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCallReservationDecision {
    Unreserved,
    Replay,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// An identity-keyed reservation claim for one validated proposal.
///
/// The key contains only project, conversation, run, and call identity. The
/// fingerprint contains the complete validated proposal semantics.
pub struct ToolCallReservation {
    key: ToolCallReservationKey,
    fingerprint: ToolProposalFingerprint,
}

impl ToolCallReservation {
    pub(super) fn new(key: ToolCallReservationKey, fingerprint: ToolProposalFingerprint) -> Self {
        Self { key, fingerprint }
    }

    pub fn key(&self) -> &ToolCallReservationKey {
        &self.key
    }

    pub fn fingerprint(&self) -> &ToolProposalFingerprint {
        &self.fingerprint
    }

    /// Classifies the fingerprint stored under this call identity.
    ///
    /// The executor must perform lookup and insertion atomically: store this
    /// fingerprint for `Unreserved`, return the prior receipt for `Replay`, and
    /// reject `Conflict`. This protocol crate deliberately owns no durable store.
    pub fn classify(
        &self,
        stored_fingerprint: Option<&ToolProposalFingerprint>,
    ) -> ToolCallReservationDecision {
        match stored_fingerprint {
            None => ToolCallReservationDecision::Unreserved,
            Some(stored) if stored == &self.fingerprint => ToolCallReservationDecision::Replay,
            Some(_) => ToolCallReservationDecision::Conflict,
        }
    }
}
