//! Contract arguments on the command line, written as `<type>:<value>`.

use anyhow::{anyhow, bail, Context, Result};
use parhelion_engine::xdr::{ScBytes, ScString, ScSymbol, ScVal};

pub const SYNTAX: &str = "\
<type>:<value>, where <type> is one of
  bool, u32, i32, u64, i64, u128, i128, symbol, string, bytes (hex), address (G... or C...)
or the bare word `void`.";

pub fn parse(arg: &str) -> Result<ScVal> {
    if arg == "void" {
        return Ok(ScVal::Void);
    }
    let (kind, value) = arg
        .split_once(':')
        .ok_or_else(|| anyhow!("expected {SYNTAX}\ngot {arg:?}"))?;
    let parsed = match kind {
        "bool" => value.parse::<bool>()?.into(),
        "u32" => value.parse::<u32>()?.into(),
        "i32" => value.parse::<i32>()?.into(),
        "u64" => value.parse::<u64>()?.into(),
        "i64" => value.parse::<i64>()?.into(),
        "u128" => value.parse::<u128>()?.into(),
        "i128" => value.parse::<i128>()?.into(),
        "symbol" => ScVal::Symbol(ScSymbol(value.try_into()?)),
        "string" => ScVal::String(ScString(value.try_into()?)),
        "bytes" => ScVal::Bytes(ScBytes(hex::decode(value)?.try_into()?)),
        "address" => ScVal::Address(value.parse()?),
        _ => bail!("unknown argument type {kind:?}; expected {SYNTAX}"),
    };
    Ok(parsed)
}

/// Parses every argument, naming the bad one on failure.
pub fn parse_all(args: &[String]) -> Result<Vec<ScVal>> {
    args.iter()
        .map(|arg| parse(arg).with_context(|| format!("argument {arg:?}")))
        .collect()
}

#[cfg(test)]
mod tests {
    use parhelion_engine::xdr::{Int128Parts, ScAddress};

    use super::*;

    #[test]
    fn parses_every_type() {
        assert_eq!(parse("void").unwrap(), ScVal::Void);
        assert_eq!(parse("bool:true").unwrap(), ScVal::Bool(true));
        assert_eq!(parse("u32:7").unwrap(), ScVal::U32(7));
        assert_eq!(parse("i32:-7").unwrap(), ScVal::I32(-7));
        assert_eq!(parse("u64:7").unwrap(), ScVal::U64(7));
        assert_eq!(parse("i64:-7").unwrap(), ScVal::I64(-7));
        assert_eq!(
            parse("i128:-1").unwrap(),
            ScVal::I128(Int128Parts {
                hi: -1,
                lo: u64::MAX
            })
        );
        assert_eq!(
            parse("symbol:transfer").unwrap(),
            ScVal::Symbol(ScSymbol("transfer".try_into().unwrap()))
        );
        assert_eq!(
            parse("string:hi there").unwrap(),
            ScVal::String(ScString("hi there".try_into().unwrap()))
        );
        assert_eq!(
            parse("bytes:0aff").unwrap(),
            ScVal::Bytes(ScBytes(vec![0x0a, 0xff].try_into().unwrap()))
        );
        let contract = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
        assert_eq!(
            parse(&format!("address:{contract}")).unwrap(),
            ScVal::Address(contract.parse::<ScAddress>().unwrap())
        );
    }

    #[test]
    fn rejects_bad_arguments() {
        assert!(parse("7").unwrap_err().to_string().contains("expected"));
        assert!(parse("u8:7").unwrap_err().to_string().contains("u8"));
        assert!(parse("u32:x").is_err());
        assert!(parse("address:nope").is_err());
        assert!(parse_all(&["u32:1".into(), "u32:x".into()])
            .unwrap_err()
            .to_string()
            .contains("u32:x"));
    }
}
