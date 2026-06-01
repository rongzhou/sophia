//! Sophia Language Server。
//!
//! 见 docs/engineering_architecture.md 第十节。架构原则：**基于 semantic data
//! 工作，而不是直接遍历 AST**——分析在 [`analysis`]（协议无关、可测试）中完成，
//! tower-lsp 外壳（[`server`]）只做协议投影。
//!
//! 起步功能（10.2 子集）：diagnostics（publishDiagnostics）、hover、goto definition；
//! 文档同步用 FULL，每次变更全量重算（增量分析后续引入，接口已是 query 风格）。

#![forbid(unsafe_code)]

mod analysis;
mod convert;
mod server;

pub use analysis::{Diagnostic, DiagnosticSource, SymbolDef, Workspace};
pub use server::SophiaLanguageServer;

use tower_lsp::{LspService, Server};

/// 以 stdio 运行 Language Server（CLI `sophia lsp` 入口的承载）。
pub async fn run_stdio() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(SophiaLanguageServer::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
