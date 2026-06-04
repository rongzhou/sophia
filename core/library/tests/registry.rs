//! 库注册表构建 / 冲突 / 消费 API 测试。

use sophia_library::{LibraryContent, LibraryError, LibraryRegistry, Scalar, TypeDesc};

fn http_content() -> LibraryContent {
    LibraryContent {
        dir_name: "http".into(),
        manifest_toml: r#"
[library]
name = "http"
summary = "从网络 URL 获取数据（网络请求）"
abi_version = 1

[[op]]
family = "Http"
op = "Get"
params = ["Text"]
returns = "Raw<Text>"
effectful = true
host_fn = "http_get"

[prompt]
asset = "http.md"
"#
        .into(),
        asset_text: "标准库 · Http。仅当任务需要从网络取数据时使用本库。".into(),
        sophia_sources: vec![],
        host_wasm: None,
    }
}

fn file_content() -> LibraryContent {
    LibraryContent {
        dir_name: "file".into(),
        manifest_toml: r#"
[library]
name = "file"
summary = "读写本地文件（读取 / 写入）"
abi_version = 1

[[op]]
family = "File"
op = "Read"
params = ["Text"]
returns = "Raw<Text>"
host_fn = "file_read"

[[op]]
family = "File"
op = "Write"
params = ["Text", "Sanitized<Text>"]
returns = "Unit"
host_fn = "file_write"

[prompt]
asset = "file.md"
"#
        .into(),
        asset_text: "标准库 · File。读写本地文件。".into(),
        sophia_sources: vec![],
        host_wasm: None,
    }
}

#[test]
fn builds_and_exposes_op_contracts() {
    let reg = LibraryRegistry::build(vec![http_content(), file_content()]).unwrap();

    let get = reg.op("Http", "Get").expect("Http.Get");
    assert_eq!(get.lib, "http");
    assert_eq!(get.params, vec![TypeDesc::Scalar(Scalar::Text)]);
    assert_eq!(
        get.returns,
        TypeDesc::Intent {
            intent: "Raw".into(),
            inner: Scalar::Text
        }
    );
    assert!(get.effectful);
    assert_eq!(get.host_fn, "http_get");

    let write = reg.op("File", "Write").expect("File.Write");
    assert_eq!(
        write.params,
        vec![
            TypeDesc::Scalar(Scalar::Text),
            TypeDesc::Intent {
                intent: "Sanitized".into(),
                inner: Scalar::Text
            }
        ]
    );
    assert_eq!(write.returns, TypeDesc::Scalar(Scalar::Unit));

    assert!(reg.is_library_family("Http"));
    assert!(reg.is_library_family("File"));
    assert!(!reg.is_library_family("Console")); // Console 是语言内置，不经库表
}

#[test]
fn catalog_and_preamble_are_deterministic() {
    let reg = LibraryRegistry::build(vec![http_content(), file_content()]).unwrap();
    // 库名字典序：file 在 http 前。
    let catalog = reg.catalog();
    assert!(catalog.starts_with("- `file` —"), "目录字典序：{catalog}");
    assert!(catalog.contains("- `http` —"));

    assert!(reg.preamble(&[]).is_empty(), "空集零注入");
    assert!(reg.preamble(&["http"]).contains("Http"));
    // 去重 + 未知库忽略。
    assert_eq!(reg.preamble(&["http", "http"]), reg.preamble(&["http"]));
    assert!(reg.preamble(&["no_such"]).is_empty());
}

#[test]
fn rejects_name_mismatch() {
    let mut c = http_content();
    c.dir_name = "网络库".into();
    let err = LibraryRegistry::build(vec![c]).unwrap_err();
    assert!(matches!(err, LibraryError::NameMismatch { .. }));
}

#[test]
fn rejects_unsupported_abi() {
    let c = LibraryContent {
        dir_name: "future".into(),
        manifest_toml: r#"
[library]
name = "future"
summary = "x"
abi_version = 999
[prompt]
asset = "future.md"
"#
        .into(),
        asset_text: "x".into(),
        sophia_sources: vec![],
        host_wasm: None,
    };
    let err = LibraryRegistry::build(vec![c]).unwrap_err();
    assert!(matches!(
        err,
        LibraryError::UnsupportedAbi { found: 999, .. }
    ));
}

#[test]
fn rejects_duplicate_family_across_libs() {
    let mut other = http_content();
    other.dir_name = "http2".into();
    other.manifest_toml = other
        .manifest_toml
        .replace("name = \"http\"", "name = \"http2\"");
    let err = LibraryRegistry::build(vec![http_content(), other]).unwrap_err();
    // 同 family.op → DuplicateOp（更具体），同 family 不同 op → DuplicateFamily。
    assert!(matches!(
        err,
        LibraryError::DuplicateOp { .. } | LibraryError::DuplicateFamily { .. }
    ));
}

#[test]
fn rejects_bad_typedesc() {
    let c = LibraryContent {
        dir_name: "bad".into(),
        manifest_toml: r#"
[library]
name = "bad"
summary = "x"
abi_version = 1
[[op]]
family = "Bad"
op = "Do"
params = ["Widget"]
returns = "Unit"
host_fn = "bad_do"
[prompt]
asset = "bad.md"
"#
        .into(),
        asset_text: "x".into(),
        sophia_sources: vec![],
        host_wasm: None,
    };
    let err = LibraryRegistry::build(vec![c]).unwrap_err();
    assert!(matches!(err, LibraryError::BadTypeDesc { .. }));
}

#[test]
fn rejects_duplicate_host_fn_within_library() {
    let c = LibraryContent {
        dir_name: "dup_host".into(),
        manifest_toml: r#"
[library]
name = "dup_host"
summary = "x"
abi_version = 1

[[op]]
family = "Dup"
op = "A"
params = ["Int"]
returns = "Int"
host_fn = "same"

[[op]]
family = "Dup"
op = "B"
params = ["Int"]
returns = "Int"
host_fn = "same"

[prompt]
asset = "dup.md"
"#
        .into(),
        asset_text: "x".into(),
        sophia_sources: vec![],
        host_wasm: None,
    };
    let err = LibraryRegistry::build(vec![c]).unwrap_err();
    assert!(matches!(err, LibraryError::DuplicateHostFn { .. }));
}

#[test]
fn loads_sophia_source_library_into_lib_domain() {
    let c = LibraryContent {
        dir_name: "geo".into(),
        manifest_toml: r#"
[library]
name = "geo"
summary = "几何助手（纯 Sophia）"
abi_version = 1
[surface]
sophia_sources = ["src/area.sophia"]
[prompt]
asset = "geo.md"
"#
        .into(),
        asset_text: "x".into(),
        sophia_sources: vec![(
            "geo/actions/Area.sophia".into(),
            "action Area { input { w: Int; h: Int } output { a: Int } body { return w * h } }"
                .into(),
        )],
        host_wasm: None,
    };
    let reg = LibraryRegistry::build(vec![c]).unwrap();
    let srcs = reg.sophia_sources();
    assert_eq!(srcs.len(), 1);
    assert_eq!(srcs[0].lib, "geo");
    assert_eq!(srcs[0].domain, "geo"); // 库名即 domain（隔离）
}
