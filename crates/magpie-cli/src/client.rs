//! Thin blocking client for the local API. Knows HTTP, not CLI.

use std::time::Duration;

use magpie_config::connection::Connection;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

const CONNECT_TIMEOUT: Duration = Duration::from_millis(300);

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// Magpie is not running (or not reachable). Distinct so callers can say so plainly.
    #[error("cannot reach Magpie at {url}: {detail}")]
    Unreachable { url: String, detail: String },
    /// The API answered with an error; `message` is its human-readable reason.
    #[error("{message} (HTTP {status})")]
    Api { status: u16, message: String },
    #[error("unexpected response: {0}")]
    Protocol(String),
}

#[derive(Debug)]
pub struct Client {
    agent: ureq::Agent,
    conn: Connection,
}

impl Client {
    pub fn new(conn: Connection, timeout: Duration) -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(timeout))
            // Loopback connects in well under a millisecond when Magpie is up.
            // When it is down, Windows retries the SYN for ~2 s before reporting
            // "refused"; cap that so hooks don't stall the agent.
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .build()
            .new_agent();
        Self { agent, conn }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.conn.base_url)
    }

    fn auth(&self) -> String {
        format!("Bearer {}", self.conn.token)
    }

    pub fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, ClientError> {
        let mut req = self
            .agent
            .get(self.url(path))
            .header("Authorization", self.auth());
        for (k, v) in query {
            req = req.query(*k, v);
        }
        self.finish(req.call())
    }

    pub fn post<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ClientError> {
        let res = self
            .agent
            .post(self.url(path))
            .header("Authorization", self.auth())
            .send_json(body);
        self.finish(res)
    }

    pub fn patch<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ClientError> {
        let res = self
            .agent
            .patch(self.url(path))
            .header("Authorization", self.auth())
            .send_json(body);
        self.finish(res)
    }

    pub fn delete(&self, path: &str) -> Result<(), ClientError> {
        let res = self
            .agent
            .delete(self.url(path))
            .header("Authorization", self.auth())
            .call();
        self.finish::<Value>(res).map(drop)
    }

    fn finish<T: DeserializeOwned>(
        &self,
        res: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    ) -> Result<T, ClientError> {
        let mut res = res.map_err(|e| ClientError::Unreachable {
            url: self.conn.base_url.clone(),
            detail: e.to_string(),
        })?;
        let status = res.status().as_u16();
        let text = res
            .body_mut()
            .read_to_string()
            .map_err(|e| ClientError::Protocol(e.to_string()))?;
        if !(200..300).contains(&status) {
            return Err(ClientError::Api {
                status,
                message: api_message(&text),
            });
        }
        // 204 No Content: deserialize from `null` so `()`/`Value` callers work.
        let text = if text.is_empty() { "null" } else { &text };
        serde_json::from_str(text).map_err(|e| ClientError::Protocol(e.to_string()))
    }
}

/// Pulls `error.message` out of an API error body; falls back to the raw text.
fn api_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(str::to_owned))
        .unwrap_or_else(|| body.trim().chars().take(200).collect())
}

#[cfg(test)]
mod tests {
    use super::api_message;

    #[test]
    fn extracts_api_error_message() {
        assert_eq!(
            api_message(r#"{"error":{"code":"invalid","message":"title must not be empty"}}"#),
            "title must not be empty"
        );
        assert_eq!(api_message("Bad Gateway\n"), "Bad Gateway");
    }
}
