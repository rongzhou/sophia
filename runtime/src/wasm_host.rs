//! 三方 WASM 库 host：解释模式经 `wasmi` 加载 `host.wasm` 执行库 op（路线 B 的 [`HostFn`] 实现）。
//!
//! 见 docs/stdlib_design.md §五.3/§五.4/§六.1。三方库的 effect-op 由一个
//! `host.wasm` 模块实现:它 **export** 与清单 `host_fn` 同名的函数,经统一字节级 ABI（复用 codegen 的
//! `sophia_host` 契约,方向相反——codegen module import、host.wasm export）。
//!
//! **本 demo 的 ABI 子集**:op 签名 `(Int, Int) -> Int`,标量 i64 直传（不入线性内存）。完整字节协议
//! （文本经 ptr/len + stash + read_copy）继承自 codegen W4,本 host 不涉及。
//!
//! **诚实红线**:实例化失败 / 导出缺失 / 签名不符 / wasm trap 一律 `Err`,解释器物化为硬错误阻断,
//! 绝不伪造。

use crate::host::HostFn;
use crate::value::Value;
use wasmi::{Engine, Instance, Linker, Module, Store, TypedFunc};

/// 一个三方 WASM 库 op 的解释模式 host:持 `host.wasm` 实例 + 一个 `(i64,i64)->i64` 导出函数句柄。
///
/// 本 demo 形态（标量 i64 直传）。更复杂签名（文本经线性内存）随需求扩展时,在此按统一 ABI 增分派。
pub struct WasmHostFn {
    store: Store<()>,
    func: TypedFunc<(i64, i64), i64>,
    /// 导出函数名（诊断用）。
    export: String,
}

impl WasmHostFn {
    /// 从 `host.wasm` 字节 + 导出函数名构建（`(i64,i64)->i64` 签名）。导出缺失 / 签名不符即 `Err`。
    pub fn new_i64_i64_i64(wasm_bytes: &[u8], export: &str) -> Result<Self, String> {
        let engine = Engine::default();
        let module =
            Module::new(&engine, wasm_bytes).map_err(|e| format!("host.wasm 加载失败:{e}"))?;
        let mut store = Store::new(&engine, ());
        // 本 demo host 无 import（纯计算);用空 linker 实例化。
        let linker = Linker::new(&engine);
        let instance: Instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| format!("host.wasm 实例化失败:{e}"))?
            .start(&mut store)
            .map_err(|e| format!("host.wasm start 失败:{e}"))?;
        let func = instance
            .get_typed_func::<(i64, i64), i64>(&store, export)
            .map_err(|e| format!("host.wasm 缺导出 `{export}` 或签名不符 (i64,i64)->i64:{e}"))?;
        Ok(WasmHostFn {
            store,
            func,
            export: export.to_string(),
        })
    }
}

impl HostFn for WasmHostFn {
    fn call(&mut self, args: &[Value]) -> Result<Value, String> {
        let a = match args.first() {
            Some(Value::Int(i)) => *i,
            other => return Err(format!("{} 第 1 实参应为 Int,实际 {other:?}", self.export)),
        };
        let b = match args.get(1) {
            Some(Value::Int(i)) => *i,
            other => return Err(format!("{} 第 2 实参应为 Int,实际 {other:?}", self.export)),
        };
        let r = self
            .func
            .call(&mut self.store, (a, b))
            .map_err(|e| format!("{} wasm 执行 trap:{e}", self.export))?;
        Ok(Value::Int(r))
    }
}
