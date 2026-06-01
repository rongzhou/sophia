//! 隐藏验证用例存储加载（CLI 协调层，IO 边界）。
//!
//! 见 docs/workflow_graph_spec.md 五A 节、docs/engineering_architecture.md §9.2.1。
//! regression gate 的 hidden case「期望输入 / 输出」是 **validation-only** 数据，绝不能让
//! 被验证的 LLM 看见。三层隔离：① 图节点只存不透明引用 `verifier.ref`；② active context 的
//! `ConstraintView` 整体剔除 verifier；③ 用例正文存于**图外**的 `sophia-runs/verifiers/hidden.json`，
//! 与 Development Graph 物理隔离，**只有确定性 gate 在 materialize 时按 ref 取用**。
//!
//! 本模块只负责把 `hidden.json` 反序列化为 `ref → HiddenCase` 映射；执行属 `runtime`
//! （`run_hidden_case`），判定属 `tools/audit`（`audit_constraints`）。`hidden.json` 由出题方 /
//! 维护者写入，**不由 LLM 产生**——其写入路径与生成代码路径物理隔离。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sophia_runtime::HiddenCase;

/// 隐藏验证用例存储：`ref → HiddenCase`。
///
/// 缺文件视为空存储（合法——项目可以没有任何 hidden case）；但若某 invariant 声明了
/// `HiddenCase` verifier 而存储里缺对应 ref，则由 gate 侧诚实硬错误阻断（见 `graph_cmd`）。
#[derive(Debug, Default)]
pub struct HiddenVerifierStore {
    by_ref: BTreeMap<String, HiddenCase>,
}

/// `hidden.json` 的标准路径（与 dev_graph.sqlite 物理隔离，不进图、不进 active context）。
pub fn store_path(root: &Path) -> PathBuf {
    root.join("sophia-runs/verifiers/hidden.json")
}

impl HiddenVerifierStore {
    /// 从项目根加载隐藏存储。文件不存在 → 空存储（非错误）。
    ///
    /// 文件存在时按 `Vec<HiddenCase>` 反序列化（strict：多余字段拒绝）；`ref` 重复视为错误
    /// （键必须唯一，见 spec 五A.2）。
    pub fn load(root: &Path) -> Result<Self> {
        let path = store_path(root);
        if !path.exists() {
            return Ok(HiddenVerifierStore::default());
        }
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("读取隐藏验证用例存储 {} 失败", path.display()))?;
        let cases: Vec<HiddenCase> = serde_json::from_str(&raw)
            .with_context(|| format!("解析 {} 失败（应为 HiddenCase 数组）", path.display()))?;

        let mut by_ref = BTreeMap::new();
        for case in cases {
            if by_ref.contains_key(&case.verifier_ref) {
                anyhow::bail!("隐藏验证用例 ref `{}` 重复（必须唯一）", case.verifier_ref);
            }
            by_ref.insert(case.verifier_ref.clone(), case);
        }
        Ok(HiddenVerifierStore { by_ref })
    }

    /// 按 ref 查 hidden case（缺 → None，由 gate 侧据此硬错误阻断）。
    pub fn get(&self, verifier_ref: &str) -> Option<&HiddenCase> {
        self.by_ref.get(verifier_ref)
    }
}
