//! 库注册表：清单 = 单一真相源 → 各层只读数据源。
//!
//! 见 docs/stdlib_design.md。`LibraryRegistry` 由一组库清单构建（标准库静态 / 三方启动时发现），
//! **构建后冻结**（确定性门禁前提）。各层（HIR / 语义 / codegen / 提示词）消费它，而非各自硬编码
//! 某库的 family/op/签名——这是「库不渗透语言核心」的结构落点。
//!
//! 本 registry **不含** host 实现（那需要 runtime::Value，落 sophia-runtime 的 HostRegistry）；
//! 这里只放无 Value 的**契约**（op 签名 / 特殊根 / 提示词资产 / Sophia 源码引用 / host_fn 分派键）。

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{LibraryError, LibraryResult, SUPPORTED_ABI_VERSION};
use crate::manifest::RawManifest;
use crate::typedesc::TypeDesc;

/// 单个 effect 操作的契约（清单 `[[op]]` 解析所得）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpContract {
    /// 来源库名（诊断 / 冲突报告用）。
    pub lib: String,
    /// effect 族。
    pub family: String,
    /// 操作名。
    pub op: String,
    /// 参数签名（有序）。
    pub params: Vec<TypeDesc>,
    /// 返回类型。
    pub returns: TypeDesc,
    /// 是否需要真实 host。
    pub effectful: bool,
    /// host 分派键。
    pub host_fn: String,
}

/// 提示词资产（design 目录 + implement 完整文）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptAsset {
    /// 一句话用途（进 stdlib_catalog）。
    pub summary: String,
    /// 完整资产文本（implement / repair 注入）。
    pub asset_text: String,
}

/// 随库装入 ASG 的 Sophia 源码节点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SophiaSource {
    /// 来源库名。
    pub lib: String,
    /// 库自有 domain（= 库名，与用户 domain 隔离）。
    pub domain: String,
    /// 源码逻辑路径（用于诊断 / index）。
    pub path: String,
    /// `.sophia` 源码正文。
    pub source: String,
}

/// 一个待注册库的全部内容（调用方从目录 / `include_str!` 组装后交给 registry 构建）。
pub struct LibraryContent {
    /// 库目录名（= 期望的库标识）。
    pub dir_name: String,
    /// `library.toml` 原文。
    pub manifest_toml: String,
    /// 提示词资产原文（清单 `[prompt].asset` 指向的文件内容）。
    pub asset_text: String,
    /// Sophia 源码节点：(逻辑路径, 源码正文)，与清单 `[surface].sophia_sources` 顺序对应。
    pub sophia_sources: Vec<(String, String)>,
    /// 库目录下 `host.wasm` 的字节（三方 WASM-effect 库提供；标准库 / 纯 Sophia 库为 `None`）。
    /// 注册表据此区分：有 effect-op 但无 `host.wasm` 的库 = 标准库（native host 编译进二进制）;
    /// 有 effect-op 且有 `host.wasm` 的库 = 三方 WASM 库（host 经 `WasmHostFn` 加载）。
    pub host_wasm: Option<Vec<u8>>,
}

/// 库注册表：各层的只读数据源。
#[derive(Debug, Clone, Default)]
pub struct LibraryRegistry {
    /// `family.op` → 契约。
    ops: BTreeMap<String, OpContract>,
    /// 特殊根白名单（= 所有库 family 并集）。
    families: BTreeSet<String>,
    /// 库名 → 提示词资产。
    prompt_assets: BTreeMap<String, PromptAsset>,
    /// 库 Sophia 源码节点（装入 ASG）。
    sophia_sources: Vec<SophiaSource>,
    /// 库 domain → 库名（domain 冲突检查）。
    domains: BTreeMap<String, String>,
    /// 三方 WASM 库的 `host.wasm` 字节（库名 → wasm 字节）。供 `WasmHostFn` 加载。
    host_wasm: BTreeMap<String, Vec<u8>>,
}

impl LibraryRegistry {
    /// 空注册表（无任何库；Console 等语言内置不经此表）。
    pub fn empty() -> Self {
        LibraryRegistry::default()
    }

    /// 从一组库内容构建注册表（标准库 + 三方合并）。冲突一律报错（不静默覆盖）。
    ///
    /// 输入顺序不影响结果（内部按库名 / family.op 字典序聚合），保证确定性。
    pub fn build(contents: Vec<LibraryContent>) -> LibraryResult<Self> {
        let mut reg = LibraryRegistry::default();
        // 按库名排序，确定性 + 冲突报告稳定。
        let mut contents = contents;
        contents.sort_by(|a, b| a.dir_name.cmp(&b.dir_name));
        for content in contents {
            reg.add_library(content)?;
        }
        Ok(reg)
    }

    /// 合并一个库（解析清单 + 校验冲突）。
    fn add_library(&mut self, content: LibraryContent) -> LibraryResult<()> {
        let dir = content.dir_name.clone();
        let raw = RawManifest::from_toml(&content.manifest_toml).map_err(|reason| {
            LibraryError::ManifestParse {
                lib: dir.clone(),
                reason,
            }
        })?;
        let lib = raw.library.name.clone();

        // 目录名 = 清单 name。
        if lib != dir {
            return Err(LibraryError::NameMismatch { dir, name: lib });
        }
        // abi 版本。
        if raw.library.abi_version != SUPPORTED_ABI_VERSION {
            return Err(LibraryError::UnsupportedAbi {
                lib,
                found: raw.library.abi_version,
                supported: SUPPORTED_ABI_VERSION,
            });
        }
        // 库名唯一。
        if self.prompt_assets.contains_key(&lib) {
            return Err(LibraryError::DuplicateLibrary { name: lib });
        }

        // ops：解析 TypeDesc + 校验 family/op 与 host_fn 冲突。
        let mut host_fns: BTreeMap<String, String> = BTreeMap::new();
        for raw_op in &raw.ops {
            let op_label = format!("{}.{}", raw_op.family, raw_op.op);
            if let Some(first_op) = host_fns.get(&raw_op.host_fn) {
                return Err(LibraryError::DuplicateHostFn {
                    lib,
                    host_fn: raw_op.host_fn.clone(),
                    first_op: first_op.clone(),
                    second_op: op_label,
                });
            }
            host_fns.insert(raw_op.host_fn.clone(), op_label);
            let params = raw_op
                .params
                .iter()
                .map(|p| {
                    TypeDesc::parse(p).map_err(|reason| LibraryError::BadTypeDesc {
                        lib: lib.clone(),
                        desc: p.clone(),
                        reason,
                    })
                })
                .collect::<LibraryResult<Vec<_>>>()?;
            let returns =
                TypeDesc::parse(&raw_op.returns).map_err(|reason| LibraryError::BadTypeDesc {
                    lib: lib.clone(),
                    desc: raw_op.returns.clone(),
                    reason,
                })?;
            let key = format!("{}.{}", raw_op.family, raw_op.op);
            if let Some(existing) = self.ops.get(&key) {
                return Err(LibraryError::DuplicateOp {
                    family: raw_op.family.clone(),
                    op: raw_op.op.clone(),
                    first: existing.lib.clone(),
                    second: lib,
                });
            }
            // family 冲突：family 已被**别的库**占用（同库多 op 共享 family 合法）。
            if let Some(owner) = self.family_owner(&raw_op.family) {
                if owner != lib {
                    return Err(LibraryError::DuplicateFamily {
                        family: raw_op.family.clone(),
                        first: owner,
                        second: lib,
                    });
                }
            }
            self.ops.insert(
                key,
                OpContract {
                    lib: lib.clone(),
                    family: raw_op.family.clone(),
                    op: raw_op.op.clone(),
                    params,
                    returns,
                    effectful: raw_op.effectful,
                    host_fn: raw_op.host_fn.clone(),
                },
            );
            self.families.insert(raw_op.family.clone());
        }

        // Sophia 源码：库 domain = 库名，与用户 / 其它库 domain 隔离。
        if !content.sophia_sources.is_empty() || !raw.surface.sophia_sources.is_empty() {
            validate_sophia_source_manifest(
                &lib,
                &raw.surface.sophia_sources,
                &content.sophia_sources,
            )?;
            let domain = lib.clone();
            if let Some(other) = self.domains.get(&domain) {
                if other != &lib {
                    return Err(LibraryError::DuplicateDomain {
                        domain,
                        first: other.clone(),
                        second: lib,
                    });
                }
            }
            self.domains.insert(domain.clone(), lib.clone());
            for (path, source) in &content.sophia_sources {
                self.sophia_sources.push(SophiaSource {
                    lib: lib.clone(),
                    domain: domain.clone(),
                    path: path.clone(),
                    source: source.clone(),
                });
            }
        }

        // 提示词资产。
        self.prompt_assets.insert(
            lib.clone(),
            PromptAsset {
                summary: raw.library.summary.clone(),
                asset_text: content.asset_text.clone(),
            },
        );
        // 三方 WASM 库的 host.wasm 字节（标准库 / 纯 Sophia 库为 None）。
        if let Some(bytes) = content.host_wasm {
            self.host_wasm.insert(lib.clone(), bytes);
        }
        Ok(())
    }

    fn family_owner(&self, family: &str) -> Option<String> {
        self.ops
            .values()
            .find(|c| c.family == family)
            .map(|c| c.lib.clone())
    }

    // ---- 各层只读消费 API ----

    /// 查 op 契约（`family.op`）。
    pub fn op(&self, family: &str, op: &str) -> Option<&OpContract> {
        self.ops.get(&format!("{family}.{op}"))
    }

    /// 某库的 `host.wasm` 字节（三方 WASM-effect 库提供；标准库 / 纯 Sophia 库为 `None`）。
    pub fn host_wasm(&self, lib: &str) -> Option<&[u8]> {
        self.host_wasm.get(lib).map(|v| v.as_slice())
    }

    /// 某 family 是否为已注册库的特殊根。
    pub fn is_library_family(&self, family: &str) -> bool {
        self.families.contains(family)
    }

    /// 全部 op 契约（字典序，确定性）。
    pub fn ops(&self) -> impl Iterator<Item = &OpContract> {
        self.ops.values()
    }

    /// 全部库名（字典序）。
    pub fn lib_names(&self) -> Vec<&str> {
        self.prompt_assets.keys().map(|s| s.as_str()).collect()
    }

    /// 某库的提示词资产。
    pub fn prompt_asset(&self, lib: &str) -> Option<&PromptAsset> {
        self.prompt_assets.get(lib)
    }

    /// 库目录（每库一行 `名 — 用途`，库名字典序）。design 阶段注入。
    pub fn catalog(&self) -> String {
        self.prompt_assets
            .iter()
            .map(|(name, a)| format!("- `{name}` — {}", a.summary))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 按所选库集合拼接完整资产（库名字典序、去重、未知库忽略、空集空串）。implement 阶段注入。
    pub fn preamble(&self, libs: &[&str]) -> String {
        let mut selected: Vec<&str> = libs.to_vec();
        selected.sort_unstable();
        selected.dedup();
        selected
            .iter()
            .filter_map(|l| self.prompt_assets.get(*l).map(|a| a.asset_text.as_str()))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// 库装入 ASG 的 Sophia 源码节点（确定性顺序）。
    pub fn sophia_sources(&self) -> &[SophiaSource] {
        &self.sophia_sources
    }
}

fn validate_sophia_source_manifest(
    lib: &str,
    manifest_paths: &[String],
    content_sources: &[(String, String)],
) -> LibraryResult<()> {
    let manifest: BTreeSet<&str> = manifest_paths.iter().map(String::as_str).collect();
    let content: BTreeSet<&str> = content_sources
        .iter()
        .map(|(path, _)| path.as_str())
        .collect();
    if manifest == content
        && manifest.len() == manifest_paths.len()
        && content.len() == content_sources.len()
    {
        return Ok(());
    }
    let missing = manifest
        .difference(&content)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    let extra = content
        .difference(&manifest)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    let mut parts = Vec::new();
    if !missing.is_empty() {
        parts.push(format!("缺少源码：{missing}"));
    }
    if !extra.is_empty() {
        parts.push(format!("多余源码：{extra}"));
    }
    if manifest.len() != manifest_paths.len() {
        parts.push("清单存在重复路径".to_string());
    }
    if content.len() != content_sources.len() {
        parts.push("传入内容存在重复路径".to_string());
    }
    Err(LibraryError::SophiaSourcesMismatch {
        lib: lib.to_string(),
        reason: parts.join("；"),
    })
}
