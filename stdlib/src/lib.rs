//! Sophia 标准库内容层（见 docs/stdlib_design.md / docs/stdlib_implementation.md）。
//!
//! 每个标准库一个 `libs/<lib>/` 目录（清单 `library.toml` + 提示词资产 + 可选 Sophia 源码），
//! 编译进二进制（`include_str!`）。本 crate 提供三件事：
//! - [`standard_registry`]：从内置清单构建 [`LibraryRegistry`]（各层契约只读源）；
//! - [`register_native_hosts`]：把库 op 的**真实** host（`reqwest` / `std::fs`）注册进
//!   [`HostRegistry`]（解释器 / CLI 真实执行用）；
//! - [`mock_host`]：注册库 op 的**确定性 mock** host（内存桶，未命中诚实 `Err`；测试 / 差测试用）。
//!
//! 分层：归 core 之上、协调层之下——可做 IO（native host）；`core` 不依赖本 crate。新增标准库 =
//! 加一个 `libs/<lib>/` 目录 + 在 [`STDLIB_LIBS`] 登记一行 + （若 effectful）在 host 注册函数补分派。

#![forbid(unsafe_code)]

mod discover;
mod native_host;

use sophia_library::{LibraryContent, LibraryRegistry};

pub use discover::{full_registry_for, full_registry_from, project_roots, DiscoverError};
pub use native_host::{
    mock_host, register_mock_hosts, register_native_hosts, register_wasm_library_hosts, MockBuckets,
};

/// 内置标准库清单：(库名, library.toml, 资产文本)。新增标准库在此登记一行。
///
/// 资产文本由清单 `[prompt].asset` 指向；这里直接 `include_str!` 配对（标准库静态编译进二进制，
/// 无需运行时按 `[prompt].asset` 再去文件系统找）。Sophia 源码库在此元组扩展（当前标准库无源码库）。
const STDLIB_LIBS: &[(&str, &str, &str)] = &[
    (
        "file",
        include_str!("../libs/file/library.toml"),
        include_str!("../libs/file/file.md"),
    ),
    (
        "http",
        include_str!("../libs/http/library.toml"),
        include_str!("../libs/http/http.md"),
    ),
];

/// 标准库内容（供 [`standard_registry`] 与三方发现合并复用）。
pub(crate) fn stdlib_contents() -> Vec<LibraryContent> {
    STDLIB_LIBS
        .iter()
        .map(|(name, manifest, asset)| LibraryContent {
            dir_name: (*name).to_string(),
            manifest_toml: (*manifest).to_string(),
            asset_text: (*asset).to_string(),
            sophia_sources: Vec::new(),
            // 标准库 native host 编译进二进制，无 host.wasm。
            host_wasm: None,
        })
        .collect()
}

/// 构建标准库注册表（仅标准库；三方库由协调层经 [`full_registry_for`] 发现后合并）。
///
/// 用于：确定性测试 / `check_program`（三方发现是启动行为,不进核心确定性门禁）；CLI 启动则用
/// [`full_registry_for`]（标准库 + 三方发现）。
pub fn standard_registry() -> LibraryRegistry {
    // 标准库清单是内置常量、由 snapshot / 单测守护，构建失败属编译期级错误。
    LibraryRegistry::build(stdlib_contents()).expect("内置标准库清单应能构建注册表（由单测守护）")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_registry_has_file_and_http() {
        let reg = standard_registry();
        assert!(reg.op("Http", "Get").is_some());
        assert!(reg.op("File", "Read").is_some());
        assert!(reg.op("File", "Write").is_some());
        assert!(reg.is_library_family("Http"));
        assert!(reg.is_library_family("File"));
        // 目录字典序：file 在 http 前。
        let catalog = reg.catalog();
        assert!(catalog.contains("`file`") && catalog.contains("`http`"));
    }
}
