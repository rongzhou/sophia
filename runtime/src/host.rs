//! Effect host：路线 B 的**开放注册表**（见 docs/stdlib_design.md）。
//!
//! 解释器把库的 effect op（`Lib.Op(args)`，如 `File.Read` / `Http.Get`）委派给宿主实现。与早期
//! 固定方法集 trait（`EffectHost::http_get` 等）不同，本层是一张 **`(family, op) → Box<dyn HostFn>`
//! 注册表**——runtime **不认识任何具体库**，库的 host 实现由上层（`sophia-stdlib` 的 native / mock、
//! 三方的 WASM）注册进来。这是「库不渗透语言核心」在运行时的落点，也使 native 与 WASM host 同构为
//! `Box<dyn HostFn>`（跨解释 / VM 两模式对称）。
//!
//! `Console`（`print`）是**例外**：它是语言内置输出原语（非库 op、非特殊根 method_call），由
//! [`HostRegistry::console_write`] 单独捕获，不经 ops 表。
//!
//! **诚实性红线**：host 实现失败（mock 未命中 / 真实 IO 失败 / wasm trap）一律返回 `Err`，解释器
//! 物化为硬错误阻断，**绝不伪造成功**。

use crate::value::Value;
use std::collections::BTreeMap;

/// 一个库 effect op 的宿主实现。`call` 收实参值、返回结果值或诚实 `Err`。
///
/// args ABI 由各 op 契约约定（如 `File.Write(path: Text, content: Text) -> Unit`）；实现自行从
/// `args` 取值。runtime 不校验 args 形状（语义层已检查），但实现应对意外输入诚实 `Err`。
pub trait HostFn {
    fn call(&mut self, args: &[Value]) -> Result<Value, String>;
}

/// 把一个 `FnMut(&[Value]) -> Result<Value, String>` 闭包包成 [`HostFn`]（便于上层用闭包注册）。
/// crate 内部用——上层经 [`HostRegistry::register_fn`] 注册闭包，不直接命名本类型。
pub(crate) struct FnHost<F>(pub F);

impl<F> HostFn for FnHost<F>
where
    F: FnMut(&[Value]) -> Result<Value, String>,
{
    fn call(&mut self, args: &[Value]) -> Result<Value, String> {
        (self.0)(args)
    }
}

/// Effect 宿主注册表：`(family, op) → HostFn` + console 捕获。
///
/// 解释器持有它，调用 `Lib.Op(args)` 时经 [`Self::call`] 委派；`print` 经 [`Self::console_write`]。
/// 空注册表（[`Self::new`]）仅支持 `Console`（语言内置）——纯逻辑 / Console 程序无需任何库 host。
/// 库 host 由上层注册（标准库 native / mock、三方 WASM），runtime 不内置任何具体库。
#[derive(Default)]
pub struct HostRegistry {
    /// 捕获的 console 行，按写入顺序（`Console.Write` / `print` 的输出）。
    pub console: Vec<String>,
    /// 库 effect op 的宿主实现表。
    ops: BTreeMap<(String, String), Box<dyn HostFn>>,
}

impl std::fmt::Debug for HostRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostRegistry")
            .field("console", &self.console)
            .field("ops", &self.ops.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl HostRegistry {
    /// 空注册表（仅 Console；无库 op）。
    pub fn new() -> Self {
        HostRegistry::default()
    }

    /// 注册一个库 effect op 的宿主实现（`family.op` → host）。重复注册覆盖。
    pub fn register(
        &mut self,
        family: impl Into<String>,
        op: impl Into<String>,
        host: Box<dyn HostFn>,
    ) {
        self.ops.insert((family.into(), op.into()), host);
    }

    /// 便捷：用闭包注册一个 host op。
    pub fn register_fn<F>(&mut self, family: impl Into<String>, op: impl Into<String>, f: F)
    where
        F: FnMut(&[Value]) -> Result<Value, String> + 'static,
    {
        self.register(family, op, Box::new(FnHost(f)));
    }

    /// 处理 `Console.Write`（`print`）：捕获到 console 行。
    pub fn console_write(&mut self, text: &str) {
        self.console.push(text.to_string());
    }

    /// 该 `(family, op)` 是否有注册的 host（解释器据此判定特殊根 effect op 调用）。
    pub fn has_op(&self, family: &str, op: &str) -> bool {
        self.ops.contains_key(&(family.to_string(), op.to_string()))
    }

    /// 委派调用某库 op。未注册 → 诚实 `Err`（不伪造）。
    pub fn call(&mut self, family: &str, op: &str, args: &[Value]) -> Result<Value, String> {
        match self.ops.get_mut(&(family.to_string(), op.to_string())) {
            Some(host) => host.call(args),
            None => Err(format!(
                "无 host 实现：`{family}.{op}`（未注册库 host；runtime 不内置具体库）"
            )),
        }
    }
}
