use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Source {
    pub id: String,
    pub source: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Summary,
    Decisions,
    Tasks,
    Analysis,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Step {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub prompt: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub steps: Vec<Step>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Evidence {
    pub source_id: String,
    pub quote: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Fact {
    pub text: String,
    #[serde(deserialize_with = "nullable")]
    pub owner: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub due: Option<String>,
    pub evidence: Vec<Evidence>,
}
fn nullable<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Answer {
    pub(crate) items: Vec<Fact>,
}
/// Why a model statement was discarded. Only the reason and a count are stored:
/// rejected model text never enters a result, so nothing unverified is shown as a note.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "camelCase")]
pub enum RejectionReason {
    /// Empty or overlong statement text.
    InvalidText,
    /// No evidence or more than six evidence entries.
    EvidenceCount,
    /// A quote is not a literal 8–600 character excerpt of an existing source.
    QuoteNotLiteral,
    /// Owner or due date on a section that is not a task list.
    OwnerOrDueOutsideTasks,
    /// Owner or due date is not literally contained in one of its quotes.
    UnverifiedOwnerOrDue,
    /// The dedicated name-candidate rules are not met.
    InvalidNameCandidate,
    /// A newly generated numeric token is absent from its literal quotes.
    NumberNotQuoted,
    /// Every cited passage explicitly marks this proposed task as nonbinding or negated.
    NonbindingTask,
    /// A valid statement beyond the per-answer limit of 20.
    ItemLimit,
}
pub type Rejections = BTreeMap<RejectionReason, usize>;
/// A screened model answer and the durable per-batch checkpoint format.
/// Every kept item has passed the literal source check; `rejected` only counts discarded items.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckedAnswer {
    pub items: Vec<Fact>,
    /// Absent in checkpoints written before 1.0.3, whose strict check never kept a partial answer.
    #[serde(default)]
    pub rejected: Rejections,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub items: Vec<Fact>,
    /// Discarded model statements per reason (result schema 2). Schema 1 results never
    /// contained partial answers, so the field is absent there and read as empty.
    #[serde(default)]
    pub rejected: Rejections,
}
/// Stored result format. 1: up to 1.0.2 (any invalid statement failed the whole analysis).
/// 2: invalid statements are discarded and counted per section.
pub const RESULT_SCHEMA: u32 = 2;
fn legacy_result_schema() -> u32 {
    1
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub id: String,
    pub meeting_id: String,
    pub source_hash: String,
    pub template: Template,
    pub sections: Vec<Section>,
    pub sources: Vec<Source>,
    pub created_at: String,
    pub model: String,
    pub mode: String,
    pub endpoint: String,
    pub prompt_version: String,
    #[serde(default = "legacy_result_schema")]
    pub result_schema: u32,
}

/// Immutable request identity and verified progress; no credentials or source text.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingAnalysis {
    pub id: String,
    pub meeting_id: String,
    pub source_hash: String,
    pub template: Template,
    pub created_at: String,
    pub model: String,
    pub mode: String,
    pub endpoint: String,
    pub confirmed_tasks: usize,
    pub total_tasks: usize,
    /// Prompt version stored with the run snapshot.
    pub prompt_version: String,
    /// Only runs of the current prompt version may be continued; others stay read-only.
    pub resumable: bool,
}

/// Deliberately not serializable: an API key must never enter a stored run snapshot.
pub struct Connection {
    pub endpoint: String,
    pub model: String,
    pub mode: String,
    pub cloud_consent: bool,
    pub api_key: String,
}
impl Drop for Connection {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.api_key.zeroize();
    }
}
