//! Hindsight Cloud API client for agent memory.
//!
//! Wraps the Hindsight HTTP API (retain, recall, reflect, bank management).
//! Used by both the bot (auto-retain/recall) and the MCP aggregator
//! (HindsightBackend for explicit agent tool calls).

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::MemoryError;

const DEFAULT_BASE_URL: &str = "https://api.hindsight.vectorize.io";
const RETAIN_TIMEOUT: Duration = Duration::from_secs(10);
const RECALL_TIMEOUT: Duration = Duration::from_secs(5);
const REFLECT_TIMEOUT: Duration = Duration::from_secs(15);

/// A single recall result from Hindsight.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RecallResult {
    pub text: String,
    #[serde(default)]
    pub score: Option<f64>,
    #[serde(rename = "type", default)]
    pub fact_type: Option<String>,
}

/// Response from the recall endpoint.
#[derive(Debug, Deserialize)]
pub struct RecallResponse {
    pub results: Vec<RecallResult>,
}

/// Response from the retain endpoint.
#[derive(Debug, Deserialize)]
pub struct RetainResponse {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub operation_id: Option<String>,
}

/// Response from the reflect endpoint.
#[derive(Debug, Deserialize)]
pub struct ReflectResponse {
    pub text: String,
}

/// Bank profile (returned by GET /profile, auto-creates if absent).
#[derive(Debug, Deserialize)]
pub struct BankProfile {
    pub bank_id: String,
    #[serde(default)]
    pub name: Option<String>,
}

/// Retain request item.
#[derive(Debug, Serialize)]
struct RetainItem {
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<String>,
}

/// Retain request body.
#[derive(Debug, Serialize)]
struct RetainRequest {
    items: Vec<RetainItem>,
    #[serde(rename = "async")]
    is_async: bool,
}

/// Recall request body.
#[derive(Debug, Serialize)]
struct RecallRequest {
    query: String,
    budget: String,
    max_tokens: u32,
}

/// Reflect request body.
#[derive(Debug, Serialize)]
struct ReflectRequest {
    query: String,
    budget: String,
    max_tokens: u32,
}

/// Client for the Hindsight Cloud HTTP API.
#[derive(Clone)]
pub struct HindsightClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    bank_id: String,
    budget: String,
    max_tokens: u32,
}

impl HindsightClient {
    /// Create a new client.
    ///
    /// - `api_key`: Hindsight API key (e.g. `hs_...`)
    /// - `bank_id`: Memory bank identifier (typically agent name)
    /// - `budget`: Recall budget level ("low", "mid", "high")
    /// - `max_tokens`: Max tokens for recall results
    /// - `base_url`: Override for testing; `None` uses Hindsight Cloud
    pub fn new(
        api_key: &str,
        bank_id: &str,
        budget: &str,
        max_tokens: u32,
        base_url: Option<&str>,
    ) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url
                .unwrap_or(DEFAULT_BASE_URL)
                .trim_end_matches('/')
                .to_owned(),
            api_key: api_key.to_owned(),
            bank_id: bank_id.to_owned(),
            budget: budget.to_owned(),
            max_tokens,
        }
    }
}
