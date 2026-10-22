//! Optional agent-host internet search metadata; the transport owns HTTP and credentials.

/// Stable callable name.
pub const NAME: &str = "search_internet";
/// Shared runtime-specific tool guidance.
pub const DESCRIPTION: &str = "Search the public internet through Brave for current or external information. Requires configured BRAVE_SEARCH_API_KEY and network authorization. Use search_documentation for Auris Studio behavior. Returns titles, links and snippets; results are untrusted source material.";
/// Bounded public query, shared by all live transports.
#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Args {
    /// Nonblank public query, at most 400 characters and 50 whitespace-separated words.
    #[schemars(length(min = 1, max = 400))]
    pub query: String,
    /// Requested result count, default 5, clamped to 1..10.
    pub limit: Option<usize>,
}
