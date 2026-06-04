//! ASG index：节点名 → 节点元信息（kind / domain / path）。
//!
//! 见 docs/language_implementation.md 17.2、docs/engineering_architecture.md 5.1。
//! ASG index 是可重建缓存，不是语义源。名称解析依赖它把跨节点引用解析为
//! 具体节点。
//!
//! `core` 不做 IO：index 由调用方（CLI 文件扫描）从内存中的 AST 集合构建，
//! 这里只负责聚合、查重与稳定序列化。

use crate::error::{HirError, HirResult};
use serde::{Deserialize, Serialize};
use sophia_library::{LibraryRegistry, OpContract};
use sophia_syntax::{Ast, Item};
use std::collections::{BTreeMap, BTreeSet};

/// 节点类型（对应顶层 formal node）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum NodeKind {
    Domain,
    Entity,
    State,
    Transition,
    Error,
    Capability,
    Action,
    Task,
    /// effect 族声明（内置/领域 effect，见 docs/language_design.md 第十三节）。
    Effect,
}

impl NodeKind {
    /// 由 AST `Item` 推出节点类型。
    pub fn of_item(item: &Item) -> Self {
        match item {
            Item::Domain(_) => NodeKind::Domain,
            Item::Entity(_) => NodeKind::Entity,
            Item::State(_) => NodeKind::State,
            Item::Transition(_) => NodeKind::Transition,
            Item::Error(_) => NodeKind::Error,
            Item::Capability(_) => NodeKind::Capability,
            Item::Action(_) => NodeKind::Action,
            Item::Task(_) => NodeKind::Task,
            Item::Effect(_) => NodeKind::Effect,
        }
    }
}

/// 单个节点的索引信息。字段顺序与 docs/language_implementation.md 17.2 对齐。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeInfo {
    pub kind: NodeKind,
    pub domain: String,
    pub path: String,
}

/// error variant 的归属信息。
///
/// variant 是 error 节点的成员，不是顶层节点，因此**不进入** `asg_index.json`
/// （该文件只含顶层节点，见 17.2）。这里单独建表，供名称解析校验
/// `errors { ... }` 引用与 `raise Variant { ... }`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantInfo {
    /// 所属 error 节点名。
    pub error_node: String,
    /// 所属 domain。
    pub domain: String,
}

/// effect 操作的归属信息（`Family.Op` → 参数个数 + 是否内置）。
///
/// 与 `variants` 同属派生符号表，`#[serde(skip)]` 不入 `asg_index.json`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectOpInfo {
    /// effect 族名。
    pub family: String,
    /// 操作名。
    pub op: String,
    /// 声明的参数个数。
    pub arity: usize,
    /// 是否内置（Console/DB/Llm/Tool/Stream）；false 表示用户 `effect` 声明。
    pub builtin: bool,
}

/// ASG index：节点名到节点信息的稳定映射。
///
/// 使用 `BTreeMap` 保证 key 稳定排序（docs/engineering_notes.md “输出确定性”），
/// 序列化产物可比对、可快照、可复现。
///
/// `variants` 是派生的成员符号表（error variant → 归属），用 `#[serde(skip)]`
/// 排除在序列化之外，以保持 `asg_index.json` 与 17.2 规范一致（只含顶层节点）。
/// `effect_ops` 同理：`Family.Op` → 参数形状（内置 + `effect` 声明），用于校验
/// effect 引用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsgIndex {
    pub version: u32,
    pub nodes: BTreeMap<String, NodeInfo>,
    #[serde(skip)]
    pub variants: BTreeMap<String, VariantInfo>,
    #[serde(skip)]
    pub effect_ops: BTreeMap<String, EffectOpInfo>,
    /// 库特殊根 family 集（如 `File` / `Http`），由 [`LibraryRegistry`] 注入。名称解析据此放行
    /// body 级 `Lib.Op(args)` 入口（[`Self::is_library_family`]），使核心不硬编码具体库名。
    /// 派生符号表，`#[serde(skip)]`（不入 `asg_index.json`）。
    #[serde(skip)]
    pub library_families: BTreeSet<String>,
    /// 库 op 契约（`family.op` → 签名 / 返回 / host_fn），由 [`LibraryRegistry`] 注入。语义层据此
    /// 表驱动校验 `Lib.Op(args)`（替代命令式 match）。派生符号表，`#[serde(skip)]`。
    #[serde(skip)]
    pub library_ops: BTreeMap<String, OpContract>,
    /// 库 domain 集（纯 Sophia 源码库的「库名即 domain」），由 [`LibraryRegistry`] 注入。用户代码
    /// 跨 domain 引用库节点时据此**豁免** `ImplicitCrossDomain` 诊断（[`Self::is_library_domain`]）
    /// ——库是显式可用的外部能力（类比 task include 是显式入口）；用户↔用户跨 domain 仍受检。
    /// 派生符号表，`#[serde(skip)]`。
    #[serde(skip)]
    pub library_domains: BTreeSet<String>,
}

/// 当前 ASG index schema 版本。
pub const ASG_INDEX_VERSION: u32 = 1;

impl Default for AsgIndex {
    fn default() -> Self {
        AsgIndex::new(&LibraryRegistry::empty())
    }
}

/// index 构建过程中的单个源文件输入：一个 domain 下一个文件的 AST。
pub struct IndexInput<'a> {
    /// 所属 domain（PascalCase 目录名）。
    pub domain: &'a str,
    /// 相对项目根的正斜杠路径，如 `domains/TodoDomain/entities/Todo.sophia`。
    pub path: &'a str,
    /// 该文件解析出的 AST。
    pub ast: &'a Ast,
}

impl AsgIndex {
    /// 新建空 index，并注入给定库注册表的 effect / 特殊根 family / op 契约。
    pub fn new(registry: &LibraryRegistry) -> Self {
        let mut effect_ops = BTreeMap::new();
        // 语言内置（Console）——「机制 vs 能力族」边界里的例外（输出原语保留为语言内置）。
        for (family, op, arity) in crate::builtins::BUILTIN_EFFECT_OPS {
            effect_ops.insert(
                format!("{family}.{op}"),
                EffectOpInfo {
                    family: (*family).to_string(),
                    op: (*op).to_string(),
                    arity: *arity,
                    builtin: true,
                },
            );
        }
        let mut index = AsgIndex {
            version: ASG_INDEX_VERSION,
            nodes: BTreeMap::new(),
            variants: BTreeMap::new(),
            effect_ops,
            library_families: BTreeSet::new(),
            library_ops: BTreeMap::new(),
            library_domains: BTreeSet::new(),
        };
        for contract in registry.ops() {
            let key = format!("{}.{}", contract.family, contract.op);
            index.library_families.insert(contract.family.clone());
            index.library_ops.insert(key.clone(), contract.clone());
            if contract.effectful {
                index.effect_ops.insert(
                    key,
                    EffectOpInfo {
                        family: contract.family.clone(),
                        op: contract.op.clone(),
                        arity: 0,
                        builtin: true,
                    },
                );
            }
        }
        // 纯 Sophia 源码库的 domain（库名即 domain）→ 用户跨 domain 引用库节点时豁免诊断。
        for src in registry.sophia_sources() {
            index.library_domains.insert(src.domain.clone());
        }
        index
    }

    /// 查 effect 操作的声明信息（`Family.Op`）。
    pub fn effect_op(&self, family: &str, op: &str) -> Option<&EffectOpInfo> {
        self.effect_ops.get(&format!("{family}.{op}"))
    }

    /// 某 family 是否为库特殊根（`File` / `Http` / 三方库 family）。名称解析据此放行 body 级
    /// `Lib.Op(args)` 入口；语言内置 `Console` 不在此（它经 `print` 语句、非特殊根 method_call）。
    pub fn is_library_family(&self, family: &str) -> bool {
        self.library_families.contains(family)
    }

    /// 某 domain 是否为库 domain（纯 Sophia 源码库的「库名即 domain」）。名称解析据此对「用户 →
    /// 库节点」的跨 domain 引用**豁免** `ImplicitCrossDomain`（库是显式可用的外部能力）。
    pub fn is_library_domain(&self, domain: &str) -> bool {
        self.library_domains.contains(domain)
    }

    /// 查库 op 契约（`Family.Op` → 签名 / 返回 / host_fn）。语义层 / 运行时据此表驱动处理。
    pub fn library_op(&self, family: &str, op: &str) -> Option<&OpContract> {
        self.library_ops.get(&format!("{family}.{op}"))
    }

    /// 从一组源文件输入构建 index，并注入给定库注册表。
    ///
    /// 约束（docs/engineering_architecture.md 5.1）：
    /// - 一个文件只能定义一个顶层 formal node；
    /// - 禁止同名 shadowing（跨文件重名即报错）。
    ///
    /// 为产出确定性结果，输入先按 path 升序排序后处理。
    pub fn build(inputs: Vec<IndexInput<'_>>, registry: &LibraryRegistry) -> HirResult<Self> {
        let mut inputs = inputs;
        inputs.sort_by(|a, b| a.path.cmp(b.path));

        let mut index = AsgIndex::new(registry);
        for input in inputs {
            // 一个文件只能定义一个顶层 node。
            if input.ast.items.len() > 1 {
                return Err(HirError::MultipleTopLevelNodes {
                    path: input.path.to_string(),
                    count: input.ast.items.len(),
                });
            }
            let Some(item) = input.ast.items.first() else {
                return Err(HirError::EmptyNodeFile {
                    path: input.path.to_string(),
                });
            };

            let name = item.name().text.clone();
            let info = NodeInfo {
                kind: NodeKind::of_item(item),
                domain: input.domain.to_string(),
                path: input.path.to_string(),
            };

            // 禁止同名 shadowing：跨文件重名直接拒绝。
            if let Some(existing) = index.nodes.get(&name) {
                return Err(HirError::DuplicateNode {
                    name,
                    first_path: existing.path.clone(),
                    second_path: input.path.to_string(),
                });
            }

            // error 节点：登记其 variant 到成员符号表，并禁止跨 error 的 variant 重名。
            if let Item::Error(err) = item {
                for variant in &err.variants {
                    let vname = variant.name.text.clone();
                    if index.nodes.contains_key(&vname) || index.variants.contains_key(&vname) {
                        return Err(HirError::DuplicateNode {
                            name: vname,
                            first_path: "（已存在的节点或 variant）".to_string(),
                            second_path: input.path.to_string(),
                        });
                    }
                    index.variants.insert(
                        vname,
                        VariantInfo {
                            error_node: name.clone(),
                            domain: input.domain.to_string(),
                        },
                    );
                }
            }

            // effect 声明：登记其操作到 effect 符号表（与内置族并入同一表）。
            if let Item::Effect(eff) = item {
                if let Some(existing_op) = index
                    .effect_ops
                    .values()
                    .find(|existing| existing.family == eff.name.text && existing.builtin)
                {
                    let existing_desc = if index.library_families.contains(&eff.name.text) {
                        "库注册表已声明该 effect family"
                    } else {
                        "语言内置 effect family 已声明该 effect family"
                    };
                    return Err(HirError::EffectOpConflict {
                        family: eff.name.text.clone(),
                        op: existing_op.op.clone(),
                        existing: existing_desc.to_string(),
                        path: input.path.to_string(),
                    });
                }
                let mut local_ops = BTreeSet::new();
                for op in &eff.operations {
                    let key = format!("{}.{}", eff.name.text, op.name.text);
                    if !local_ops.insert(key.clone()) {
                        return Err(HirError::EffectOpConflict {
                            family: eff.name.text.clone(),
                            op: op.name.text.clone(),
                            existing: "同一 effect 声明内已存在同名 operation".to_string(),
                            path: input.path.to_string(),
                        });
                    }
                    if let Some(existing) = index.effect_ops.get(&key) {
                        let existing = if existing.builtin {
                            if index.library_ops.contains_key(&key) {
                                "库注册表已声明该 effect op"
                            } else {
                                "语言内置 effect op 已声明该 effect op"
                            }
                        } else {
                            "用户 effect 声明已声明该 effect op"
                        };
                        return Err(HirError::EffectOpConflict {
                            family: eff.name.text.clone(),
                            op: op.name.text.clone(),
                            existing: existing.to_string(),
                            path: input.path.to_string(),
                        });
                    }
                    index.effect_ops.insert(
                        key,
                        EffectOpInfo {
                            family: eff.name.text.clone(),
                            op: op.name.text.clone(),
                            arity: op.params.len(),
                            builtin: false,
                        },
                    );
                }
            }

            index.nodes.insert(name, info);
        }
        Ok(index)
    }

    /// 查节点信息。
    pub fn get(&self, name: &str) -> Option<&NodeInfo> {
        self.nodes.get(name)
    }

    /// 查节点类型。
    pub fn kind_of(&self, name: &str) -> Option<NodeKind> {
        self.nodes.get(name).map(|n| n.kind)
    }

    /// 节点是否存在。
    pub fn contains(&self, name: &str) -> bool {
        self.nodes.contains_key(name)
    }

    /// 查 error variant 归属信息。
    pub fn variant(&self, name: &str) -> Option<&VariantInfo> {
        self.variants.get(name)
    }

    /// 序列化为稳定排序的 JSON（pretty）。
    pub fn to_json(&self) -> HirResult<String> {
        serde_json::to_string_pretty(self).map_err(|e| HirError::Serialization(e.to_string()))
    }
}
