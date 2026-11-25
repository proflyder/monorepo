use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EntryView {
    pub(super) path: String,
    pub(super) description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) consumer: Option<String>,
    pub(super) live: bool,
    pub(super) in_catalog: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct RevealBody {
    pub(super) key: String,
    #[serde(default)]
    pub(super) version: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct UpdateBody {
    pub(super) key: String,
    pub(super) value: String,
    pub(super) cas: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct RollbackBody {
    pub(super) version: u64,
    pub(super) cas: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StepUpBody {
    pub(super) challenge: String,
    pub(super) signature: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ChallengeBody {
    pub(super) action: String,
    #[serde(default)]
    pub(super) path: Option<String>,
    #[serde(default)]
    pub(super) key: Option<String>,
    #[serde(default)]
    pub(super) version: Option<u64>,
    #[serde(default)]
    pub(super) cas: Option<u64>,
    #[serde(default)]
    pub(super) value_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct EnrollBody {
    pub(super) code: String,
    pub(super) public_key: String,
    pub(super) attestation_challenge: String,
    #[serde(default)]
    pub(super) attestation_chain: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DeleteQuery {
    pub(super) cas: u64,
    #[serde(default)]
    pub(super) version: Option<u64>,
}
