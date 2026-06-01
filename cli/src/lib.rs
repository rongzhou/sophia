//! Sophia CLI 库（协调层）。
//!
//! 见 docs/engineering_architecture.md 第九节。CLI 是 IO 与呈现的归属层：`core` / `tools` 保持
//! 无 IO，文件读取与诊断渲染都在这里完成。
//!
//! 二进制入口在 `main.rs`（薄封装，调用本库）；本库同时供 `examples/`（e2e / benchmark）复用
//! 协调层构件。e2e 用**真实 IO** 执行（e2e 禁 mock，见 docs/e2e_test.md）：库 host 由
//! `sophia-stdlib` 的 `register_native_hosts` 注册进 `sophia_runtime::HostRegistry`（真实 `reqwest`
//! / `std::fs`）。

pub mod commands;
pub mod graph_cmd;
pub mod project;
pub mod render;
pub mod verifier_store;
