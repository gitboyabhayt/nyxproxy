//! Tiny client for the NyxProxy backend AI gateway.

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub temperature: f64,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatChoice {
    pub message: ChatMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub provider: String,
    pub model: String,
    pub choices: Vec<ChatChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeResponse {
    pub provider: String,
    pub model: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub name: String,
    pub available: bool,
    pub default_model: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvidersResponse {
    pub default: String,
    pub providers: Vec<ProviderInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRequestPayload {
    pub method: String,
    pub url: String,
    pub http_version: String,
    pub headers: std::collections::HashMap<String, String>,
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResponsePayload {
    pub status: u16,
    pub http_version: String,
    pub headers: std::collections::HashMap<String, String>,
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeRequestBody {
    pub request: HttpRequestPayload,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<HttpResponsePayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadRequestBody {
    pub request: HttpRequestPayload,
    pub parameter: String,
    pub attack_type: String,
    pub count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

#[derive(Clone)]
pub struct AiClient {
    client: Client,
    base_url: String,
    token: Option<String>,
}

impl AiClient {
    pub fn new(base_url: String, token: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .user_agent("NyxProxy-Desktop/0.1")
            .build()
            .unwrap_or_default();
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
        }
    }

    pub async fn chat(&self, req: ChatRequest) -> Result<ChatResponse> {
        let url = format!("{}/v1/chat/completions", self.base_url);
        let response = self
            .auth(self.client.post(&url))
            .json(&req)
            .send()
            .await?
            .error_for_status()?
            .json::<ChatResponse>()
            .await?;
        if response.choices.is_empty() {
            return Err(anyhow!("empty choices"));
        }
        Ok(response)
    }

    pub async fn analyze_request(&self, body: AnalyzeRequestBody) -> Result<AnalyzeResponse> {
        self.post_analyze("/v1/analyze/request", body).await
    }

    pub async fn find_vulns(&self, body: AnalyzeRequestBody) -> Result<AnalyzeResponse> {
        self.post_analyze("/v1/analyze/vulns", body).await
    }

    pub async fn generate_payloads(&self, body: PayloadRequestBody) -> Result<AnalyzeResponse> {
        let url = format!("{}/v1/analyze/payloads", self.base_url);
        Ok(self
            .auth(self.client.post(&url))
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json::<AnalyzeResponse>()
            .await?)
    }

    pub async fn providers(&self) -> Result<ProvidersResponse> {
        let url = format!("{}/v1/providers", self.base_url);
        Ok(self
            .auth(self.client.get(&url))
            .send()
            .await?
            .error_for_status()?
            .json::<ProvidersResponse>()
            .await?)
    }

    async fn post_analyze(
        &self,
        path: &str,
        body: AnalyzeRequestBody,
    ) -> Result<AnalyzeResponse> {
        let url = format!("{}{}", self.base_url, path);
        Ok(self
            .auth(self.client.post(&url))
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json::<AnalyzeResponse>()
            .await?)
    }

    fn auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(token) = &self.token {
            req.bearer_auth(token)
        } else {
            req
        }
    }
}
