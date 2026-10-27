//! Immutable evidence captured for a particular synthesized performance.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// How an identity value was obtained.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSource {
    /// SHA-256 computed from bytes opened locally.
    LocalHash,
    /// A value observed in the loaded local implementation or its metadata.
    LocalObservation,
    /// A remote synthesis server's declaration, without independent verification.
    ServerDeclaration,
    /// No identity was available.
    #[default]
    Unknown,
}

/// A bounded identity claim and its source; absence remains explicit.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct IdentityEvidence {
    /// Identifier, version or SHA-256; absent when unknown.
    pub value: Option<String>,
    /// Evidence behind this value.
    pub source: EvidenceSource,
}

impl IdentityEvidence {
    /// Retain a bounded declaration; invalid or absent strings remain unknown.
    pub fn new(value: Option<&str>, source: EvidenceSource) -> Self {
        value
            .filter(|v| !v.trim().is_empty() && v.len() <= 4096)
            .map_or_else(Self::default, |v| Self {
                value: Some(v.into()),
                source,
            })
    }
}

/// A synthesis component, identified independently of a connection configuration.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SynthesisComponent {
    /// Stable backend-defined component identifier.
    pub id: IdentityEvidence,
    /// Version reported for this component.
    pub version: IdentityEvidence,
    /// Build identifier, when available.
    pub build: IdentityEvidence,
    /// Model artifact hash, when available; never a connection-file hash.
    pub sha256: IdentityEvidence,
}

/// Components used for a particular synthesis, captured anew for each render.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SynthesisIdentity {
    /// Backend kind, such as voicevox, diffsinger or leapsinger.
    pub backend: String,
    /// Engine identity and build.
    pub engine: SynthesisComponent,
    /// Acoustic model identity.
    pub model: SynthesisComponent,
    /// Actual selected speaker identity.
    pub speaker: SynthesisComponent,
    /// Actual selected decoder style identity.
    pub style: SynthesisComponent,
    /// Additional loaded model components, such as the vocoder.
    pub components: BTreeMap<String, SynthesisComponent>,
}

/// Whether a backend consumes the supplied seed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SynthesisSeedSupport {
    /// The implementation supplies the seed to its stochastic inference.
    Supported,
    /// The protocol does not provide a synthesis seed parameter.
    Unsupported,
    /// The implementation did not report seed support.
    #[default]
    Unknown,
}

/// Evidence belonging exclusively to one generated take, never refreshed on project load.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SynthesisProvenance {
    /// Provenance schema version, currently one.
    pub schema_version: u32,
    /// Engine, model, speaker and style observed for this render.
    pub identity: SynthesisIdentity,
    /// Entry configuration bytes captured when loading; separate from model hashes.
    pub configuration_sha256: IdentityEvidence,
    /// Bounded synthesis settings, excluding endpoints, credentials and input text.
    pub settings: BTreeMap<String, String>,
    /// Requested seed.
    pub seed: u64,
    /// Whether the backend consumes the seed.
    pub seed_support: SynthesisSeedSupport,
    /// SHA-256 of serialized frames, score, speaker, seed and native sample rate.
    pub input_sha256: String,
    /// SHA-256 of mono float32 little-endian samples at the declared native sample rate.
    pub output_audio_sha256: String,
    /// SHA-256 of the encoded WAV actually published by the session.
    pub output_wav_sha256: Option<String>,
}
