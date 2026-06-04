//! 非浏览器 WASM 程序 runner。
//!
//! 该 runner 执行 `sophia-codegen` 产出的 `program.wasm`，并把 WASM import 接到同一份
//! [`HostRegistry`]。它不发现库、不注册标准库真实实现、不读写项目目录；调用方负责传入构建时同源的
//! [`sophia_library::LibraryRegistry`] 与已注册好的 host。

use crate::{HostRegistry, Outcome, RaisedError, RuntimeError, RuntimeResult, Value};
use sophia_library::LibraryRegistry;
use sophia_semantic::SemanticModel;
use std::collections::BTreeMap;
use wasmi::{Caller, Engine, Linker, Module, Store};

const TAG_UNIT: i32 = 0;
const TAG_BOOL: i32 = 1;
const TAG_INT: i32 = 2;
const TAG_TEXT: i32 = 3;
const TAG_NULL: i32 = 4;
const TAG_ERROR_VALUE: i32 = 6;
const TAG_ENTITY: i32 = 7;
const TAG_STATE: i32 = 8;

const OUTCOME_RETURNED: i32 = 0;
const OUTCOME_RAISED: i32 = 1;

const OFF_TAG: usize = 0;
const OFF_BOOL_PAYLOAD: usize = 4;
const OFF_INT_PAYLOAD: usize = 8;
const OFF_TEXT_PTR: usize = 4;
const OFF_TEXT_LEN: usize = 8;
const OFF_REC_NAME_PTR: usize = 4;
const OFF_REC_NAME_LEN: usize = 8;
const OFF_REC_NFIELDS: usize = 12;
const REC_HEADER_SIZE: usize = 16;
const REC_FIELD_SIZE: usize = 12;
const OFF_FIELD_KEY_PTR: usize = 0;
const OFF_FIELD_KEY_LEN: usize = 4;
const OFF_FIELD_VAL: usize = 8;
const OFF_STATE_NAME_PTR: usize = 4;
const OFF_STATE_NAME_LEN: usize = 8;
const OFF_STATE_VALUE_PTR: usize = 12;
const OFF_STATE_VALUE_LEN: usize = 16;

/// 执行 codegen 产出的 `program.wasm`。
pub struct WasmProgramRunner {
    engine: Engine,
    module: Module,
    ops: Vec<HostImport>,
}

#[derive(Clone)]
struct HostImport {
    module: String,
    name: String,
    family: String,
    op: String,
}

struct RunnerState<'a> {
    host: &'a mut HostRegistry,
    stash: Vec<u8>,
}

impl WasmProgramRunner {
    /// 编译 WASM 模块，并冻结 registry 中的 host import 映射。
    pub fn new(wasm_bytes: &[u8], registry: &LibraryRegistry) -> RuntimeResult<Self> {
        let engine = Engine::default();
        let module = Module::new(&engine, wasm_bytes)
            .map_err(|e| RuntimeError::Validation(format!("program.wasm 加载失败：{e}")))?;
        let ops = registry
            .ops()
            .map(|contract| HostImport {
                module: format!("sophia_lib:{}", contract.lib),
                name: contract.host_fn.clone(),
                family: contract.family.clone(),
                op: contract.op.clone(),
            })
            .collect();
        Ok(WasmProgramRunner {
            engine,
            module,
            ops,
        })
    }

    /// 执行 action / transition 导出。`entry_is_action` 决定导出名前缀。
    pub fn run(
        &self,
        model: &SemanticModel,
        entry: &str,
        args: &[Value],
        entry_is_action: bool,
        host: &mut HostRegistry,
    ) -> RuntimeResult<Outcome> {
        validate_inputs(model, entry, args)?;
        let mut linker = Linker::<RunnerState<'_>>::new(&self.engine);
        link_host(&mut linker, &self.ops)?;
        let mut store = Store::new(
            &self.engine,
            RunnerState {
                host,
                stash: Vec::new(),
            },
        );
        let instance = linker
            .instantiate(&mut store, &self.module)
            .map_err(|e| RuntimeError::Validation(format!("program.wasm 实例化失败：{e}")))?
            .start(&mut store)
            .map_err(|e| RuntimeError::Validation(format!("program.wasm start 失败：{e}")))?;
        let memory = instance
            .get_memory(&store, "memory")
            .ok_or_else(|| RuntimeError::Validation("program.wasm 未导出 memory".into()))?;

        let reset = instance
            .get_typed_func::<(), ()>(&store, "sophia_reset")
            .map_err(|e| RuntimeError::Validation(format!("缺少 sophia_reset 导出：{e}")))?;
        reset
            .call(&mut store, ())
            .map_err(|e| RuntimeError::Validation(format!("sophia_reset trap：{e}")))?;

        let mut handles = Vec::with_capacity(args.len());
        for arg in args {
            handles.push(write_arg_value(&mut store, &instance, &memory, arg)?);
        }

        let prefix = if entry_is_action {
            "action_"
        } else {
            "transition_"
        };
        let fname = format!("{prefix}{entry}");
        let outcome_handle = call_entry(&mut store, &instance, &fname, &handles)?;
        let outcome = read_outcome(&mut store, &instance, &memory, outcome_handle)?;
        validate_output(model, entry, &outcome)?;
        Ok(outcome)
    }
}

fn validate_inputs(model: &SemanticModel, entry: &str, args: &[Value]) -> RuntimeResult<()> {
    let decl = model
        .callables
        .get(entry)
        .ok_or_else(|| RuntimeError::Validation(format!("`{entry}` 不在语义模型中")))?;
    if args.len() != decl.inputs.len() {
        return Err(RuntimeError::Validation(format!(
            "`{entry}` 期望 {} 个实参，得到 {}",
            decl.inputs.len(),
            args.len()
        )));
    }
    for ((pname, pty), arg) in decl.inputs.iter().zip(args) {
        crate::validate::check_value(arg, pty, model)
            .map_err(|e| RuntimeError::Validation(format!("input `{pname}`：{e}")))?;
    }
    Ok(())
}

fn validate_output(model: &SemanticModel, entry: &str, outcome: &Outcome) -> RuntimeResult<()> {
    let decl = model
        .callables
        .get(entry)
        .ok_or_else(|| RuntimeError::Validation(format!("`{entry}` 不在语义模型中")))?;
    if let Outcome::Returned(v) = outcome {
        if let Some(out_ty) = decl.sole_output_ty() {
            crate::validate::check_value(v, out_ty, model)
                .map_err(|e| RuntimeError::Validation(format!("output：{e}")))?;
        }
    }
    Ok(())
}

fn link_host(linker: &mut Linker<RunnerState<'_>>, ops: &[HostImport]) -> RuntimeResult<()> {
    linker
        .func_wrap(
            "sophia_host",
            "console_write",
            |mut caller: Caller<'_, RunnerState<'_>>, ptr: i32, len: i32| {
                let text = read_caller_string(&mut caller, ptr, len)?;
                caller.data_mut().host.console_write(&text);
                Ok(())
            },
        )
        .map_err(|e| RuntimeError::Validation(format!("注册 console_write import 失败：{e}")))?;
    linker
        .func_wrap(
            "sophia_host",
            "read_copy",
            |mut caller: Caller<'_, RunnerState<'_>>, dst: i32| {
                if dst < 0 {
                    return Err(wasmi::Error::new("read_copy 目标指针为负数"));
                }
                let bytes = std::mem::take(&mut caller.data_mut().stash);
                let mem = caller
                    .get_export("memory")
                    .and_then(|e| e.into_memory())
                    .ok_or_else(|| wasmi::Error::new("program.wasm 未导出 memory"))?;
                mem.write(&mut caller, dst as usize, &bytes)
                    .map_err(|e| wasmi::Error::new(format!("read_copy 写内存失败：{e}")))?;
                Ok(())
            },
        )
        .map_err(|e| RuntimeError::Validation(format!("注册 read_copy import 失败：{e}")))?;

    for import in ops {
        let family = import.family.clone();
        let op = import.op.clone();
        linker
            .func_wrap(
                &import.module,
                &import.name,
                move |mut caller: Caller<'_, RunnerState<'_>>,
                      ptr: i32,
                      len: i32|
                      -> Result<i32, wasmi::Error> {
                    let bytes = read_caller_bytes(&mut caller, ptr, len)?;
                    let args = crate::value_wire::decode_args(&bytes).map_err(wasmi::Error::new)?;
                    let result = caller
                        .data_mut()
                        .host
                        .call(&family, &op, &args)
                        .map_err(wasmi::Error::new)?;
                    caller.data_mut().stash =
                        crate::value_wire::encode_value(&result).map_err(wasmi::Error::new)?;
                    Ok(caller.data().stash.len() as i32)
                },
            )
            .map_err(|e| {
                RuntimeError::Validation(format!(
                    "注册 {}.{} import 失败：{e}",
                    import.module, import.name
                ))
            })?;
    }
    Ok(())
}

fn read_caller_bytes(
    caller: &mut Caller<'_, RunnerState<'_>>,
    ptr: i32,
    len: i32,
) -> Result<Vec<u8>, wasmi::Error> {
    if ptr < 0 || len < 0 {
        return Err(wasmi::Error::new("负数内存范围"));
    }
    let mem = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| wasmi::Error::new("program.wasm 未导出 memory"))?;
    let mut buf = vec![0u8; len as usize];
    mem.read(caller, ptr as usize, &mut buf)
        .map_err(|e| wasmi::Error::new(format!("读取 WASM 内存失败：{e}")))?;
    Ok(buf)
}

fn read_caller_string(
    caller: &mut Caller<'_, RunnerState<'_>>,
    ptr: i32,
    len: i32,
) -> Result<String, wasmi::Error> {
    let bytes = read_caller_bytes(caller, ptr, len)?;
    String::from_utf8(bytes).map_err(|e| wasmi::Error::new(format!("UTF-8 解码失败：{e}")))
}

fn write_arg_value(
    store: &mut Store<RunnerState<'_>>,
    instance: &wasmi::Instance,
    memory: &wasmi::Memory,
    value: &Value,
) -> RuntimeResult<i32> {
    match value {
        Value::Unit => instance
            .get_typed_func::<(), i32>(&*store, "sophia_make_unit")
            .map_err(|e| RuntimeError::Validation(format!("缺少 sophia_make_unit 导出：{e}")))?
            .call(store, ())
            .map_err(|e| RuntimeError::Validation(format!("sophia_make_unit trap：{e}"))),
        Value::Bool(b) => instance
            .get_typed_func::<i32, i32>(&*store, "sophia_make_bool")
            .map_err(|e| RuntimeError::Validation(format!("缺少 sophia_make_bool 导出：{e}")))?
            .call(store, i32::from(*b))
            .map_err(|e| RuntimeError::Validation(format!("sophia_make_bool trap：{e}"))),
        Value::Int(i) => instance
            .get_typed_func::<i64, i32>(&*store, "sophia_make_int")
            .map_err(|e| RuntimeError::Validation(format!("缺少 sophia_make_int 导出：{e}")))?
            .call(store, *i)
            .map_err(|e| RuntimeError::Validation(format!("sophia_make_int trap：{e}"))),
        Value::Null => instance
            .get_typed_func::<(), i32>(&*store, "sophia_make_null")
            .map_err(|e| RuntimeError::Validation(format!("缺少 sophia_make_null 导出：{e}")))?
            .call(store, ())
            .map_err(|e| RuntimeError::Validation(format!("sophia_make_null trap：{e}"))),
        Value::Text(text) => {
            let (ptr, len) = write_bytes(store, instance, memory, text.as_bytes())?;
            instance
                .get_typed_func::<(i32, i32), i32>(&*store, "sophia_make_text")
                .map_err(|e| RuntimeError::Validation(format!("缺少 sophia_make_text 导出：{e}")))?
                .call(store, (ptr, len))
                .map_err(|e| RuntimeError::Validation(format!("sophia_make_text trap：{e}")))
        }
        Value::State { state, value } => {
            let (sp, sl) = write_bytes(store, instance, memory, state.as_bytes())?;
            let (vp, vl) = write_bytes(store, instance, memory, value.as_bytes())?;
            instance
                .get_typed_func::<(i32, i32, i32, i32), i32>(&*store, "sophia_make_state")
                .map_err(|e| RuntimeError::Validation(format!("缺少 sophia_make_state 导出：{e}")))?
                .call(store, (sp, sl, vp, vl))
                .map_err(|e| RuntimeError::Validation(format!("sophia_make_state trap：{e}")))
        }
        other => Err(RuntimeError::Validation(format!(
            "WASM runner 暂不支持该入参值：{other:?}"
        ))),
    }
}

fn write_bytes(
    store: &mut Store<RunnerState<'_>>,
    instance: &wasmi::Instance,
    memory: &wasmi::Memory,
    bytes: &[u8],
) -> RuntimeResult<(i32, i32)> {
    let len = checked_i32_len(bytes.len(), "入参")?;
    let alloc = instance
        .get_typed_func::<i32, i32>(&*store, "sophia_alloc")
        .map_err(|e| RuntimeError::Validation(format!("缺少 sophia_alloc 导出：{e}")))?;
    let ptr = alloc
        .call(&mut *store, len)
        .map_err(|e| RuntimeError::Validation(format!("sophia_alloc trap：{e}")))?;
    if ptr < 0 {
        return Err(RuntimeError::Validation("sophia_alloc 返回负数指针".into()));
    }
    memory
        .write(store, ptr as usize, bytes)
        .map_err(|e| RuntimeError::Validation(format!("写入 WASM 内存失败：{e}")))?;
    Ok((ptr, len))
}

fn checked_i32_len(len: usize, role: &str) -> RuntimeResult<i32> {
    i32::try_from(len)
        .map_err(|_| RuntimeError::Validation(format!("WASM runner {role}字节长度超过 i32")))
}

fn call_entry(
    store: &mut Store<RunnerState<'_>>,
    instance: &wasmi::Instance,
    fname: &str,
    handles: &[i32],
) -> RuntimeResult<i32> {
    match handles.len() {
        0 => call_typed::<()>(store, instance, fname, ()),
        1 => call_typed::<i32>(store, instance, fname, handles[0]),
        2 => call_typed::<(i32, i32)>(store, instance, fname, (handles[0], handles[1])),
        3 => call_typed::<(i32, i32, i32)>(
            store,
            instance,
            fname,
            (handles[0], handles[1], handles[2]),
        ),
        4 => call_typed::<(i32, i32, i32, i32)>(
            store,
            instance,
            fname,
            (handles[0], handles[1], handles[2], handles[3]),
        ),
        n => Err(RuntimeError::Validation(format!(
            "WASM runner 暂支持 0..=4 个入口参数，实际 {n}"
        ))),
    }
}

fn call_typed<Params>(
    store: &mut Store<RunnerState<'_>>,
    instance: &wasmi::Instance,
    fname: &str,
    args: Params,
) -> RuntimeResult<i32>
where
    Params: wasmi::WasmParams,
{
    instance
        .get_typed_func::<Params, i32>(&*store, fname)
        .map_err(|e| RuntimeError::Validation(format!("缺少 `{fname}` 导出或签名不符：{e}")))?
        .call(store, args)
        .map_err(|e| RuntimeError::Validation(format!("`{fname}` 执行 trap：{e}")))
}

fn read_outcome(
    store: &mut Store<RunnerState<'_>>,
    instance: &wasmi::Instance,
    memory: &wasmi::Memory,
    outcome: i32,
) -> RuntimeResult<Outcome> {
    let outcome_kind = instance
        .get_typed_func::<i32, i32>(&*store, "sophia_outcome_kind")
        .map_err(|e| RuntimeError::Validation(format!("缺少 sophia_outcome_kind 导出：{e}")))?;
    let outcome_value = instance
        .get_typed_func::<i32, i32>(&*store, "sophia_outcome_value")
        .map_err(|e| RuntimeError::Validation(format!("缺少 sophia_outcome_value 导出：{e}")))?;
    let kind = outcome_kind
        .call(&mut *store, outcome)
        .map_err(|e| RuntimeError::Validation(format!("sophia_outcome_kind trap：{e}")))?;
    let value_handle = outcome_value
        .call(&mut *store, outcome)
        .map_err(|e| RuntimeError::Validation(format!("sophia_outcome_value trap：{e}")))?;
    let value = read_value(&*store, memory, value_handle)?;
    match kind {
        OUTCOME_RETURNED => Ok(Outcome::Returned(value)),
        OUTCOME_RAISED => match value {
            Value::ErrorValue { variant, fields } => {
                Ok(Outcome::Raised(RaisedError { variant, fields }))
            }
            other => Err(RuntimeError::Validation(format!(
                "Raised outcome 的 value 不是 ErrorValue：{other:?}"
            ))),
        },
        other => Err(RuntimeError::Validation(format!(
            "未知 outcome kind：{other}"
        ))),
    }
}

fn read_value(
    store: &Store<RunnerState<'_>>,
    memory: &wasmi::Memory,
    handle: i32,
) -> RuntimeResult<Value> {
    let data = memory.data(store);
    let base = checked_base(handle, data.len())?;
    match read_i32(data, base + OFF_TAG)? {
        TAG_UNIT => Ok(Value::Unit),
        TAG_BOOL => Ok(Value::Bool(read_i32(data, base + OFF_BOOL_PAYLOAD)? != 0)),
        TAG_INT => Ok(Value::Int(read_i64(data, base + OFF_INT_PAYLOAD)?)),
        TAG_TEXT => {
            let ptr = read_i32(data, base + OFF_TEXT_PTR)?;
            let len = read_i32(data, base + OFF_TEXT_LEN)?;
            Ok(Value::Text(read_string(data, ptr, len)?))
        }
        TAG_NULL => Ok(Value::Null),
        TAG_ERROR_VALUE => {
            let (name, fields) = read_record(store, memory, base)?;
            Ok(Value::ErrorValue {
                variant: name,
                fields,
            })
        }
        TAG_ENTITY => {
            let (name, fields) = read_record(store, memory, base)?;
            Ok(Value::Entity { name, fields })
        }
        TAG_STATE => {
            let state = read_string(
                data,
                read_i32(data, base + OFF_STATE_NAME_PTR)?,
                read_i32(data, base + OFF_STATE_NAME_LEN)?,
            )?;
            let value = read_string(
                data,
                read_i32(data, base + OFF_STATE_VALUE_PTR)?,
                read_i32(data, base + OFF_STATE_VALUE_LEN)?,
            )?;
            Ok(Value::State { state, value })
        }
        tag => Err(RuntimeError::Validation(format!(
            "未知 WASM value tag：{tag}"
        ))),
    }
}

fn read_record(
    store: &Store<RunnerState<'_>>,
    memory: &wasmi::Memory,
    base: usize,
) -> RuntimeResult<(String, BTreeMap<String, Value>)> {
    let data = memory.data(store);
    let name = read_string(
        data,
        read_i32(data, base + OFF_REC_NAME_PTR)?,
        read_i32(data, base + OFF_REC_NAME_LEN)?,
    )?;
    let nfields = read_i32(data, base + OFF_REC_NFIELDS)?;
    if nfields < 0 {
        return Err(RuntimeError::Validation("记录字段数为负".into()));
    }
    let mut fields = BTreeMap::new();
    for i in 0..nfields as usize {
        let field_base = base + REC_HEADER_SIZE + i * REC_FIELD_SIZE;
        let key = read_string(
            data,
            read_i32(data, field_base + OFF_FIELD_KEY_PTR)?,
            read_i32(data, field_base + OFF_FIELD_KEY_LEN)?,
        )?;
        let value_handle = read_i32(data, field_base + OFF_FIELD_VAL)?;
        fields.insert(key, read_value(store, memory, value_handle)?);
    }
    Ok((name, fields))
}

fn checked_base(handle: i32, len: usize) -> RuntimeResult<usize> {
    if handle < 0 {
        return Err(RuntimeError::Validation(format!(
            "负数 WASM handle：{handle}"
        )));
    }
    let base = handle as usize;
    if base >= len {
        return Err(RuntimeError::Validation(format!(
            "WASM handle 越界：{handle}"
        )));
    }
    Ok(base)
}

fn read_i32(data: &[u8], off: usize) -> RuntimeResult<i32> {
    let bytes = data
        .get(off..off + 4)
        .ok_or_else(|| RuntimeError::Validation(format!("读取 i32 越界：{off}")))?;
    Ok(i32::from_le_bytes(bytes.try_into().unwrap()))
}

fn read_i64(data: &[u8], off: usize) -> RuntimeResult<i64> {
    let bytes = data
        .get(off..off + 8)
        .ok_or_else(|| RuntimeError::Validation(format!("读取 i64 越界：{off}")))?;
    Ok(i64::from_le_bytes(bytes.try_into().unwrap()))
}

fn read_string(data: &[u8], ptr: i32, len: i32) -> RuntimeResult<String> {
    if ptr < 0 || len < 0 {
        return Err(RuntimeError::Validation("字符串内存范围为负".into()));
    }
    let start = ptr as usize;
    let end = start
        .checked_add(len as usize)
        .ok_or_else(|| RuntimeError::Validation("字符串内存范围溢出".into()))?;
    let bytes = data
        .get(start..end)
        .ok_or_else(|| RuntimeError::Validation("字符串内存范围越界".into()))?;
    String::from_utf8(bytes.to_vec())
        .map_err(|e| RuntimeError::Validation(format!("字符串 UTF-8 解码失败：{e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_i32_len_rejects_overflow() {
        assert_eq!(
            checked_i32_len(i32::MAX as usize, "测试").unwrap(),
            i32::MAX
        );
        assert!(matches!(
            checked_i32_len(i32::MAX as usize + 1, "测试"),
            Err(RuntimeError::Validation(msg)) if msg.contains("超过 i32")
        ));
    }
}
