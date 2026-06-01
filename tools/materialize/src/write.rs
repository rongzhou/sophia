//! 原子文件写入。
//!
//! 见 docs/language_design.md 10.10：materialize 必须原子——先写临时目录，
//! preflight 通过后再替换目标文件。这避免半写状态污染 `domains/`。
//!
//! 实现：先把全部文件写入目标根下的隐藏 staging 目录（同一文件系统，保证 rename
//! 原子且无跨设备拷贝），全部写成功后逐个 rename 到最终路径；任一步失败则清理
//! staging，不触碰已有目标文件。

use crate::error::{MaterializeError, MaterializeResult};
use std::path::Path;

/// 把 `(相对路径, 内容)` 文件集合原子写入 `target_root`。
///
/// 相对路径使用正斜杠；父目录按需创建。
pub fn atomic_write_all(target_root: &Path, files: &[(String, String)]) -> MaterializeResult<()> {
    let staging = target_root.join(".sophia-staging");
    // 清理可能残留的旧 staging。
    let _ = std::fs::remove_dir_all(&staging);
    mkdir(&staging)?;

    // 阶段一：全部写入 staging。任一失败即清理并返回。
    let mut staged: Vec<(std::path::PathBuf, std::path::PathBuf)> = Vec::new();
    for (rel, content) in files {
        let rel_path = sanitize_rel(rel)?;
        let staged_path = staging.join(&rel_path);
        let final_path = target_root.join(&rel_path);
        if let Some(parent) = staged_path.parent() {
            mkdir(parent)?;
        }
        if let Err(e) = std::fs::write(&staged_path, content) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(MaterializeError::Write(format!(
                "写入 staging `{}` 失败：{e}",
                rel
            )));
        }
        staged.push((staged_path, final_path));
    }

    // 阶段二：逐个 rename 到最终位置。
    for (from, to) in &staged {
        if let Some(parent) = to.parent() {
            mkdir(parent)?;
        }
        if let Err(e) = std::fs::rename(from, to) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(MaterializeError::Write(format!(
                "替换目标 `{}` 失败：{e}",
                to.display()
            )));
        }
    }

    let _ = std::fs::remove_dir_all(&staging);
    Ok(())
}

fn mkdir(p: &Path) -> MaterializeResult<()> {
    std::fs::create_dir_all(p)
        .map_err(|e| MaterializeError::Write(format!("创建目录 `{}` 失败：{e}", p.display())))
}

/// 校验并规范化相对路径：拒绝绝对路径与 `..` 逃逸，避免写出 target_root 之外。
fn sanitize_rel(rel: &str) -> MaterializeResult<std::path::PathBuf> {
    let path = Path::new(rel);
    if path.is_absolute() {
        return Err(MaterializeError::Write(format!("拒绝绝对路径 `{rel}`")));
    }
    for comp in path.components() {
        if matches!(comp, std::path::Component::ParentDir) {
            return Err(MaterializeError::Write(format!(
                "拒绝 `..` 路径逃逸 `{rel}`"
            )));
        }
    }
    Ok(path.to_path_buf())
}
