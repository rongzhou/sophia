//! tower-lsp 服务器外壳。
//!
//! 见 docs/engineering_architecture.md 第十节。本模块只做 LSP 协议 ↔ [`Workspace`]
//! 语义查询的薄投影：所有分析在 [`crate::analysis`]（协议无关、可测试）中完成。
//!
//! 起步功能：textDocument/didOpen|didChange|didClose、publishDiagnostics、hover、
//! definition。增量分析后续引入；当前每次变更全量重算。

use crate::analysis::{DiagnosticSource, Workspace};
use crate::convert::{byte_to_position, position_to_byte, span_to_range};
use std::sync::Mutex;
use tower_lsp::jsonrpc::Result as RpcResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

/// Sophia Language Server。
pub struct SophiaLanguageServer {
    client: Client,
    workspace: Mutex<Workspace>,
}

impl SophiaLanguageServer {
    pub fn new(client: Client) -> Self {
        SophiaLanguageServer {
            client,
            workspace: Mutex::new(Workspace::new()),
        }
    }

    /// 由 URI 推导 domain：取 `domains/<Domain>/...` 段；无法推导时用文件名。
    fn domain_for(uri: &Url) -> String {
        let path = uri.path();
        let segs: Vec<&str> = path.split('/').collect();
        if let Some(pos) = segs.iter().position(|s| *s == "domains") {
            if let Some(d) = segs.get(pos + 1) {
                return (*d).to_string();
            }
        }
        "default".to_string()
    }

    /// 重算并发布某文档诊断。
    async fn publish(&self, uri: Url) {
        let (source, diags) = {
            let ws = self.workspace.lock().unwrap();
            let key = uri.to_string();
            let source = ws.source(&key).map(|s| s.to_string());
            let diags = ws.diagnostics(&key);
            (source, diags)
        };
        let Some(source) = source else { return };

        let lsp_diags: Vec<Diagnostic> = diags
            .iter()
            .map(|d| Diagnostic {
                range: span_to_range(&source, d.span),
                severity: Some(severity_of(d.source)),
                code: Some(NumberOrString::String(d.code.clone())),
                source: Some("sophia".to_string()),
                message: d.message.clone(),
                ..Default::default()
            })
            .collect();

        self.client.publish_diagnostics(uri, lsp_diags, None).await;
    }
}

fn severity_of(source: DiagnosticSource) -> DiagnosticSeverity {
    match source {
        // 语法 / HIR / 语义错误均为 Error（起步阶段不区分 warning）。
        DiagnosticSource::Syntax | DiagnosticSource::Hir | DiagnosticSource::Semantic => {
            DiagnosticSeverity::ERROR
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for SophiaLanguageServer {
    async fn initialize(&self, _params: InitializeParams) -> RpcResult<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "sophia-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Sophia LSP 已启动")
            .await;
    }

    async fn shutdown(&self) -> RpcResult<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        {
            let mut ws = self.workspace.lock().unwrap();
            ws.upsert(
                uri.to_string(),
                Self::domain_for(&uri),
                params.text_document.text,
            );
        }
        self.publish(uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        // FULL 同步：取最后一个 change 的全文。
        if let Some(change) = params.content_changes.into_iter().last() {
            {
                let mut ws = self.workspace.lock().unwrap();
                ws.upsert(uri.to_string(), Self::domain_for(&uri), change.text);
            }
            self.publish(uri).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        {
            let mut ws = self.workspace.lock().unwrap();
            ws.remove(uri.as_ref());
        }
        // 清空诊断。
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    async fn hover(&self, params: HoverParams) -> RpcResult<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let ws = self.workspace.lock().unwrap();
        let key = uri.to_string();
        let Some(source) = ws.source(&key) else {
            return Ok(None);
        };
        let byte = position_to_byte(source, pos);
        Ok(ws.hover(&key, byte).map(|md| Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: md,
            }),
            range: None,
        }))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> RpcResult<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let ws = self.workspace.lock().unwrap();
        let key = uri.to_string();
        let Some(source) = ws.source(&key) else {
            return Ok(None);
        };
        let byte = position_to_byte(source, pos);
        let Some(def) = ws.goto_definition(&key, byte) else {
            return Ok(None);
        };
        // 定义所在文档的源码（可能是别的文件）。
        let def_source = ws.source(&def.uri).unwrap_or(source);
        let target_uri = match Url::parse(&def.uri) {
            Ok(u) => u,
            Err(_) => return Ok(None),
        };
        let range = Range {
            start: byte_to_position(def_source, def.name_span.start.byte),
            end: byte_to_position(def_source, def.name_span.end.byte),
        };
        Ok(Some(GotoDefinitionResponse::Scalar(Location {
            uri: target_uri,
            range,
        })))
    }
}
