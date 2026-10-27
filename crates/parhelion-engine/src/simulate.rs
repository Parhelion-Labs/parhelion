//! Simulation of an `InvokeHostFunction` operation, the way `simulateTransaction` does it.
//!
//! Mirrors stellar-rpc's preflight (v29): recording mode with non-root authorization disabled,
//! the default resource adjustments, and the store handed to the host as is. Since protocol 23
//! the host restores archived entries inside the invocation itself and reports them in the
//! transaction data, so no separate restore preamble is produced here.

use std::rc::Rc;

use soroban_env_host_curr::e2e_invoke::RecordingInvocationAuthMode;
use soroban_env_host_curr::storage::SnapshotSource;
use soroban_env_host_curr::HostError;
use soroban_simulation_curr::simulation::{
    simulate_invoke_host_function_op, InvokeHostFunctionSimulationResult,
    SimulationAdjustmentConfig,
};

use crate::xdr::{
    AccountId, HostFunction, InvokeContractArgs, LedgerFootprint, LedgerKey, ScAddress, ScSymbol,
    ScVal, SorobanResources, SorobanTransactionData,
};
use crate::{Error, Fork, Result};

/// What one simulation produced.
pub struct Simulation {
    /// Return value, authorization entries, events, transaction data and modified entries.
    pub result: InvokeHostFunctionSimulationResult,
    /// Keys the store fetched from the remote during this simulation, in fetch order.
    pub fetched_keys: Vec<LedgerKey>,
}

impl Simulation {
    pub fn return_value(&self) -> Result<&ScVal, &HostError> {
        self.result.invoke_result.as_ref()
    }

    /// `None` when the invocation failed.
    pub fn transaction_data(&self) -> Option<&SorobanTransactionData> {
        self.result.transaction_data.as_ref()
    }

    pub fn footprint(&self) -> Option<&LedgerFootprint> {
        self.resources().map(|resources| &resources.footprint)
    }

    pub fn resources(&self) -> Option<&SorobanResources> {
        self.transaction_data().map(|data| &data.resources)
    }

    pub fn min_resource_fee(&self) -> Option<i64> {
        self.transaction_data().map(|data| data.resource_fee)
    }
}

impl Fork {
    /// Simulates `host_function` submitted by `source_account` against the fork.
    pub fn simulate(
        &self,
        host_function: HostFunction,
        source_account: &AccountId,
    ) -> Result<Simulation> {
        self.store.take_fetched();
        let snapshot: Rc<dyn SnapshotSource> = self.store.clone();
        let result = simulate_invoke_host_function_op(
            snapshot,
            &self.network_config,
            &SimulationAdjustmentConfig::default_adjustment(),
            &self.ledger_info,
            host_function,
            RecordingInvocationAuthMode::recording(true, false),
            source_account,
            [0; 32],
            true,
        )
        .map_err(Error::Simulation)?;
        Ok(Simulation {
            result,
            fetched_keys: self.store.take_fetched(),
        })
    }
}

/// The host function invoking `function` of `contract` with `args`.
pub fn invoke_contract(
    contract: ScAddress,
    function: &str,
    args: Vec<ScVal>,
) -> Result<HostFunction> {
    Ok(HostFunction::InvokeContract(InvokeContractArgs {
        contract_address: contract,
        function_name: ScSymbol(function.try_into()?),
        args: args.try_into()?,
    }))
}
