//! `simulateTransaction`: the remote's own simulation, which the differential suite compares
//! Parhelion against.
//!
//! Spec: <https://developers.stellar.org/docs/data/apis/rpc/api-reference/methods/simulateTransaction>

use serde::{Deserialize, Serialize};

use super::{Error, HostFunctionResult, Result, RpcClient, SimulateTransactionResult};
use crate::xdr::{
    Limits, ReadXdr, ScVal, SorobanAuthorizationEntry, SorobanTransactionData, TransactionEnvelope,
    WriteXdr,
};

const METHOD: &str = "simulateTransaction";

impl RpcClient {
    pub async fn simulate_transaction(
        &self,
        envelope: &TransactionEnvelope,
    ) -> Result<SimulateTransactionResult> {
        let params = Params {
            transaction: envelope.to_xdr_base64(Limits::none())?,
        };
        let raw: RawResult = self.call(METHOD, params).await?;
        raw.decode()
    }
}

#[derive(Serialize)]
struct Params {
    /// Base64 `TransactionEnvelope`.
    transaction: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawResult {
    latest_ledger: u32,
    /// Base64 `SorobanTransactionData`; absent when the simulation failed.
    transaction_data: Option<String>,
    /// Stroops, sent as a string.
    min_resource_fee: Option<String>,
    #[serde(default)]
    results: Vec<RawHostFunctionResult>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct RawHostFunctionResult {
    /// Base64 `ScVal`.
    xdr: String,
    /// Base64 `SorobanAuthorizationEntry`s.
    #[serde(default)]
    auth: Vec<String>,
}

impl RawResult {
    fn decode(self) -> Result<SimulateTransactionResult> {
        let transaction_data = self
            .transaction_data
            .filter(|xdr| !xdr.is_empty())
            .map(|xdr| SorobanTransactionData::from_xdr_base64(xdr, Limits::none()))
            .transpose()?;
        let min_resource_fee = self
            .min_resource_fee
            .map(|fee| {
                fee.parse().map_err(|_| Error::InvalidResponse {
                    method: METHOD,
                    reason: format!("minResourceFee {fee:?} is not a number"),
                })
            })
            .transpose()?;
        let results = self
            .results
            .into_iter()
            .map(|result| {
                Ok(HostFunctionResult {
                    return_value: ScVal::from_xdr_base64(result.xdr, Limits::none())?,
                    auth: result
                        .auth
                        .into_iter()
                        .map(|entry| {
                            SorobanAuthorizationEntry::from_xdr_base64(entry, Limits::none())
                        })
                        .collect::<Result<_, _>>()?,
                })
            })
            .collect::<Result<_>>()?;
        Ok(SimulateTransactionResult {
            latest_ledger: self.latest_ledger,
            transaction_data,
            min_resource_fee,
            results,
            error: self.error,
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::test_utils::{b64, server_returning};
    use crate::xdr::{
        LedgerFootprint, Memo, MuxedAccount, Preconditions, SequenceNumber, SorobanResources,
        SorobanTransactionDataExt, Transaction, TransactionExt, TransactionV1Envelope, Uint256,
    };

    fn envelope() -> TransactionEnvelope {
        TransactionEnvelope::Tx(TransactionV1Envelope {
            tx: Transaction {
                source_account: MuxedAccount::Ed25519(Uint256([1; 32])),
                fee: 100,
                seq_num: SequenceNumber(0),
                cond: Preconditions::None,
                memo: Memo::None,
                operations: Default::default(),
                ext: TransactionExt::V0,
            },
            signatures: Default::default(),
        })
    }

    fn transaction_data() -> SorobanTransactionData {
        SorobanTransactionData {
            ext: SorobanTransactionDataExt::V0,
            resources: SorobanResources {
                footprint: LedgerFootprint {
                    read_only: Default::default(),
                    read_write: Default::default(),
                },
                instructions: 1000,
                disk_read_bytes: 0,
                write_bytes: 0,
            },
            resource_fee: 6130,
        }
    }

    #[tokio::test]
    async fn sends_the_envelope_and_decodes_the_result() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(body_partial_json(json!({
                "method": "simulateTransaction",
                "params": { "transaction": b64(&envelope()) },
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0", "id": 1,
                "result": {
                    "latestLedger": 7,
                    "transactionData": b64(&transaction_data()),
                    "minResourceFee": "6130",
                    "results": [{"xdr": b64(&ScVal::U32(7)), "auth": []}],
                    "events": [],
                },
            })))
            .expect(1)
            .mount(&server)
            .await;

        let result = RpcClient::new(server.uri())
            .simulate_transaction(&envelope())
            .await
            .unwrap();

        assert_eq!(result.latest_ledger, 7);
        assert_eq!(result.transaction_data, Some(transaction_data()));
        assert_eq!(result.min_resource_fee, Some(6130));
        assert_eq!(result.results[0].return_value, ScVal::U32(7));
        assert!(result.results[0].auth.is_empty());
        assert_eq!(result.error, None);
    }

    #[tokio::test]
    async fn decodes_a_failed_simulation() {
        let server = server_returning(json!({
            "jsonrpc": "2.0", "id": 1,
            "result": {"latestLedger": 7, "error": "HostError: Error(Value, InvalidInput)"},
        }))
        .await;

        let result = RpcClient::new(server.uri())
            .simulate_transaction(&envelope())
            .await
            .unwrap();

        assert_eq!(result.transaction_data, None);
        assert_eq!(result.min_resource_fee, None);
        assert!(result.results.is_empty());
        assert_eq!(
            result.error.as_deref(),
            Some("HostError: Error(Value, InvalidInput)")
        );
    }
}
