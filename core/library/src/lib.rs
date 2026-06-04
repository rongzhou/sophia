//! Sophia 库契约类型层（见 docs/stdlib_design.md / docs/stdlib_implementation.md）。
//!
//! 把「库」从散落编译器各层的硬编码切片，收敛为 **清单（`library.toml`）= 单一真相源** +
//! **[`LibraryRegistry`] = 各层只读数据源**。标准库与三方库共用同一份清单 schema 与注册表接口，
//! 只在装载路径上分二元（标准库静态编译 / 三方启动时一次性发现）。
//!
//! 分层：本 crate 属 core 层、core/* 可依赖——它只放**无 `runtime::Value` 的契约类型 + 清单解析**
//! （[`TypeDesc`] / [`OpContract`] / [`LibraryRegistry`]）。host 实现（需 `Value`）落 `sophia-runtime`
//! 的 `HostRegistry`，避免依赖环（见 stdlib_design 依赖图）。**零文件 IO**：只解析传入的清单字符串，
//! 文件发现由协调层（CLI）做。

#![forbid(unsafe_code)]

mod error;
mod manifest;
mod registry;
mod typedesc;

pub use error::{LibraryError, LibraryResult};
pub use manifest::RawManifest;
pub use registry::{LibraryContent, LibraryRegistry, OpContract, PromptAsset, SophiaSource};
pub use typedesc::{Scalar, TypeDesc};
