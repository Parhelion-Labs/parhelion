use serde::de::DeserializeOwned;
use serde::Serialize;

use super::jsonrpc::{Request, Response};
use super::{Error, Result};

/// JSON-RPC client for a remote Stellar RPC server.
///
/// Each RPC method is implemented in its own file in this module.
#[derive(Debug, Clone)]
pub struct RpcClient {
    http: reqwest::Client,
    url: String,
}

impl RpcClient {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            url: url.into(),
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub(super) async fn call<P, R>(&self, method: &'static str, params: P) -> Result<R>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        let request = Request {
            jsonrpc: "2.0",
            // One request per HTTP call, so the id never needs to tell responses apart.
            id: 1,
            method,
            params,
        };
        let response: Response<R> = self
            .http
            .post(&self.url)
            .json(&request)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|source| Error::Http { method, source })?
            .json()
            .await
            .map_err(|source| Error::Http { method, source })?;

        match (response.result, response.error) {
            (_, Some(error)) => Err(Error::Rpc {
                method,
                code: error.code,
                message: error.message,
            }),
            (Some(result), None) => Ok(result),
            (None, None) => Err(Error::InvalidResponse {
                method,
                reason: "neither `result` nor `error` is present".into(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::test_utils::server_returning;

    #[tokio::test]
    async fn surfaces_json_rpc_errors() {
        let server = server_returning(json!({
            "jsonrpc": "2.0", "id": 1,
            "error": {"code": -32602, "message": "invalid key"},
        }))
        .await;

        let error = RpcClient::new(server.uri())
            .call::<_, serde_json::Value>("anyMethod", json!({}))
            .await
            .unwrap_err();

        assert!(matches!(error, Error::Rpc { code: -32602, .. }), "{error}");
    }

    #[tokio::test]
    async fn rejects_a_response_without_result_or_error() {
        let server = server_returning(json!({"jsonrpc": "2.0", "id": 1})).await;

        let error = RpcClient::new(server.uri())
            .call::<_, serde_json::Value>("anyMethod", json!({}))
            .await
            .unwrap_err();

        assert!(matches!(error, Error::InvalidResponse { .. }), "{error}");
    }
}
