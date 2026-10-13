//! `getLedgerEntries`: reads the current value of ledger entries.
//!
//! Spec: <https://developers.stellar.org/docs/data/apis/rpc/api-reference/methods/getLedgerEntries>

use serde::{Deserialize, Serialize};

use super::{Error, LedgerEntries, RemoteEntry, Result, RpcClient};
use crate::xdr::{
    LedgerEntry, LedgerEntryData, LedgerEntryExt, LedgerKey, Limits, ReadXdr, WriteXdr,
};

const METHOD: &str = "getLedgerEntries";

/// Maximum number of keys a single `getLedgerEntries` request may carry.
pub const MAX_KEYS_PER_REQUEST: usize = 200;

impl RpcClient {
    /// Fetches the current value of `keys`. Absent keys are omitted from the result.
    pub async fn get_ledger_entries(&self, keys: &[LedgerKey]) -> Result<LedgerEntries> {
        if keys.len() > MAX_KEYS_PER_REQUEST {
            return Err(Error::TooManyKeys(keys.len()));
        }
        let keys = keys
            .iter()
            .map(|key| key.to_xdr_base64(Limits::none()))
            .collect::<Result<Vec<_>, _>>()?;

        let result: RawResult = self.call(METHOD, Params { keys }).await?;
        result.decode()
    }
}

#[derive(Serialize)]
struct Params {
    /// Base64 `LedgerKey`s.
    keys: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawResult {
    latest_ledger: u32,
    #[serde(default)]
    entries: Vec<RawEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawEntry {
    key: String,
    /// Base64 `LedgerEntryData`, not a full `LedgerEntry`.
    xdr: String,
    last_modified_ledger_seq: u32,
    live_until_ledger_seq: Option<u32>,
}

impl RawResult {
    fn decode(self) -> Result<LedgerEntries> {
        let entries = self
            .entries
            .into_iter()
            .map(RawEntry::decode)
            .collect::<Result<_>>()?;
        Ok(LedgerEntries {
            latest_ledger: self.latest_ledger,
            entries,
        })
    }
}

impl RawEntry {
    fn decode(self) -> Result<RemoteEntry> {
        let key = LedgerKey::from_xdr_base64(&self.key, Limits::none())?;
        let data = LedgerEntryData::from_xdr_base64(&self.xdr, Limits::none())?;
        // Both discriminants are `LedgerEntryType`.
        if key.discriminant() != data.discriminant() {
            return Err(Error::InvalidResponse {
                method: METHOD,
                reason: format!("entry data does not match its key {}", self.key),
            });
        }
        Ok(RemoteEntry {
            key,
            entry: LedgerEntry {
                last_modified_ledger_seq: self.last_modified_ledger_seq,
                data,
                // The RPC does not return the extension (sponsorship). The Soroban host
                // does not read it.
                ext: LedgerEntryExt::V0,
            },
            live_until_ledger_seq: self.live_until_ledger_seq.filter(|&seq| seq != 0),
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::test_utils::{account_data, account_key, b64, server_returning};
    use crate::xdr::TtlEntry;

    #[tokio::test]
    async fn sends_base64_keys() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(body_partial_json(json!({
                "jsonrpc": "2.0",
                "method": "getLedgerEntries",
                "params": { "keys": [b64(&account_key(1))] },
            })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    json!({"jsonrpc": "2.0", "id": 1, "result": {"latestLedger": 7}}),
                ),
            )
            .expect(1)
            .mount(&server)
            .await;

        let result = RpcClient::new(server.uri())
            .get_ledger_entries(&[account_key(1)])
            .await
            .unwrap();

        assert_eq!(result.latest_ledger, 7);
        assert!(result.entries.is_empty());
    }

    #[tokio::test]
    async fn decodes_entries() {
        let server = server_returning(json!({
            "jsonrpc": "2.0", "id": 1,
            "result": {
                "latestLedger": 100,
                "entries": [{
                    "key": b64(&account_key(1)),
                    "xdr": b64(&account_data(1, 42)),
                    "lastModifiedLedgerSeq": 90,
                }],
            },
        }))
        .await;

        let result = RpcClient::new(server.uri())
            .get_ledger_entries(&[account_key(1), account_key(2)])
            .await
            .unwrap();

        assert_eq!(result.latest_ledger, 100);
        assert_eq!(
            result.entries,
            vec![RemoteEntry {
                key: account_key(1),
                entry: LedgerEntry {
                    last_modified_ledger_seq: 90,
                    data: account_data(1, 42),
                    ext: LedgerEntryExt::V0,
                },
                live_until_ledger_seq: None,
            }]
        );
    }

    #[tokio::test]
    async fn rejects_data_that_does_not_match_its_key() {
        let mismatched = LedgerEntryData::Ttl(TtlEntry {
            key_hash: Default::default(),
            live_until_ledger_seq: 1,
        });
        let server = server_returning(json!({
            "jsonrpc": "2.0", "id": 1,
            "result": {
                "latestLedger": 1,
                "entries": [{"key": b64(&account_key(1)), "xdr": b64(&mismatched), "lastModifiedLedgerSeq": 1}],
            },
        }))
        .await;

        let error = RpcClient::new(server.uri())
            .get_ledger_entries(&[account_key(1)])
            .await
            .unwrap_err();

        assert!(matches!(error, Error::InvalidResponse { .. }), "{error}");
    }

    #[tokio::test]
    async fn rejects_too_many_keys_without_calling_the_server() {
        let keys = vec![account_key(1); MAX_KEYS_PER_REQUEST + 1];

        let error = RpcClient::new("http://unused.invalid")
            .get_ledger_entries(&keys)
            .await
            .unwrap_err();

        assert!(matches!(error, Error::TooManyKeys(201)));
    }
}
