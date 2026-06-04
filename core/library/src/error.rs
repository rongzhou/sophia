//! 库层错误。

use thiserror::Error;

/// 当前支持的清单 ABI 版本。发现到更高版本即报错（不静默跳过）。
pub(crate) const SUPPORTED_ABI_VERSION: u32 = 1;

/// 库构建 / 合并错误。
#[derive(Debug, Error, PartialEq, Eq)]
pub enum LibraryError {
    /// 清单 TOML 解析失败。
    #[error("库 `{lib}` 清单解析失败：{reason}")]
    ManifestParse { lib: String, reason: String },

    /// 不支持的清单 ABI 版本。
    #[error("库 `{lib}` 的 abi_version={found} 不受支持（当前支持 {supported}）")]
    UnsupportedAbi {
        lib: String,
        found: u32,
        supported: u32,
    },

    /// 类型描述符语法非法。
    #[error("库 `{lib}` 的类型描述符 `{desc}` 非法：{reason}")]
    BadTypeDesc {
        lib: String,
        desc: String,
        reason: String,
    },

    /// 库名冲突（两个库目录同名）。
    #[error("库名冲突：`{name}` 被多次定义")]
    DuplicateLibrary { name: String },

    /// effect 族冲突（两个库声明同一 family）。
    #[error("effect 族冲突：`{family}` 被库 `{first}` 与 `{second}` 同时声明")]
    DuplicateFamily {
        family: String,
        first: String,
        second: String,
    },

    /// effect 操作冲突（同一 `family.op` 来自不同库）。
    #[error("effect 操作冲突：`{family}.{op}` 被库 `{first}` 与 `{second}` 同时声明")]
    DuplicateOp {
        family: String,
        op: String,
        first: String,
        second: String,
    },

    /// 同一库内 host 分派键冲突。
    #[error("库 `{lib}` 的 host_fn `{host_fn}` 被 `{first_op}` 与 `{second_op}` 同时使用")]
    DuplicateHostFn {
        lib: String,
        host_fn: String,
        first_op: String,
        second_op: String,
    },

    /// Sophia 源码库 domain 冲突。
    #[error("库 domain 冲突：`{domain}` 被库 `{first}` 与 `{second}` 同时占用")]
    DuplicateDomain {
        domain: String,
        first: String,
        second: String,
    },

    /// 清单 name 与目录名不一致。
    #[error("库目录名 `{dir}` 与清单 name `{name}` 不一致")]
    NameMismatch { dir: String, name: String },

    /// 清单声明的 Sophia 源码路径集合与调用方提供的源码集合不一致。
    #[error("库 `{lib}` 的 Sophia 源码集合与清单不一致：{reason}")]
    SophiaSourcesMismatch { lib: String, reason: String },
}

/// 库层结果别名。
pub type LibraryResult<T> = Result<T, LibraryError>;
