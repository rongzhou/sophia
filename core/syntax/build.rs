//! 编译 Tree-sitter 生成的 C parser。
//!
//! 单一路线：仅依赖本地 `src/parser.c`（由 `tree-sitter generate --abi 15` 生成）。
//! 不嵌入外部 git 仓库，符合 docs/engineering_notes.md 的 vendor 策略。

use std::path::Path;

fn main() {
    let src_dir = Path::new("src");
    let parser_c = src_dir.join("parser.c");

    println!("cargo:rerun-if-changed={}", parser_c.display());
    println!("cargo:rerun-if-changed=grammar.js");

    let mut build = cc::Build::new();
    build.include(src_dir);
    build.file(&parser_c);
    build.flag_if_supported("-Wno-unused-parameter");
    build.flag_if_supported("-Wno-unused-but-set-variable");
    build.flag_if_supported("-std=c11");
    build.compile("tree-sitter-sophia");
}
