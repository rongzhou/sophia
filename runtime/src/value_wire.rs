//! Host 边界 ValueWire 编码。
//!
//! 这是 program.wasm 动态 import 与三方 `host.wasm` provider 共用的唯一字节协议：
//! `ArgsWire = u32 argc + ValueWire*`，`ValueWire = tag + payload`。Intent 在边界擦除为内层标量。

use crate::Value;
use sophia_library::{Scalar, TypeDesc};

const WIRE_UNIT: u8 = 0;
const WIRE_BOOL: u8 = 1;
const WIRE_INT: u8 = 2;
const WIRE_TEXT: u8 = 3;

pub(crate) fn encode_args(args: &[Value], params: &[TypeDesc]) -> Result<Vec<u8>, String> {
    if args.len() != params.len() {
        return Err(format!(
            "ValueWire 期望 {} 个实参，得到 {}",
            params.len(),
            args.len()
        ));
    }
    let mut out = Vec::new();
    out.extend_from_slice(&(args.len() as u32).to_le_bytes());
    for (idx, (value, desc)) in args.iter().zip(params).enumerate() {
        encode_typed_value(value, desc, &mut out)
            .map_err(|e| format!("ValueWire 第 {} 个实参：{e}", idx + 1))?;
    }
    Ok(out)
}

pub(crate) fn decode_args(bytes: &[u8]) -> Result<Vec<Value>, String> {
    if bytes.len() < 4 {
        return Err("ValueWire args 缺 argc".into());
    }
    let argc = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
    let mut off = 4usize;
    let mut args = Vec::with_capacity(argc);
    for _ in 0..argc {
        args.push(decode_value(bytes, &mut off)?);
    }
    if off != bytes.len() {
        return Err("ValueWire args 尾部有多余字节".into());
    }
    Ok(args)
}

pub(crate) fn encode_typed_value(
    value: &Value,
    desc: &TypeDesc,
    out: &mut Vec<u8>,
) -> Result<(), String> {
    match scalar(desc) {
        Scalar::Unit => match value {
            Value::Unit => {
                out.push(WIRE_UNIT);
                Ok(())
            }
            other => Err(format!("应为 Unit，实际 {other:?}")),
        },
        Scalar::Bool => match value {
            Value::Bool(b) => {
                out.push(WIRE_BOOL);
                out.push(u8::from(*b));
                Ok(())
            }
            other => Err(format!("应为 Bool，实际 {other:?}")),
        },
        Scalar::Int => match value {
            Value::Int(i) => {
                out.push(WIRE_INT);
                out.extend_from_slice(&i.to_le_bytes());
                Ok(())
            }
            other => Err(format!("应为 Int，实际 {other:?}")),
        },
        Scalar::Text => match value {
            Value::Text(text) => {
                out.push(WIRE_TEXT);
                out.extend_from_slice(&(text.len() as u32).to_le_bytes());
                out.extend_from_slice(text.as_bytes());
                Ok(())
            }
            other => Err(format!("应为 Text，实际 {other:?}")),
        },
    }
}

pub(crate) fn encode_value(value: &Value) -> Result<Vec<u8>, String> {
    let desc = match value {
        Value::Unit => TypeDesc::Scalar(Scalar::Unit),
        Value::Bool(_) => TypeDesc::Scalar(Scalar::Bool),
        Value::Int(_) => TypeDesc::Scalar(Scalar::Int),
        Value::Text(_) => TypeDesc::Scalar(Scalar::Text),
        other => return Err(format!("ValueWire 暂不支持 host 返回值 {other:?}")),
    };
    let mut out = Vec::new();
    encode_typed_value(value, &desc, &mut out)?;
    Ok(out)
}

pub(crate) fn decode_typed_value(bytes: &[u8], desc: &TypeDesc) -> Result<Value, String> {
    let mut off = 0usize;
    let value = decode_value(bytes, &mut off)?;
    if off != bytes.len() {
        return Err("ValueWire 返回值尾部有多余字节".into());
    }
    validate_scalar(&value, scalar(desc))?;
    Ok(value)
}

fn decode_value(bytes: &[u8], off: &mut usize) -> Result<Value, String> {
    let tag = *bytes
        .get(*off)
        .ok_or_else(|| "ValueWire 缺 tag".to_string())?;
    *off += 1;
    match tag {
        WIRE_UNIT => Ok(Value::Unit),
        WIRE_BOOL => {
            let b = *bytes
                .get(*off)
                .ok_or_else(|| "ValueWire Bool 缺 payload".to_string())?;
            *off += 1;
            Ok(Value::Bool(b != 0))
        }
        WIRE_INT => {
            if bytes.len() < *off + 8 {
                return Err("ValueWire Int 缺 payload".into());
            }
            let i = i64::from_le_bytes(bytes[*off..*off + 8].try_into().unwrap());
            *off += 8;
            Ok(Value::Int(i))
        }
        WIRE_TEXT => {
            if bytes.len() < *off + 4 {
                return Err("ValueWire Text 缺长度".into());
            }
            let len = u32::from_le_bytes(bytes[*off..*off + 4].try_into().unwrap()) as usize;
            *off += 4;
            if bytes.len() < *off + len {
                return Err("ValueWire Text 缺字节".into());
            }
            let s = String::from_utf8(bytes[*off..*off + len].to_vec())
                .map_err(|e| format!("ValueWire Text 非 UTF-8：{e}"))?;
            *off += len;
            Ok(Value::Text(s))
        }
        other => Err(format!("未知 ValueWire tag {other}")),
    }
}

fn validate_scalar(value: &Value, expected: Scalar) -> Result<(), String> {
    let ok = matches!(
        (expected, value),
        (Scalar::Unit, Value::Unit)
            | (Scalar::Bool, Value::Bool(_))
            | (Scalar::Int, Value::Int(_))
            | (Scalar::Text, Value::Text(_))
    );
    if ok {
        Ok(())
    } else {
        Err(format!(
            "ValueWire 返回值应为 {}，实际 {value:?}",
            expected.as_str()
        ))
    }
}

fn scalar(desc: &TypeDesc) -> Scalar {
    match desc {
        TypeDesc::Scalar(s) => *s,
        TypeDesc::Intent { inner, .. } => *inner,
    }
}
