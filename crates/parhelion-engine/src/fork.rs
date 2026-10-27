//! A fork of a remote network: its entries behind the layered store, plus the ledger and network
//! configuration the host simulates with.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use parhelion_state::disk_cache::DiskCache;
use parhelion_state::fetcher::Fetcher;
use parhelion_state::rpc::RpcClient;
use parhelion_state::store::{LayeredStore, RemoteSource};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use soroban_env_host_curr::LedgerInfo;
use soroban_simulation_curr::NetworkConfig;

use crate::xdr::{ConfigSettingId, LedgerKey, LedgerKeyConfigSetting};
use crate::{Error, Result};

/// Mainnet's base reserve in stroops. It lives in the ledger header, not in any entry.
const BASE_RESERVE: u32 = 5_000_000;

#[derive(Debug, Clone)]
pub struct ForkOptions {
    pub rpc_url: String,
    /// Freezes what runs see, see the README. `None` follows the remote's latest ledger.
    pub pin_ledger: Option<u32>,
    /// Where pinned ledgers are cached.
    pub cache_dir: PathBuf,
}

pub type Remote = Box<dyn RemoteSource + Send + Sync>;

pub struct Fork {
    /// Shared with the host, which takes its snapshot source as an `Rc`.
    pub store: Rc<LayeredStore<Remote>>,
    pub ledger_info: LedgerInfo,
    pub network_config: NetworkConfig,
}

impl Fork {
    /// Connects to the remote and loads what every simulation needs: the ledger header and the
    /// network's `ConfigSetting` entries. A pinned fork reads both from disk after its first run.
    pub async fn connect(options: &ForkOptions) -> Result<Self> {
        let client = RpcClient::new(options.rpc_url.clone());
        let fetcher = Fetcher::spawn(options.rpc_url.clone())?;
        let (remote, header): (Remote, LedgerHeader) = match options.pin_ledger {
            None => (Box::new(fetcher), LedgerHeader::fetch(&client).await?),
            Some(ledger) => {
                let cache = DiskCache::open(&options.cache_dir, ledger, fetcher)?;
                let header = LedgerHeader::pinned(cache.dir(), ledger, &client).await?;
                (Box::new(cache), header)
            }
        };

        let store = LayeredStore::new(remote);
        store.prefetch(&config_setting_keys())?;
        let network_config =
            NetworkConfig::load_from_snapshot(&store).map_err(Error::NetworkConfig)?;

        let mut ledger_info = LedgerInfo {
            protocol_version: header.protocol_version,
            sequence_number: header.sequence,
            timestamp: header.close_time,
            network_id: Sha256::digest(header.passphrase.as_bytes()).into(),
            base_reserve: BASE_RESERVE,
            ..Default::default()
        };
        network_config.fill_config_fields_in_ledger_info(&mut ledger_info);

        Ok(Self {
            store: Rc::new(store),
            ledger_info,
            network_config,
        })
    }
}

/// Every `ConfigSetting` key, so the network configuration loads in one remote call.
fn config_setting_keys() -> Vec<LedgerKey> {
    ConfigSettingId::VARIANTS
        .iter()
        .map(|&config_setting_id| {
            LedgerKey::ConfigSetting(LedgerKeyConfigSetting { config_setting_id })
        })
        .collect()
}

/// What the host needs to know about the ledger besides its entries.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LedgerHeader {
    sequence: u32,
    /// Unix seconds.
    close_time: u64,
    protocol_version: u32,
    passphrase: String,
}

impl LedgerHeader {
    const FILE: &'static str = "ledger.json";

    async fn fetch(client: &RpcClient) -> Result<Self> {
        let latest = client.get_latest_ledger().await?;
        let network = client.get_network().await?;
        Ok(Self {
            sequence: latest.sequence,
            close_time: latest.close_time,
            protocol_version: latest.protocol_version,
            passphrase: network.passphrase,
        })
    }

    /// The header of pinned `ledger`: read from `dir`, or fetched once and written there so later
    /// runs need no network at all.
    async fn pinned(dir: &Path, ledger: u32, client: &RpcClient) -> Result<Self> {
        let path = dir.join(Self::FILE);
        match fs::read(&path) {
            Ok(json) => return Ok(serde_json::from_slice(&json).map_err(io::Error::from)?),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let header = Self {
            sequence: ledger,
            ..Self::fetch(client).await?
        };
        fs::write(
            &path,
            serde_json::to_vec_pretty(&header).map_err(io::Error::from)?,
        )?;
        Ok(header)
    }
}
