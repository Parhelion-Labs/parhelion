//! `getLatestLedger`: the ledger the remote closed last.
//!
//! Spec: <https://developers.stellar.org/docs/data/apis/rpc/api-reference/methods/getLatestLedger>

use serde::{Deserialize, Serialize};

use super::{Error, LatestLedger, Result, RpcClient};

const METHOD: &str = "getLatestLedger";

impl RpcClient {
    pub async fn get_latest_ledger(&self) -> Result<LatestLedger> {
        let raw: RawResult = self.call(METHOD, Params {}).await?;
        let close_time = raw.close_time.parse().map_err(|_| Error::InvalidResponse {
            method: METHOD,
            reason: format!("closeTime {:?} is not a number", raw.close_time),
        })?;
        Ok(LatestLedger {
            sequence: raw.sequence,
            protocol_version: raw.protocol_version,
            close_time,
        })
    }
}

#[derive(Serialize)]
struct Params {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawResult {
    sequence: u32,
    protocol_version: u32,
    /// Unix seconds, sent as a string.
    close_time: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::test_utils::server_returning;

    #[tokio::test]
    async fn decodes_the_latest_ledger() {
        let server = server_returning(json!({
            "jsonrpc": "2.0", "id": 1,
            "result": {
                "id": "666f65",
                "protocolVersion": 29,
                "sequence": 65162349,
                "closeTime": "1793097528",
            },
        }))
        .await;

        let latest = RpcClient::new(server.uri())
            .get_latest_ledger()
            .await
            .unwrap();

        assert_eq!(
            latest,
            LatestLedger {
                sequence: 65162349,
                protocol_version: 29,
                close_time: 1793097528,
            }
        );
    }

    #[tokio::test]
    async fn rejects_a_close_time_that_is_not_a_number() {
        let server = server_returning(json!({
            "jsonrpc": "2.0", "id": 1,
            "result": {"protocolVersion": 29, "sequence": 1, "closeTime": "soon"},
        }))
        .await;

        let error = RpcClient::new(server.uri())
            .get_latest_ledger()
            .await
            .unwrap_err();

        assert!(matches!(error, Error::InvalidResponse { .. }), "{error}");
    }
}
