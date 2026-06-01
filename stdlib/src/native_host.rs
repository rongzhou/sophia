//! 标准库 effect op 的 host 实现：真实（native）+ 确定性 mock。
//!
//! 路线 B（见 docs/stdlib_design.md §5.3）：库 host 是注册进 [`HostRegistry`] 的 `(family, op)`
//! 闭包。本模块提供标准库 `Http` / `File` 的两类实现：
//! - **native**（[`register_native_hosts`]）：真实 `reqwest::blocking` / `std::fs`（CLI 真实执行）；
//! - **mock**（[`register_mock_hosts`] / [`mock_host`]）：确定性内存桶（测试 / 差测试），未命中诚实 `Err`。
//!
//! **诚实性红线**：native 失败（网络非 2xx / 超时 / 文件不存在 / 非 UTF-8）与 mock 未命中一律返回
//! `Err`，解释器物化为硬错误阻断，**绝不伪造成功 / 编造默认响应**。

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Duration;

use sophia_library::{LibraryRegistry, OpContract, Scalar, TypeDesc};
use sophia_runtime::{HostRegistry, Value, WasmHostFn};

/// `Http.Get` 真实 host 的固定超时（秒）。挂死的连接快速失败而非无限等待。
const HTTP_TIMEOUT_SECS: u64 = 10;

/// 从实参取一个 `Text`（库 op 的 path / url / content 参数）。
fn text_arg(args: &[Value], idx: usize, op: &str, role: &str) -> Result<String, String> {
    match args.get(idx) {
        Some(Value::Text(s)) => Ok(s.clone()),
        Some(other) => Err(format!("{op} 的 {role} 应为 Text，实际 {other}")),
        None => Err(format!("{op} 缺少 {role} 实参")),
    }
}

/// 把标准库 op 的**真实** host 注册进给定注册表（CLI 真实执行用）。
///
/// `Http.Get` → 真实 `reqwest::blocking` GET；`File.Read`/`File.Write` → 真实 `std::fs`。
/// 真实失败一律诚实 `Err`。`Console`（`print`）不在此（语言内置，由 `HostRegistry::console_write`
/// 直接捕获）。
pub fn register_native_hosts(host: &mut HostRegistry) {
    // Http.Get：真实网络（blocking）。client 经 Rc 在闭包间共享、复用连接池。
    let client = Rc::new(
        reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
            .build()
            .expect("构建 HTTP client"),
    );
    host.register_fn("Http", "Get", move |args| {
        let url = text_arg(args, 0, "Http.Get", "url")?;
        let resp = client
            .get(&url)
            .send()
            .map_err(|e| format!("Http.Get(\"{url}\") 请求失败：{e}"))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(format!("Http.Get(\"{url}\") 非 2xx 状态：{status}"));
        }
        let body = resp
            .text()
            .map_err(|e| format!("Http.Get(\"{url}\") 读取响应体失败：{e}"))?;
        Ok(Value::Text(body))
    });

    // File.Read：真实读取（不存在 / 无权限 / 非 UTF-8 一律诚实 Err，不伪造内容）。
    host.register_fn("File", "Read", |args| {
        let path = text_arg(args, 0, "File.Read", "path")?;
        std::fs::read_to_string(&path)
            .map(Value::Text)
            .map_err(|e| format!("File.Read(\"{path}\") 失败：{e}"))
    });

    // File.Write：真实写入（覆盖）。
    host.register_fn("File", "Write", |args| {
        let path = text_arg(args, 0, "File.Write", "path")?;
        let content = text_arg(args, 1, "File.Write", "content")?;
        std::fs::write(&path, content)
            .map(|()| Value::Unit)
            .map_err(|e| format!("File.Write(\"{path}\") 失败：{e}"))
    });
}

/// 为注册表里全部**三方 WASM 库**注册 host（经 [`WasmHostFn`] 加载各库 `host.wasm`）。
///
/// 区分准则 = **装载方式**（见 docs/stdlib_design.md §五.3）：注册表里携带 `host.wasm` 字节的库
/// 即三方 WASM-effect 库,其每个 effect-op 由 host.wasm 对应导出函数（清单 `host_fn`）实现;标准库
/// （`File`/`Http`,无 host.wasm）的 native host 由 [`register_native_hosts`] 注册,二者互补不重叠。
///
/// 本 demo ABI 子集:op 签名 `(Int, Int) -> Int`,标量 i64 直传（见 `WasmHostFn`）。遇到超出该子集
/// 的签名（含 `Text` / `Unit` / intent 包装）一律诚实 `Err`——不静默跳过、不伪造 host,待 ABI 随需
/// 扩展后再支持。host.wasm 加载 / 导出缺失 / 签名不符同样 `Err`,启动期暴露。
pub fn register_wasm_library_hosts(
    host: &mut HostRegistry,
    registry: &LibraryRegistry,
) -> Result<(), String> {
    for op in registry.ops() {
        // 仅三方 WASM 库（注册表持有其 host.wasm 字节）的 op 需在此注册;标准库 op 走 native。
        let Some(wasm_bytes) = registry.host_wasm(&op.lib) else {
            continue;
        };
        ensure_i64_i64_i64_abi(op)?;
        let wasm_host = WasmHostFn::new_i64_i64_i64(wasm_bytes, &op.host_fn).map_err(|e| {
            format!(
                "三方 WASM 库 `{}` 的 op `{}.{}`（host_fn `{}`）host.wasm 装载失败：{e}",
                op.lib, op.family, op.op, op.host_fn
            )
        })?;
        host.register(op.family.clone(), op.op.clone(), Box::new(wasm_host));
    }
    Ok(())
}

/// 校验一个 op 契约符合本 demo WASM ABI 子集 `(Int, Int) -> Int`（标量 i64 直传）。
///
/// 超出子集（参数 / 返回含 `Text`/`Bool`/`Unit`/intent 包装,或参数个数 ≠ 2）一律诚实 `Err`——
/// `WasmHostFn` 当前只实现 `(i64,i64)->i64`,不为未实现的 ABI 伪造 host。
fn ensure_i64_i64_i64_abi(op: &OpContract) -> Result<(), String> {
    let is_int = |t: &TypeDesc| matches!(t, TypeDesc::Scalar(Scalar::Int));
    if op.params.len() == 2 && op.params.iter().all(is_int) && is_int(&op.returns) {
        Ok(())
    } else {
        Err(format!(
            "三方 WASM 库 `{}` 的 op `{}.{}` 签名超出当前 WASM host ABI 子集 (Int, Int) -> Int；\
             更复杂签名（Text / intent / 多参）随 ABI 扩展后支持（见 docs/stdlib_design.md §六.1）",
            op.lib, op.family, op.op
        ))
    }
}

/// 标准库 op 的**确定性 mock** 桶（path/url → 内容）。供测试 / 差测试预置数据。
#[derive(Clone, Default)]
pub struct MockBuckets {
    /// `File` 的内存桶：path → 内容（File.Write 写入、File.Read 读取）。
    files: Rc<RefCell<BTreeMap<String, String>>>,
    /// `Http.Get` 的预置响应桶：url → 响应体。
    http: Rc<RefCell<BTreeMap<String, String>>>,
}

impl MockBuckets {
    pub fn new() -> Self {
        MockBuckets::default()
    }

    /// 预置一条 `Http.Get` mock 响应（url → body）。未预置的 url 在 Http.Get 时诚实 `Err`。
    pub fn seed_http(&self, url: impl Into<String>, body: impl Into<String>) {
        self.http.borrow_mut().insert(url.into(), body.into());
    }

    /// 预置一条 `File.Read` mock 文件（path → 内容）。未预置且未写入的 path 诚实 `Err`。
    pub fn seed_file(&self, path: impl Into<String>, content: impl Into<String>) {
        self.files.borrow_mut().insert(path.into(), content.into());
    }
}

/// 把标准库 op 的 mock host（基于给定 [`MockBuckets`]）注册进注册表。
pub fn register_mock_hosts(host: &mut HostRegistry, buckets: &MockBuckets) {
    let http = buckets.http.clone();
    host.register_fn("Http", "Get", move |args| {
        let url = text_arg(args, 0, "Http.Get", "url")?;
        match http.borrow().get(&url) {
            Some(body) => Ok(Value::Text(body.clone())),
            None => Err(format!(
                "Http.Get(\"{url}\") 无 mock 响应（不做真实网络；未命中诚实 Err）"
            )),
        }
    });

    let read_files = buckets.files.clone();
    host.register_fn("File", "Read", move |args| {
        let path = text_arg(args, 0, "File.Read", "path")?;
        match read_files.borrow().get(&path) {
            Some(content) => Ok(Value::Text(content.clone())),
            None => Err(format!(
                "File.Read(\"{path}\") 无 mock 文件（不做真实文件 IO；未命中诚实 Err）"
            )),
        }
    });

    let write_files = buckets.files.clone();
    host.register_fn("File", "Write", move |args| {
        let path = text_arg(args, 0, "File.Write", "path")?;
        let content = text_arg(args, 1, "File.Write", "content")?;
        write_files.borrow_mut().insert(path, content);
        Ok(Value::Unit)
    });
}

/// 便捷：新建一个注册了标准库 mock host 的 [`HostRegistry`]，并返回其桶（供预置 + 读回）。
pub fn mock_host() -> (HostRegistry, MockBuckets) {
    let buckets = MockBuckets::new();
    let mut host = HostRegistry::new();
    register_mock_hosts(&mut host, &buckets);
    (host, buckets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sophia_library::LibraryContent;

    #[test]
    fn mock_file_roundtrip_and_seed() {
        let (mut host, buckets) = mock_host();
        buckets.seed_file("/seeded", "abcd");
        // seeded read
        let r = host
            .call("File", "Read", &[Value::Text("/seeded".into())])
            .unwrap();
        assert_eq!(r, Value::Text("abcd".into()));
        // write then read
        host.call(
            "File",
            "Write",
            &[Value::Text("/w".into()), Value::Text("xyz".into())],
        )
        .unwrap();
        let r = host
            .call("File", "Read", &[Value::Text("/w".into())])
            .unwrap();
        assert_eq!(r, Value::Text("xyz".into()));
    }

    #[test]
    fn mock_missing_is_honest_err() {
        let (mut host, _b) = mock_host();
        assert!(host
            .call("File", "Read", &[Value::Text("/no".into())])
            .is_err());
        assert!(host
            .call("Http", "Get", &[Value::Text("http://no".into())])
            .is_err());
    }

    #[test]
    fn mock_http_seeded() {
        let (mut host, buckets) = mock_host();
        buckets.seed_http("http://api/x", "payload");
        let r = host
            .call("Http", "Get", &[Value::Text("http://api/x".into())])
            .unwrap();
        assert_eq!(r, Value::Text("payload".into()));
    }

    /// 标准库注册表无 host.wasm → 注册三方 WASM host 为 no-op（不报错、不注册任何 op）。
    #[test]
    fn wasm_host_registration_noop_for_standard_registry() {
        let reg = crate::standard_registry();
        let mut host = HostRegistry::new();
        register_wasm_library_hosts(&mut host, &reg).expect("标准库无 WASM 库,应 no-op");
        // 标准库 op 的 host 由 native 注册,不应被 WASM 注册触及。
        assert!(!host.has_op("File", "Read"));
        assert!(!host.has_op("Http", "Get"));
    }

    /// 构造一个带 host.wasm 但签名超出 ABI 子集的三方库注册表 → 注册应诚实 `Err`（不伪造 host）。
    #[test]
    fn wasm_host_registration_rejects_out_of_abi_signature() {
        // op 签名含 Text 参数,超出 (Int, Int) -> Int 子集。host.wasm 字节随意（ABI 校验在加载前）。
        let content = LibraryContent {
            dir_name: "badwasm".into(),
            manifest_toml: r#"
[library]
name = "badwasm"
summary = "签名超 ABI 子集的 WASM 库"
abi_version = 1
[[op]]
family = "BadWasm"
op = "Hash"
params = ["Text"]
returns = "Int"
effectful = false
host_fn = "bad_hash"
[prompt]
asset = "x.md"
"#
            .into(),
            asset_text: "x".into(),
            sophia_sources: vec![],
            host_wasm: Some(vec![0, 1, 2, 3]),
        };
        let reg = LibraryRegistry::build(vec![content]).expect("build badwasm registry");
        let mut host = HostRegistry::new();
        let err =
            register_wasm_library_hosts(&mut host, &reg).expect_err("签名超 ABI 子集应诚实 Err");
        assert!(err.contains("ABI 子集"), "应报 ABI 子集不符：{err}");
        assert!(!host.has_op("BadWasm", "Hash"), "失败时不应注册 host");
    }

    /// 签名符合 ABI 子集但 host.wasm 非法字节 → 加载失败诚实 `Err`（启动期暴露,不静默跳过）。
    #[test]
    fn wasm_host_registration_rejects_invalid_wasm_bytes() {
        let content = LibraryContent {
            dir_name: "okabi".into(),
            manifest_toml: r#"
[library]
name = "okabi"
summary = "签名合规但 wasm 字节非法"
abi_version = 1
[[op]]
family = "OkAbi"
op = "Mix"
params = ["Int", "Int"]
returns = "Int"
effectful = false
host_fn = "mix"
[prompt]
asset = "x.md"
"#
            .into(),
            asset_text: "x".into(),
            sophia_sources: vec![],
            host_wasm: Some(vec![0, 1, 2, 3]),
        };
        let reg = LibraryRegistry::build(vec![content]).expect("build okabi registry");
        let mut host = HostRegistry::new();
        let err =
            register_wasm_library_hosts(&mut host, &reg).expect_err("非法 wasm 字节应诚实 Err");
        assert!(err.contains("装载失败"), "应报装载失败：{err}");
    }
}
