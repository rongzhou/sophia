//! 三方库的启动时一次性发现（见 docs/stdlib_design.md §五.1）。
//!
//! 进程启动时按约定顺序扫描三方根目录,逐子目录读 `library.toml` + 资产 + `src/*.sophia`,组装
//! [`LibraryContent`] 交 [`LibraryRegistry::build`]（与标准库合并、冲突校验）。**只在启动做一次**,
//! 注册表随后冻结。失败一律 `Err`(启动期诚实报错退出,不静默跳过 / 部分加载 / 覆盖同名)。
//!
//! 本模块**做文件 IO**（归 sophia-stdlib 内容 / 装载层正当）；`core` 不依赖它。

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use sophia_library::{LibraryContent, LibraryRegistry, RawManifest};

use crate::stdlib_contents;

/// 发现错误（启动期硬错误,导致诚实报错退出）。
#[derive(Debug)]
pub enum DiscoverError {
    /// 读目录 / 文件失败。
    Io { path: PathBuf, reason: String },
    /// 清单 TOML 解析失败。
    Manifest { dir: String, reason: String },
    /// 注册表构建失败（冲突 / abi / TypeDesc 等）。
    Registry(sophia_library::LibraryError),
}

impl std::fmt::Display for DiscoverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiscoverError::Io { path, reason } => {
                write!(f, "三方库发现 IO 失败 [{}]：{reason}", path.display())
            }
            DiscoverError::Manifest { dir, reason } => {
                write!(f, "三方库 `{dir}` 清单解析失败：{reason}")
            }
            DiscoverError::Registry(e) => write!(f, "库注册表构建失败：{e}"),
        }
    }
}

impl std::error::Error for DiscoverError {}

type Result<T> = std::result::Result<T, DiscoverError>;

/// 项目根相对的三方库根目录：① `<project_root>/sophia_libs/`,② 环境变量 `$SOPHIA_LIB_PATH`
/// （平台 path-list 语义,机器级三方库路径）。返回存在的根目录列表（不存在的根静默忽略——根缺失不是
/// 错误,库缺失才是）。
///
/// CLI 各命令以显式 `--root` 定位项目（非进程 CWD）,故按项目根解析本地 `sophia_libs/`。
pub fn project_roots(project_root: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let local = project_root.join("sophia_libs");
    if local.is_dir() {
        roots.push(local);
    }
    // `$SOPHIA_LIB_PATH` 采用平台 path-list 语义（Unix `:` / Windows `;`）。
    if let Some(path_var) = std::env::var_os("SOPHIA_LIB_PATH") {
        roots.extend(env_roots_from(&path_var));
    }
    roots
}

fn env_roots_from(path_var: &OsStr) -> Vec<PathBuf> {
    std::env::split_paths(path_var)
        .filter(|p| !p.as_os_str().is_empty() && p.is_dir())
        .collect()
}

/// 完整注册表 = 标准库 + 以**项目根**解析的三方库（`<root>/sophia_libs/` + `$SOPHIA_LIB_PATH`）。
/// CLI 启动入口（`sophia run` / `check` / `graph` 等据 `--root` 调用）。
///
/// 确定性测试 / `check_program` 仍用 [`crate::standard_registry`]（三方发现是启动行为,不进核心
/// 确定性门禁）。
pub fn full_registry_for(project_root: &Path) -> Result<LibraryRegistry> {
    full_registry_from(&project_roots(project_root))
}

/// 从指定三方根目录集发现并构建完整注册表（标准库 + 三方）。供测试用 fixture 根显式调用
/// （不读环境变量,确定）。
pub fn full_registry_from(roots: &[PathBuf]) -> Result<LibraryRegistry> {
    let mut contents = stdlib_contents();
    for root in roots {
        contents.extend(discover_root(root)?);
    }
    LibraryRegistry::build(contents).map_err(DiscoverError::Registry)
}

/// 扫描一个三方根目录下的全部库子目录,返回各库的 [`LibraryContent`]。
fn discover_root(root: &Path) -> Result<Vec<LibraryContent>> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(root).map_err(|e| DiscoverError::Io {
        path: root.to_path_buf(),
        reason: e.to_string(),
    })?;
    // 收集 + 按目录名排序,确定性（不依赖文件系统枚举顺序）。
    let mut dirs: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| DiscoverError::Io {
            path: root.to_path_buf(),
            reason: e.to_string(),
        })?;
        let path = entry.path();
        if path.is_dir() {
            dirs.push(path);
        }
    }
    dirs.sort();
    for dir in dirs {
        out.push(read_library_dir(&dir)?);
    }
    Ok(out)
}

/// 读一个库目录:`library.toml` + 资产（清单 `[prompt].asset`）+ Sophia 源码（清单
/// `[surface].sophia_sources`）。源码逻辑路径保持为库内相对路径；库名/domain 由注册表单独保存。
fn read_library_dir(dir: &Path) -> Result<LibraryContent> {
    let dir_name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let manifest_toml = read_file(&dir.join("library.toml"))?;

    // 先解析清单（仅用于得知资产 / 源码文件名;注册表构建会再解析一次做语义校验——这里解析失败
    // 也提前报错,定位更清晰）。
    let raw = RawManifest::from_toml(&manifest_toml).map_err(|reason| DiscoverError::Manifest {
        dir: dir_name.clone(),
        reason,
    })?;

    let asset_text = read_file(&dir.join(raw.prompt_asset()))?;

    let mut sophia_sources = Vec::new();
    for rel in raw.sophia_source_paths() {
        let source = read_file(&dir.join(rel))?;
        sophia_sources.push((rel.clone(), source));
    }

    // 库目录下 `host.wasm`（三方 WASM-effect 库提供）：存在则读字节,缺失为 None（纯 Sophia /
    // 无 effect-op 库）。读失败（存在但读不了）一律诚实 `Err`,不静默降级。
    let host_wasm_path = dir.join("host.wasm");
    let host_wasm = if host_wasm_path.is_file() {
        Some(read_bytes(&host_wasm_path)?)
    } else {
        None
    };

    Ok(LibraryContent {
        dir_name,
        manifest_toml,
        asset_text,
        sophia_sources,
        host_wasm,
    })
}

fn read_file(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| DiscoverError::Io {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })
}

fn read_bytes(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|e| DiscoverError::Io {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "sophia_stdlib_discover_{tag}_{}",
            std::process::id()
        ))
    }

    #[test]
    fn env_roots_use_platform_path_list_semantics() {
        let a = temp_dir("a");
        let b = temp_dir("b");
        let missing = temp_dir("missing");
        std::fs::create_dir_all(&a).expect("create a");
        std::fs::create_dir_all(&b).expect("create b");
        let path_var = std::env::join_paths([&a, &missing, &b]).expect("join paths");

        let roots = env_roots_from(&path_var);

        assert_eq!(roots, vec![a, b]);
    }

    #[test]
    fn discovered_sophia_sources_keep_manifest_relative_paths() {
        let root = temp_dir("relative_sources");
        let lib = root.join("math_sophia");
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(lib.join("src")).expect("create lib");
        std::fs::write(
            lib.join("library.toml"),
            r#"[library]
name = "math_sophia"
summary = "测试库"
abi_version = 1

[surface]
sophia_sources = ["src/double.sophia"]

[prompt]
asset = "math_sophia.md"
"#,
        )
        .expect("write manifest");
        std::fs::write(lib.join("math_sophia.md"), "测试资产").expect("write asset");
        std::fs::write(
            lib.join("src/double.sophia"),
            "action LibDouble { input { n: Int } output { y: Int } body { return n + n } }",
        )
        .expect("write source");

        let contents = discover_root(&root).expect("discover root");

        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].sophia_sources[0].0, "src/double.sophia");
        std::fs::remove_dir_all(&root).ok();
    }
}
