//! Fixtures shared by unit tests.

use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::xdr::{
    AccountEntry, AccountId, LedgerEntryData, LedgerKey, LedgerKeyAccount, Limits, PublicKey,
    Uint256, WriteXdr,
};

/// Account id whose ed25519 key is `byte` repeated.
pub(crate) fn account_id(byte: u8) -> AccountId {
    AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([byte; 32])))
}

/// Ledger key of the account [`account_id`]`(byte)`.
pub(crate) fn account_key(byte: u8) -> LedgerKey {
    LedgerKey::Account(LedgerKeyAccount {
        account_id: account_id(byte),
    })
}

/// Minimal account entry for [`account_id`]`(byte)` holding `balance` stroops.
pub(crate) fn account_data(byte: u8, balance: i64) -> LedgerEntryData {
    LedgerEntryData::Account(AccountEntry {
        account_id: account_id(byte),
        balance,
        seq_num: 1.into(),
        num_sub_entries: 0,
        inflation_dest: None,
        flags: 0,
        home_domain: Default::default(),
        thresholds: [1, 0, 0, 0].into(),
        signers: Default::default(),
        ext: Default::default(),
    })
}

/// Base64 XDR of `value`.
pub(crate) fn b64(value: &impl WriteXdr) -> String {
    value.to_xdr_base64(Limits::none()).unwrap()
}

/// Mock server answering every POST with `body`.
pub(crate) async fn server_returning(body: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;
    server
}
