#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiDocs {
    /// A caller-facing statement of what the item is for.
    pub summary: String,
    /// Non-obvious caller constraints; empty is allowed when the signature is
    /// already the complete contract.
    pub contract: String,
    /// A short XSH fragment when spelling or result handling benefits from it.
    pub example: Option<String>,
    /// Retrieval vocabulary for API search.
    pub tags: Vec<String>,
}
