//! 库清单（`library.toml`）的反序列化形态。
//!
//! 见 docs/stdlib_design.md §清单 schema。清单是**单一真相源**：一个库在各层（HIR / 语义 /
//! 运行时 / codegen / 提示词）的契约都从这里派生。本模块只做 TOML → Rust 结构的反序列化 +
//! 形状校验；语义校验（intent 名是否属核心集、与用户 domain 是否冲突）在更高层。

use serde::Deserialize;

/// 一份 `library.toml` 的原始反序列化结果。
#[derive(Debug, Clone, Deserialize)]
pub struct RawManifest {
    pub library: RawLibrary,
    /// surface 维度 A：Sophia 源码节点（可选）。
    #[serde(default)]
    pub surface: RawSurface,
    /// surface 维度 B + host：effect 操作（可选，每 op 一条）。
    #[serde(default, rename = "op")]
    pub ops: Vec<RawOp>,
    /// 提示词资产引用。
    pub prompt: RawPrompt,
}

/// `[library]` 段：库身份。
#[derive(Debug, Clone, Deserialize)]
pub struct RawLibrary {
    /// 库名（= 目录名，唯一标识）。
    pub name: String,
    /// 一句话用途（进 stdlib_catalog）。
    pub summary: String,
    /// 清单 schema 版本。
    pub abi_version: u32,
}

/// `[surface]` 段：随库装入 ASG 的 Sophia 源码节点路径（相对库目录）。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RawSurface {
    #[serde(default)]
    pub sophia_sources: Vec<String>,
}

/// `[[op]]` 段：一个 effect 操作的契约。
#[derive(Debug, Clone, Deserialize)]
pub struct RawOp {
    /// effect 族（= 特殊根名，如 `Http`）。
    pub family: String,
    /// 操作名（如 `Get`）。
    pub op: String,
    /// 参数签名（有序，每项是 TypeDesc 字符串）。
    #[serde(default)]
    pub params: Vec<String>,
    /// 返回类型描述符。
    pub returns: String,
    /// 是否需要真实 host（false = 纯计算 op，无 host import）。
    #[serde(default = "default_true")]
    pub effectful: bool,
    /// host 分派键（HostRegistry 的键 / WASM import 名）。
    pub host_fn: String,
}

fn default_true() -> bool {
    true
}

/// `[prompt]` 段：提示词资产引用。
#[derive(Debug, Clone, Deserialize)]
pub struct RawPrompt {
    /// 资产文件名（相对库目录）。
    pub asset: String,
}

impl RawManifest {
    /// 从 TOML 文本解析（仅形状，不做语义校验）。
    pub fn from_toml(src: &str) -> Result<RawManifest, String> {
        toml::from_str(src).map_err(|e| e.to_string())
    }
}
