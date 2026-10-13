use crate::rpc::get_ledger_entries::MAX_KEYS_PER_REQUEST;

/// Errors returned by [`RpcClient`](super::RpcClient).
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The request could not be sent, or the response could not be read.
    #[error("HTTP error calling {method}: {source}")]
    Http {
        method: &'static str,
        source: reqwest::Error,
    },

    /// The server answered with a JSON-RPC error object.
    #[error("RPC error from {method} ({code}): {message}")]
    Rpc {
        method: &'static str,
        code: i64,
        message: String,
    },

    /// The response was well-formed JSON but not a valid result for the method.
    #[error("invalid response from {method}: {reason}")]
    InvalidResponse {
        method: &'static str,
        reason: String,
    },

    /// A ledger key or entry could not be encoded or decoded.
    #[error("XDR error: {0}")]
    Xdr(#[from] crate::xdr::Error),

    /// More keys were requested than one call accepts.
    #[error("{0} keys requested, at most {MAX_KEYS_PER_REQUEST} per request")]
    TooManyKeys(usize),
}

/// Result alias for this module.
pub type Result<T, E = Error> = std::result::Result<T, E>;
