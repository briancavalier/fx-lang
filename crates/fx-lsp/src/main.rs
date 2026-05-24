use fx_core::{SourceFile, check_workspace, index_workspace};
use std::path::PathBuf;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams, DidOpenTextDocumentParams, Hover,
    HoverContents, HoverParams, InitializeParams, InitializeResult, MarkedString, OneOf, Position,
    Range, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
};
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Debug)]
struct Backend {
    client: Client,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(tower_lsp::lsp_types::HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                ..ServerCapabilities::default()
            },
            server_info: None,
        })
    }

    async fn initialized(&self, _: tower_lsp::lsp_types::InitializedParams) {}

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.publish(
            params.text_document.uri.to_string(),
            params.text_document.text,
        )
        .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().next() {
            self.publish(params.text_document.uri.to_string(), change.text)
                .await;
        }
    }

    async fn hover(&self, _: HoverParams) -> Result<Option<Hover>> {
        Ok(Some(Hover {
            contents: HoverContents::Scalar(MarkedString::String(
                "fx Phase 1 hover data is wired to the checker in a later slice".to_string(),
            )),
            range: None,
        }))
    }
}

impl Backend {
    async fn publish(&self, uri: String, source: String) {
        let path = PathBuf::from(uri.strip_prefix("file://").unwrap_or(&uri));
        let root = path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf();
        let graph = index_workspace(&root, vec![SourceFile { path, source }]);
        let result = check_workspace(&graph);
        let diagnostics = result
            .diagnostics
            .into_iter()
            .map(|diagnostic| Diagnostic {
                range: byte_range_to_lsp(&diagnostic.span),
                severity: Some(DiagnosticSeverity::ERROR),
                message: diagnostic.message,
                ..Diagnostic::default()
            })
            .collect();
        if let Ok(url) = uri.parse() {
            self.client
                .publish_diagnostics(url, diagnostics, None)
                .await;
        }
    }
}

fn byte_range_to_lsp(span: &fx_syntax::span::Span) -> Range {
    Range {
        start: Position {
            line: 0,
            character: span.start as u32,
        },
        end: Position {
            line: 0,
            character: span.end as u32,
        },
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend { client });
    Server::new(stdin, stdout, socket).serve(service).await;
}
