//! JSON-RPC 2.0 envelope shared by every method.

use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub(super) struct Request<P> {
    pub(super) jsonrpc: &'static str,
    pub(super) id: u64,
    pub(super) method: &'static str,
    pub(super) params: P,
}

#[derive(Deserialize)]
pub(super) struct Response<R> {
    pub(super) result: Option<R>,
    pub(super) error: Option<RpcError>,
}

#[derive(Deserialize)]
pub(super) struct RpcError {
    pub(super) code: i64,
    pub(super) message: String,
}
