//! Differential harness: the same invocation through Parhelion and through mainnet RPC's own
//! `simulateTransaction`, compared field by field.
//!
//! Compared: the return value, the read-only and read-write footprint sets, the resources
//! (instructions, disk read bytes, write bytes) and the min resource fee. Both sides run at the
//! latest ledger; the RPC reports which one it used.

use std::collections::BTreeSet;
use std::fmt;

use anyhow::{anyhow, Context, Result};
use parhelion_engine::xdr::{
    AccountId, HostFunction, InvokeHostFunctionOp, LedgerKey, Limits, Memo, MuxedAccount,
    Operation, OperationBody, Preconditions, PublicKey, ScAddress, ScVal, SequenceNumber,
    SorobanResources, Transaction, TransactionEnvelope, TransactionExt, TransactionV1Envelope,
    WriteXdr,
};
use parhelion_engine::{invoke_contract, Fork};
use parhelion_state::rpc::RpcClient;

/// One invocation to compare.
pub struct Case {
    pub name: &'static str,
    /// `C...`
    pub contract: &'static str,
    pub function: &'static str,
    pub args: Vec<ScVal>,
}

/// A field that differs between the two sides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    pub field: &'static str,
    pub parhelion: String,
    pub mainnet: String,
}

pub struct Comparison {
    pub case: &'static str,
    pub parhelion_ledger: u32,
    pub mainnet_ledger: u32,
    pub mismatches: Vec<Mismatch>,
}

impl Comparison {
    pub fn agrees(&self) -> bool {
        self.mismatches.is_empty()
    }
}

impl fmt::Display for Comparison {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{}: Parhelion at ledger {}, mainnet RPC at ledger {}",
            self.case, self.parhelion_ledger, self.mainnet_ledger
        )?;
        if self.agrees() {
            return write!(f, "  all fields agree");
        }
        for mismatch in &self.mismatches {
            writeln!(f, "  {} differs", mismatch.field)?;
            writeln!(f, "    parhelion: {}", mismatch.parhelion)?;
            writeln!(f, "    mainnet:   {}", mismatch.mainnet)?;
        }
        Ok(())
    }
}

/// Runs `case` through `fork` and through the RPC at `rpc`, and compares. The fork should be
/// connected to the same RPC, unpinned, so both sides see the same state.
pub async fn compare(
    fork: &Fork,
    rpc: &RpcClient,
    source: &AccountId,
    case: &Case,
) -> Result<Comparison> {
    let contract: ScAddress = case.contract.parse().context("contract id")?;
    let host_function = invoke_contract(contract, case.function, case.args.clone())?;

    let ours = fork.simulate(host_function.clone(), source)?;
    let theirs = rpc
        .simulate_transaction(&envelope(host_function, source))
        .await?;

    let mut mismatches = Vec::new();
    let mut check = |field, a: String, b: String| {
        if a != b {
            mismatches.push(Mismatch {
                field,
                parhelion: a,
                mainnet: b,
            });
        }
    };

    let our_value = match ours.return_value() {
        Ok(value) => format!("{value:?}"),
        Err(error) => format!("failed: {error}"),
    };
    let their_value = match (&theirs.error, theirs.results.first()) {
        (Some(error), _) => format!("failed: {error}"),
        (None, Some(result)) => format!("{:?}", result.return_value),
        (None, None) => "no result".into(),
    };
    check("return value", our_value, their_value);

    let our_resources = ours.resources();
    let their_resources = theirs.transaction_data.as_ref().map(|data| &data.resources);
    check(
        "footprint read-only",
        key_set(our_resources.map(|r| r.footprint.read_only.as_slice())),
        key_set(their_resources.map(|r| r.footprint.read_only.as_slice())),
    );
    check(
        "footprint read-write",
        key_set(our_resources.map(|r| r.footprint.read_write.as_slice())),
        key_set(their_resources.map(|r| r.footprint.read_write.as_slice())),
    );
    check(
        "resources",
        resources(our_resources),
        resources(their_resources),
    );
    check(
        "min resource fee",
        format!("{:?}", ours.min_resource_fee()),
        format!("{:?}", theirs.min_resource_fee),
    );

    Ok(Comparison {
        case: case.name,
        parhelion_ledger: fork.ledger_info.sequence_number,
        mainnet_ledger: theirs.latest_ledger,
        mismatches,
    })
}

/// Keys as a sorted set of base64 XDR, so order does not matter and the diff is readable.
fn key_set(keys: Option<&[LedgerKey]>) -> String {
    match keys {
        None => "none".into(),
        Some(keys) => {
            let set: BTreeSet<String> = keys
                .iter()
                .map(|key| key.to_xdr_base64(Limits::none()).unwrap_or_default())
                .collect();
            format!("{set:?}")
        }
    }
}

fn resources(resources: Option<&SorobanResources>) -> String {
    match resources {
        None => "none".into(),
        Some(r) => format!(
            "{} instructions, {} bytes read, {} bytes written",
            r.instructions, r.disk_read_bytes, r.write_bytes
        ),
    }
}

/// An unsigned transaction carrying only `host_function`, as `simulateTransaction` wants it.
fn envelope(host_function: HostFunction, source: &AccountId) -> TransactionEnvelope {
    let PublicKey::PublicKeyTypeEd25519(key) = &source.0;
    TransactionEnvelope::Tx(TransactionV1Envelope {
        tx: Transaction {
            source_account: MuxedAccount::Ed25519(key.clone()),
            fee: 100,
            seq_num: SequenceNumber(0),
            cond: Preconditions::None,
            memo: Memo::None,
            operations: vec![Operation {
                source_account: None,
                body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
                    host_function,
                    auth: Default::default(),
                }),
            }]
            .try_into()
            .expect("one operation"),
            ext: TransactionExt::V0,
        },
        signatures: Default::default(),
    })
}

/// Fails with every mismatch listed when any case disagrees.
pub fn assert_all_agree(comparisons: &[Comparison]) -> Result<()> {
    let report: Vec<String> = comparisons.iter().map(ToString::to_string).collect();
    if comparisons.iter().all(Comparison::agrees) {
        return Ok(());
    }
    Err(anyhow!("differential mismatch:\n{}", report.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_every_differing_field() {
        let comparison = Comparison {
            case: "swap",
            parhelion_ledger: 10,
            mainnet_ledger: 11,
            mismatches: vec![
                Mismatch {
                    field: "resources",
                    parhelion: "10 instructions, 0 bytes read, 0 bytes written".into(),
                    mainnet: "12 instructions, 0 bytes read, 0 bytes written".into(),
                },
                Mismatch {
                    field: "min resource fee",
                    parhelion: "Some(100)".into(),
                    mainnet: "Some(101)".into(),
                },
            ],
        };

        assert!(!comparison.agrees());
        assert_eq!(
            comparison.to_string(),
            "swap: Parhelion at ledger 10, mainnet RPC at ledger 11\n  \
             resources differs\n    \
             parhelion: 10 instructions, 0 bytes read, 0 bytes written\n    \
             mainnet:   12 instructions, 0 bytes read, 0 bytes written\n  \
             min resource fee differs\n    \
             parhelion: Some(100)\n    \
             mainnet:   Some(101)\n"
        );
        let error = assert_all_agree(&[comparison]).unwrap_err().to_string();
        assert!(error.contains("min resource fee differs"), "{error}");
    }

    #[test]
    fn key_sets_ignore_order() {
        let a = LedgerKey::ConfigSetting(parhelion_engine::xdr::LedgerKeyConfigSetting {
            config_setting_id: parhelion_engine::xdr::ConfigSettingId::StateArchival,
        });
        let b = LedgerKey::ConfigSetting(parhelion_engine::xdr::LedgerKeyConfigSetting {
            config_setting_id: parhelion_engine::xdr::ConfigSettingId::ContractComputeV0,
        });
        assert_eq!(
            key_set(Some(&[a.clone(), b.clone()])),
            key_set(Some(&[b, a]))
        );
        assert_eq!(key_set(None), "none");
    }
}
