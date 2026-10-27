//! Bridges the host's synchronous `SnapshotSource::get` to the async [`RpcClient`].
//!
//! A dedicated thread owns its own Tokio runtime, so [`Fetcher::fetch`] can block the caller
//! without ever blocking the runtime that serves JSON-RPC.

use std::collections::HashMap;
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::rpc::{self, LedgerEntries, RemoteEntry, RpcClient, MAX_KEYS_PER_REQUEST};
use crate::xdr::LedgerKey;

/// How long the fetcher waits for more keys before sending a batch.
const COALESCE_WINDOW: Duration = Duration::from_millis(5);

#[derive(Debug, Clone, thiserror::Error)]
pub enum FetchError {
    /// Shared by every caller waiting on the failed batch.
    #[error(transparent)]
    Remote(Arc<rpc::Error>),
    #[error("fetcher thread has stopped")]
    Stopped,
    /// The on-disk cache of a pinned ledger could not be read or written.
    #[error("disk cache: {0}")]
    Disk(Arc<std::io::Error>),
}

pub type FetchResult = Result<Option<RemoteEntry>, FetchError>;

/// Handle to the fetcher thread. Cheap to clone; the thread stops when every handle is dropped.
#[derive(Debug, Clone)]
pub struct Fetcher {
    requests: UnboundedSender<Request>,
}

impl Fetcher {
    /// Starts a fetcher thread for the RPC at `url`.
    ///
    /// The thread builds its own HTTP client: a connection is driven by the runtime that opened
    /// it, so a client shared with a runtime that then blocks on [`Fetcher::fetch`] would hang.
    pub fn spawn(url: impl Into<String>) -> std::io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let url = url.into();

        let (requests, receiver) = unbounded_channel();
        thread::Builder::new()
            .name("parhelion-fetcher".into())
            .spawn(move || runtime.block_on(run(RpcClient::new(url), receiver)))?;

        Ok(Self { requests })
    }

    /// Fetches `keys` from the remote, blocking until all have answered.
    /// Results are in the order of `keys`; `None` means the key does not exist.
    pub fn fetch(&self, keys: &[LedgerKey]) -> Vec<FetchResult> {
        // Send every key before waiting on any, so they can share one batch.
        let replies: Vec<_> = keys.iter().map(|key| self.send(key.clone())).collect();
        replies
            .into_iter()
            .map(|reply| {
                reply
                    .and_then(|reply| reply.recv().ok())
                    .unwrap_or(Err(FetchError::Stopped))
            })
            .collect()
    }

    /// Queues one key; `None` if the fetcher thread has stopped.
    fn send(&self, key: LedgerKey) -> Option<mpsc::Receiver<FetchResult>> {
        let (reply, receiver) = mpsc::channel();
        self.requests.send(Request { key, reply }).ok()?;
        Some(receiver)
    }
}

struct Request {
    key: LedgerKey,
    /// A std channel, so the caller can block on it from any thread, async or not.
    reply: mpsc::Sender<FetchResult>,
}

type BatchResult = (Vec<LedgerKey>, rpc::Result<LedgerEntries>);

async fn run(client: RpcClient, mut requests: UnboundedReceiver<Request>) {
    // Every key queued or in flight, with the callers waiting for it.
    let mut waiting: HashMap<LedgerKey, Vec<mpsc::Sender<FetchResult>>> = HashMap::new();
    let (done, mut finished) = unbounded_channel::<BatchResult>();

    loop {
        tokio::select! {
            request = requests.recv() => {
                let Some(request) = request else { break };
                let mut batch = Vec::new();
                enqueue(&mut waiting, &mut batch, request);

                let window = tokio::time::sleep(COALESCE_WINDOW);
                tokio::pin!(window);
                loop {
                    tokio::select! {
                        () = &mut window => break,
                        request = requests.recv() => match request {
                            Some(request) => enqueue(&mut waiting, &mut batch, request),
                            None => break,
                        },
                    }
                }

                for keys in batch.chunks(MAX_KEYS_PER_REQUEST) {
                    let keys = keys.to_vec();
                    let client = client.clone();
                    let done = done.clone();
                    tokio::spawn(async move {
                        let result = client.get_ledger_entries(&keys).await;
                        let _ = done.send((keys, result));
                    });
                }
            }
            Some((keys, result)) = finished.recv() => answer(&mut waiting, keys, result),
        }
    }
}

/// Registers the caller; only a key nobody is waiting for yet joins the batch.
fn enqueue(
    waiting: &mut HashMap<LedgerKey, Vec<mpsc::Sender<FetchResult>>>,
    batch: &mut Vec<LedgerKey>,
    request: Request,
) {
    let callers = waiting.entry(request.key.clone()).or_default();
    if callers.is_empty() {
        batch.push(request.key);
    }
    callers.push(request.reply);
}

fn answer(
    waiting: &mut HashMap<LedgerKey, Vec<mpsc::Sender<FetchResult>>>,
    keys: Vec<LedgerKey>,
    result: rpc::Result<LedgerEntries>,
) {
    let result = result.map_err(Arc::new);
    let mut found: HashMap<LedgerKey, RemoteEntry> = match &result {
        Ok(entries) => entries
            .entries
            .iter()
            .map(|entry| (entry.key.clone(), entry.clone()))
            .collect(),
        Err(_) => HashMap::new(),
    };
    for key in keys {
        let value = match &result {
            Ok(_) => Ok(found.remove(&key)),
            Err(error) => Err(FetchError::Remote(error.clone())),
        };
        for caller in waiting.remove(&key).unwrap_or_default() {
            // The caller may have given up; nothing to do then.
            let _ = caller.send(value.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Barrier;

    use serde_json::{json, Value};
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::test_utils::{account_data, account_key, b64};

    /// Mock server answering every POST with account `1` after `delay`.
    async fn server_with_account_1(delay: Duration) -> MockServer {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "latestLedger": 100,
                "entries": [{
                    "key": b64(&account_key(1)),
                    "xdr": b64(&account_data(1, 7)),
                    "lastModifiedLedgerSeq": 90,
                }],
            },
        });
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(body)
                    .set_delay(delay),
            )
            .mount(&server)
            .await;
        server
    }

    /// Keys sent in each request the server received.
    async fn requested_keys(server: &MockServer) -> Vec<Vec<String>> {
        let requests = server.received_requests().await.unwrap();
        requests
            .iter()
            .map(|request| {
                let body: Value = request.body_json().unwrap();
                serde_json::from_value(body["params"]["keys"].clone()).unwrap()
            })
            .collect()
    }

    fn fetcher(server: &MockServer) -> Fetcher {
        Fetcher::spawn(server.uri()).unwrap()
    }

    #[tokio::test]
    async fn returns_results_in_key_order() {
        let server = server_with_account_1(Duration::ZERO).await;

        let results = fetcher(&server).fetch(&[account_key(2), account_key(1)]);

        assert!(matches!(results[0], Ok(None)));
        let entry = results[1].clone().unwrap().unwrap();
        assert_eq!(entry.key, account_key(1));
    }

    #[tokio::test]
    async fn sends_a_repeated_key_once() {
        let server = server_with_account_1(Duration::ZERO).await;

        fetcher(&server).fetch(&[account_key(1), account_key(1)]);

        assert_eq!(requested_keys(&server).await, [[b64(&account_key(1))]]);
    }

    #[tokio::test]
    async fn coalesces_concurrent_callers_into_one_request() {
        let server = server_with_account_1(Duration::ZERO).await;
        let fetcher = fetcher(&server);
        let start = Barrier::new(3);

        thread::scope(|scope| {
            for byte in 1..=3 {
                let (fetcher, start) = (&fetcher, &start);
                scope.spawn(move || {
                    start.wait();
                    fetcher.fetch(&[account_key(byte)]);
                });
            }
        });

        let requests = requested_keys(&server).await;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].len(), 3);
    }

    #[tokio::test]
    async fn joins_a_fetch_already_in_flight() {
        let server = server_with_account_1(Duration::from_millis(200)).await;
        let fetcher = fetcher(&server);

        thread::scope(|scope| {
            scope.spawn(|| fetcher.fetch(&[account_key(1)]));
            thread::sleep(Duration::from_millis(50));
            scope.spawn(|| fetcher.fetch(&[account_key(1)]));
        });

        assert_eq!(requested_keys(&server).await.len(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn does_not_deadlock_inside_another_runtime() {
        let server = server_with_account_1(Duration::ZERO).await;

        // Blocks this runtime's only thread; the fetcher must still make progress.
        let results = fetcher(&server).fetch(&[account_key(1)]);

        assert!(results[0].as_ref().unwrap().is_some());
    }

    #[tokio::test]
    async fn shares_a_remote_error_with_every_caller() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let results = fetcher(&server).fetch(&[account_key(1), account_key(2)]);

        assert!(matches!(results[0], Err(FetchError::Remote(_))));
        assert!(matches!(results[1], Err(FetchError::Remote(_))));
    }
}
