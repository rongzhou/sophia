//! 项目文件扫描与装载（CLI 的 IO 层）。
//!
//! 见 docs/engineering_architecture.md 第五节。CLI 负责文件 IO；`core` 保持无 IO。
//! 本模块扫描 `domains/` 下的 `.sophia` 文件，推导每个文件的 domain，解析为 AST，
//! 供 index / check / run 复用。
//!
//! 遍历顺序按路径字典序（docs/engineering_notes.md 输出确定性）。

use anyhow::{Context, Result};
use sophia_syntax::{parse_str, Ast, SyntaxDiagnostic};
use std::path::{Path, PathBuf};

/// 一个已装载的源文件单元。
pub struct LoadedFile {
    /// 所属 domain（`domains/<Domain>/...` 的 `<Domain>` 段）。
    pub domain: String,
    /// 相对项目根的正斜杠路径（如 `domains/TodoDomain/entities/Todo.sophia`）。
    pub rel_path: String,
    /// 源码（strip-assist 门禁需重解析，故保留）。
    pub source: String,
    /// AST。
    pub ast: Ast,
    /// 语法诊断（容错解析的产物）。
    pub syntax_diags: Vec<SyntaxDiagnostic>,
}

/// 一个已装载的项目。
pub struct Project {
    pub files: Vec<LoadedFile>,
}

impl Project {
    /// 从项目根装载全部 domain 源文件。
    ///
    /// `root` 应包含 `domains/` 目录（由 `sophia init` 创建）。
    pub fn load(root: &Path) -> Result<Self> {
        let domains_dir = root.join("domains");
        if !domains_dir.is_dir() {
            anyhow::bail!(
                "未找到 domains 目录：{}（先运行 `sophia init`？）",
                domains_dir.display()
            );
        }

        let mut paths = Vec::new();
        collect_sophia_files(&domains_dir, &mut paths)
            .with_context(|| format!("扫描 {} 失败", domains_dir.display()))?;
        // 字典序，保证确定性。
        paths.sort();

        let mut files = Vec::new();
        for path in paths {
            let source = std::fs::read_to_string(&path)
                .with_context(|| format!("读取 {} 失败", path.display()))?;
            let rel_path = rel_to_root(root, &path);
            let domain = domain_of(&rel_path).with_context(|| {
                format!("无法从路径推导 domain：{rel_path}（应为 domains/<Domain>/...）")
            })?;
            let tree = parse_str(source.clone()).context("解析失败")?;
            let syntax_diags = tree.errors();
            let ast = tree.to_ast();
            files.push(LoadedFile {
                domain,
                rel_path,
                source,
                ast,
                syntax_diags,
            });
        }
        Ok(Project { files })
    }

    /// 是否存在任何语法错误。
    pub fn has_syntax_errors(&self) -> bool {
        self.files.iter().any(|f| !f.syntax_diags.is_empty())
    }
}

/// 递归收集目录下全部 `.sophia` 文件。
fn collect_sophia_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<_, _>>()?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_sophia_files(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("sophia") {
            out.push(path);
        }
    }
    Ok(())
}

/// 相对项目根的正斜杠路径。
fn rel_to_root(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// 从相对路径 `domains/<Domain>/...` 推导 domain 名。
fn domain_of(rel_path: &str) -> Option<String> {
    let segs: Vec<&str> = rel_path.split('/').collect();
    match (segs.first(), segs.get(1)) {
        (Some(&"domains"), Some(domain)) => Some((*domain).to_string()),
        _ => None,
    }
}
