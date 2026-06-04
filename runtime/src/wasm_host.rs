//! 三方 WASM 库 host：解释模式经 `wasmi` 加载 `host.wasm` 执行库 op（路线 B 的 [`HostFn`] 实现）。
//!
//! 见 docs/stdlib_design.md §五.3/§五.4/§六.1。三方库的 effect-op 由一个
//! `host.wasm` 模块实现:它 **export** 与清单 `host_fn` 同名的函数,经统一字节级 ABI（复用 codegen 的
//! `sophia_host` 契约,方向相反——codegen module import、host.wasm export）。
//!
//! **诚实红线**:实例化失败 / 导出缺失 / 签名不符 / wasm trap 一律 `Err`,解释器物化为硬错误阻断,
//! 绝不伪造。

use crate::host::HostFn;
use crate::value::Value;
use sophia_library::OpContract;
use wasmi::{Engine, Instance, Linker, Memory, Module, Store, TypedFunc};

/// 一个三方 WASM 库 op 的解释模式 host。
pub struct WasmHostFn {
    store: Store<()>,
    memory: Memory,
    alloc: TypedFunc<i32, i32>,
    read_copy: TypedFunc<i32, ()>,
    func: TypedFunc<(i32, i32), i32>,
    contract: OpContract,
    /// 导出函数名（诊断用）。
    export: String,
}

impl WasmHostFn {
    /// 从 `host.wasm` 字节 + op 契约构建 ValueWire provider。
    ///
    /// provider 必须 export:
    /// `memory`、`sophia_alloc(i32)->i32`、`sophia_read_copy(i32)`、`host_fn(i32,i32)->i32`。
    pub fn new(wasm_bytes: &[u8], contract: &OpContract) -> Result<Self, String> {
        let engine = Engine::default();
        let module =
            Module::new(&engine, wasm_bytes).map_err(|e| format!("host.wasm 加载失败:{e}"))?;
        let mut store = Store::new(&engine, ());
        // provider 通过自身 export 的线性内存通信；MVP 不给 provider 注入 imports。
        let linker = Linker::new(&engine);
        let instance: Instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| format!("host.wasm 实例化失败:{e}"))?
            .start(&mut store)
            .map_err(|e| format!("host.wasm start 失败:{e}"))?;
        let memory = instance
            .get_memory(&store, "memory")
            .ok_or_else(|| "host.wasm 缺导出 `memory`".to_string())?;
        let alloc = instance
            .get_typed_func::<i32, i32>(&store, "sophia_alloc")
            .map_err(|e| format!("host.wasm 缺导出 `sophia_alloc(i32)->i32` 或签名不符:{e}"))?;
        let read_copy = instance
            .get_typed_func::<i32, ()>(&store, "sophia_read_copy")
            .map_err(|e| format!("host.wasm 缺导出 `sophia_read_copy(i32)` 或签名不符:{e}"))?;
        let func = instance
            .get_typed_func::<(i32, i32), i32>(&store, &contract.host_fn)
            .map_err(|e| {
                format!(
                    "host.wasm 缺导出 `{}` 或签名不符 (i32,i32)->i32:{e}",
                    contract.host_fn
                )
            })?;
        Ok(WasmHostFn {
            store,
            memory,
            alloc,
            read_copy,
            func,
            contract: contract.clone(),
            export: contract.host_fn.clone(),
        })
    }

    fn call_provider(&mut self, args_wire: &[u8]) -> Result<Vec<u8>, String> {
        let args_len = checked_i32_len(args_wire.len(), "实参")?;
        let args_ptr = self
            .alloc
            .call(&mut self.store, args_len)
            .map_err(|e| format!("{} sophia_alloc(args) trap:{e}", self.export))?;
        if args_ptr < 0 {
            return Err(format!("{} sophia_alloc(args) 返回负指针", self.export));
        }
        self.memory
            .write(&mut self.store, args_ptr as usize, args_wire)
            .map_err(|e| format!("{} 写入 provider memory 失败:{e}", self.export))?;

        let result_len = self
            .func
            .call(&mut self.store, (args_ptr, args_len))
            .map_err(|e| format!("{} wasm 执行 trap:{e}", self.export))?;
        if result_len < 0 {
            return Err(format!("{} 返回负数 result_len", self.export));
        }
        let result_ptr = self
            .alloc
            .call(&mut self.store, result_len)
            .map_err(|e| format!("{} sophia_alloc(result) trap:{e}", self.export))?;
        if result_ptr < 0 {
            return Err(format!("{} sophia_alloc(result) 返回负指针", self.export));
        }
        self.read_copy
            .call(&mut self.store, result_ptr)
            .map_err(|e| format!("{} sophia_read_copy trap:{e}", self.export))?;
        let mut out = vec![0u8; result_len as usize];
        self.memory
            .read(&self.store, result_ptr as usize, &mut out)
            .map_err(|e| format!("{} 读取 provider result 失败:{e}", self.export))?;
        Ok(out)
    }
}

impl HostFn for WasmHostFn {
    fn call(&mut self, args: &[Value]) -> Result<Value, String> {
        let args_wire = crate::value_wire::encode_args(args, &self.contract.params)?;
        let result_wire = self.call_provider(&args_wire)?;
        crate::value_wire::decode_typed_value(&result_wire, &self.contract.returns)
    }
}

fn checked_i32_len(len: usize, role: &str) -> Result<i32, String> {
    i32::try_from(len).map_err(|_| format!("ValueWire {role}字节长度超过 i32"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sophia_library::{OpContract, Scalar, TypeDesc};

    #[test]
    fn value_wire_provider_supports_text_signature() {
        let contract = OpContract {
            lib: "text_wasm".into(),
            family: "TextWasm".into(),
            op: "Echo".into(),
            params: vec![TypeDesc::Scalar(Scalar::Text)],
            returns: TypeDesc::Scalar(Scalar::Text),
            effectful: false,
            host_fn: "text_echo".into(),
        };
        let mut host = WasmHostFn::new(&text_provider_wasm(), &contract).expect("provider");
        let out = host
            .call(&[Value::Text("ignored".into())])
            .expect("call provider");
        assert_eq!(out, Value::Text("ok".into()));
    }

    fn text_provider_wasm() -> Vec<u8> {
        use wasm_encoder::{
            CodeSection, ConstExpr, ExportKind, ExportSection, Function, FunctionSection,
            GlobalSection, GlobalType, Instruction, MemorySection, MemoryType, Module, TypeSection,
            ValType,
        };

        let mut module = Module::new();
        let mut types = TypeSection::new();
        types.ty().function([ValType::I32], [ValType::I32]);
        types.ty().function([ValType::I32], []);
        types
            .ty()
            .function([ValType::I32, ValType::I32], [ValType::I32]);
        module.section(&types);

        let mut funcs = FunctionSection::new();
        funcs.function(0);
        funcs.function(1);
        funcs.function(2);
        module.section(&funcs);

        let mut mems = MemorySection::new();
        mems.memory(MemoryType {
            minimum: 1,
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        });
        module.section(&mems);

        let mut globals = GlobalSection::new();
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: true,
                shared: false,
            },
            &ConstExpr::i32_const(1024),
        );
        module.section(&globals);

        let mut exports = ExportSection::new();
        exports.export("memory", ExportKind::Memory, 0);
        exports.export("sophia_alloc", ExportKind::Func, 0);
        exports.export("sophia_read_copy", ExportKind::Func, 1);
        exports.export("text_echo", ExportKind::Func, 2);
        module.section(&exports);

        let mut code = CodeSection::new();
        let mut alloc = Function::new(vec![(1u32, ValType::I32)]);
        alloc.instruction(&Instruction::GlobalGet(0));
        alloc.instruction(&Instruction::LocalSet(1));
        alloc.instruction(&Instruction::GlobalGet(0));
        alloc.instruction(&Instruction::LocalGet(0));
        alloc.instruction(&Instruction::I32Add);
        alloc.instruction(&Instruction::GlobalSet(0));
        alloc.instruction(&Instruction::LocalGet(1));
        alloc.instruction(&Instruction::End);
        code.function(&alloc);

        let mut read_copy = Function::new(vec![]);
        for i in 0..7 {
            read_copy.instruction(&Instruction::LocalGet(0));
            read_copy.instruction(&Instruction::I32Const(i));
            read_copy.instruction(&Instruction::I32Add);
            read_copy.instruction(&Instruction::I32Const(8 + i));
            read_copy.instruction(&Instruction::I32Load8U(mem(0, 0)));
            read_copy.instruction(&Instruction::I32Store8(mem(0, 0)));
        }
        read_copy.instruction(&Instruction::End);
        code.function(&read_copy);

        let mut echo = Function::new(vec![]);
        echo.instruction(&Instruction::I32Const(8));
        echo.instruction(&Instruction::I32Const(3)); // Text tag
        echo.instruction(&Instruction::I32Store8(mem(0, 0)));
        echo.instruction(&Instruction::I32Const(9));
        echo.instruction(&Instruction::I32Const(2)); // len
        echo.instruction(&Instruction::I32Store(mem(0, 0)));
        echo.instruction(&Instruction::I32Const(13));
        echo.instruction(&Instruction::I32Const(b'o' as i32));
        echo.instruction(&Instruction::I32Store8(mem(0, 0)));
        echo.instruction(&Instruction::I32Const(14));
        echo.instruction(&Instruction::I32Const(b'k' as i32));
        echo.instruction(&Instruction::I32Store8(mem(0, 0)));
        echo.instruction(&Instruction::I32Const(7));
        echo.instruction(&Instruction::End);
        code.function(&echo);
        module.section(&code);
        module.finish()
    }

    fn mem(offset: u64, align: u32) -> wasm_encoder::MemArg {
        wasm_encoder::MemArg {
            offset,
            align,
            memory_index: 0,
        }
    }
}
