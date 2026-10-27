//! Short, readable forms of the XDR values the CLI prints.

use parhelion_engine::xdr::{
    ContractDataDurability, Int128Parts, LedgerKey, ScAddress, ScVal, UInt128Parts,
};

pub fn scval(value: &ScVal) -> String {
    match value {
        ScVal::Bool(b) => b.to_string(),
        ScVal::Void => "void".into(),
        ScVal::U32(n) => n.to_string(),
        ScVal::I32(n) => n.to_string(),
        ScVal::U64(n) => n.to_string(),
        ScVal::I64(n) => n.to_string(),
        ScVal::Timepoint(t) => format!("timepoint {}", t.0),
        ScVal::Duration(d) => format!("duration {}", d.0),
        ScVal::U128(UInt128Parts { hi, lo }) => {
            ((u128::from(*hi) << 64) | u128::from(*lo)).to_string()
        }
        ScVal::I128(Int128Parts { hi, lo }) => {
            ((i128::from(*hi) << 64) | i128::from(*lo)).to_string()
        }
        ScVal::Bytes(bytes) => format!("0x{}", hex::encode(bytes.as_slice())),
        ScVal::String(s) => format!("{:?}", s.to_string()),
        ScVal::Symbol(s) => s.to_string(),
        ScVal::Vec(Some(items)) => {
            format!(
                "[{}]",
                items.iter().map(scval).collect::<Vec<_>>().join(", ")
            )
        }
        ScVal::Map(Some(entries)) => format!(
            "{{{}}}",
            entries
                .iter()
                .map(|entry| format!("{}: {}", scval(&entry.key), scval(&entry.val)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ScVal::Address(address) => self::address(address),
        other => format!("{other:?}"),
    }
}

pub fn address(address: &ScAddress) -> String {
    match address {
        ScAddress::Account(account) => account.to_string(),
        ScAddress::Contract(contract) => contract.to_string(),
        other => format!("{other:?}"),
    }
}

pub fn ledger_key(key: &LedgerKey) -> String {
    match key {
        LedgerKey::Account(account) => format!("account {}", account.account_id),
        LedgerKey::Trustline(line) => {
            format!("trustline {} {:?}", line.account_id, line.asset)
        }
        LedgerKey::ContractData(data) => format!(
            "contract_data {} {} ({})",
            address(&data.contract),
            scval(&data.key),
            match data.durability {
                ContractDataDurability::Persistent => "persistent",
                ContractDataDurability::Temporary => "temporary",
            }
        ),
        LedgerKey::ContractCode(code) => format!("contract_code {}", code.hash),
        LedgerKey::ConfigSetting(setting) => {
            format!("config_setting {:?}", setting.config_setting_id)
        }
        LedgerKey::Ttl(ttl) => format!("ttl {}", ttl.key_hash),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use parhelion_engine::xdr::{
        LedgerKeyContractCode, LedgerKeyContractData, ScMap, ScMapEntry, ScSymbol, ScVec,
    };

    use super::*;

    const USDC_SAC: &str = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";

    #[test]
    fn formats_values() {
        assert_eq!(scval(&ScVal::U32(7)), "7");
        assert_eq!(scval(&ScVal::from(-5i128)), "-5");
        assert_eq!(scval(&ScVal::from(u128::MAX)), u128::MAX.to_string());
        assert_eq!(
            scval(&ScVal::Symbol(ScSymbol("USDC".try_into().unwrap()))),
            "USDC"
        );
        assert_eq!(scval(&ScVal::Address(USDC_SAC.parse().unwrap())), USDC_SAC);
        let vec = ScVal::Vec(Some(ScVec(
            vec![ScVal::U32(1), ScVal::Bool(false)].try_into().unwrap(),
        )));
        assert_eq!(scval(&vec), "[1, false]");
        let map = ScVal::Map(Some(ScMap(
            vec![ScMapEntry {
                key: ScVal::Symbol(ScSymbol("k".try_into().unwrap())),
                val: ScVal::Void,
            }]
            .try_into()
            .unwrap(),
        )));
        assert_eq!(scval(&map), "{k: void}");
    }

    #[test]
    fn formats_keys() {
        let instance = LedgerKey::ContractData(LedgerKeyContractData {
            contract: USDC_SAC.parse().unwrap(),
            key: ScVal::LedgerKeyContractInstance,
            durability: ContractDataDurability::Persistent,
        });
        assert_eq!(
            ledger_key(&instance),
            format!("contract_data {USDC_SAC} LedgerKeyContractInstance (persistent)")
        );
        let code = LedgerKey::ContractCode(LedgerKeyContractCode {
            hash: [0xab; 32].into(),
        });
        assert_eq!(
            ledger_key(&code),
            format!("contract_code {}", "ab".repeat(32))
        );
    }
}
