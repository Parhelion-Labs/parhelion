//! `getNetwork`: passphrase and protocol version of the remote network.
//!
//! Spec: <https://developers.stellar.org/docs/data/apis/rpc/api-reference/methods/getNetwork>

use serde::{Deserialize, Serialize};

use super::{Network, Result, RpcClient};

const METHOD: &str = "getNetwork";

impl RpcClient {
    pub async fn get_network(&self) -> Result<Network> {
        let raw: RawResult = self.call(METHOD, Params {}).await?;
        Ok(Network {
            passphrase: raw.passphrase,
            protocol_version: raw.protocol_version,
        })
    }
}

#[derive(Serialize)]
struct Params {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawResult {
    passphrase: String,
    protocol_version: u32,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::test_utils::server_returning;

    #[tokio::test]
    async fn decodes_the_network() {
        let server = server_returning(json!({
            "jsonrpc": "2.0", "id": 1,
            "result": {
                "passphrase": "Public Global Stellar Network ; September 2015",
                "protocolVersion": 29,
            },
        }))
        .await;

        let network = RpcClient::new(server.uri()).get_network().await.unwrap();

        assert_eq!(
            network,
            Network {
                passphrase: "Public Global Stellar Network ; September 2015".into(),
                protocol_version: 29,
            }
        );
    }
}
