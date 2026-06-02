use fx_core::{Analysis, SourceFile, analyze_workspace};
use fx_syntax::span::Span;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverContents, HoverParams,
    InitializeParams, InitializeResult, Location, MarkedString, OneOf, Position, Range,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, Url,
};
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Debug)]
struct Backend {
    client: Client,
    state: Arc<Mutex<ServerState>>,
}

#[derive(Debug, Default)]
struct ServerState {
    root: Option<PathBuf>,
    documents: BTreeMap<Url, Document>,
}

#[derive(Debug, Clone)]
struct Document {
    uri: Url,
    path: PathBuf,
    text: String,
    line_index: LineIndex,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        self.state.lock().await.root = params.root_uri.and_then(|uri| uri.to_file_path().ok());

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(tower_lsp::lsp_types::HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
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
        if let Ok(path) = params.text_document.uri.to_file_path() {
            let document = Document::new(
                params.text_document.uri.clone(),
                path,
                params.text_document.text,
            );
            self.state
                .lock()
                .await
                .documents
                .insert(params.text_document.uri.clone(), document);
            self.publish(params.text_document.uri).await;
        }
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().next() {
            let mut state = self.state.lock().await;
            if let Some(document) = state.documents.get_mut(&params.text_document.uri) {
                document.text = change.text;
                document.line_index = LineIndex::new(&document.text);
            }
            drop(state);
            self.publish(params.text_document.uri).await;
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let Some((analysis, document)) = self.analysis_for_uri(&uri).await else {
            return Ok(None);
        };
        let Some(module) = analysis.module_for_path(&document.path) else {
            return Ok(None);
        };
        let Some(offset) = document.line_index.position_to_offset(position) else {
            return Ok(None);
        };
        let Some(hover) = analysis.hover_at(module, offset) else {
            return Ok(None);
        };

        Ok(Some(Hover {
            contents: HoverContents::Scalar(MarkedString::String(hover.contents)),
            range: Some(document.line_index.span_to_range(hover.span)),
        }))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let Some((analysis, document)) = self.analysis_for_uri(&uri).await else {
            return Ok(None);
        };
        let Some(module) = analysis.module_for_path(&document.path) else {
            return Ok(None);
        };
        let Some(offset) = document.line_index.position_to_offset(position) else {
            return Ok(None);
        };
        let Some(definition) = analysis.definition_at(module, offset) else {
            return Ok(None);
        };
        let Some(target_module) = analysis.graph.modules.get(&definition.module) else {
            return Ok(None);
        };
        let Ok(target_uri) = Url::from_file_path(&definition.path) else {
            return Ok(None);
        };
        let target_line_index = LineIndex::new(&target_module.parsed.source);

        Ok(Some(GotoDefinitionResponse::Scalar(Location {
            uri: target_uri,
            range: target_line_index.span_to_range(definition.span),
        })))
    }
}

impl Backend {
    async fn publish(&self, uri: Url) {
        let Some((analysis, document)) = self.analysis_for_uri(&uri).await else {
            return;
        };
        let diagnostics = analysis
            .check
            .diagnostics
            .into_iter()
            .map(|diagnostic| Diagnostic {
                range: document.line_index.span_to_range(diagnostic.span),
                severity: Some(DiagnosticSeverity::ERROR),
                message: diagnostic.message,
                ..Diagnostic::default()
            })
            .collect();
        self.client
            .publish_diagnostics(document.uri, diagnostics, None)
            .await;
    }

    async fn analysis_for_uri(&self, uri: &Url) -> Option<(Analysis, Document)> {
        let state = self.state.lock().await;
        let document = state.documents.get(uri)?.clone();
        let root = state
            .root
            .clone()
            .or_else(|| document.path.parent().map(Path::to_path_buf))?;
        let open_documents = state.documents.values().cloned().collect::<Vec<_>>();
        drop(state);

        let mut files = collect_workspace_files(&root);
        for document in open_documents {
            if let Some(existing) = files.iter_mut().find(|file| file.path == document.path) {
                existing.source = document.text.clone();
            } else {
                files.push(SourceFile {
                    path: document.path.clone(),
                    source: document.text.clone(),
                });
            }
        }

        Some((analyze_workspace(&root, files), document))
    }
}

impl Document {
    fn new(uri: Url, path: PathBuf, text: String) -> Self {
        let line_index = LineIndex::new(&text);
        Self {
            uri,
            path,
            text,
            line_index,
        }
    }
}

fn collect_workspace_files(root: &Path) -> Vec<SourceFile> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "fx"))
        .filter_map(|entry| {
            fs::read_to_string(entry.path())
                .ok()
                .map(|source| SourceFile {
                    path: entry.path().to_path_buf(),
                    source,
                })
        })
        .collect()
}

#[derive(Debug, Clone)]
struct LineIndex {
    text: String,
    line_starts: Vec<usize>,
}

impl LineIndex {
    fn new(text: &str) -> Self {
        let mut line_starts = vec![0];
        for (offset, ch) in text.char_indices() {
            if ch == '\n' {
                line_starts.push(offset + ch.len_utf8());
            }
        }
        Self {
            text: text.to_string(),
            line_starts,
        }
    }

    fn position_to_offset(&self, position: Position) -> Option<usize> {
        let line_start = *self.line_starts.get(position.line as usize)?;
        let line_end = self
            .line_starts
            .get(position.line as usize + 1)
            .copied()
            .unwrap_or(self.text.len());
        let mut utf16 = 0u32;
        for (relative, ch) in self.text[line_start..line_end].char_indices() {
            if utf16 == position.character {
                return Some(line_start + relative);
            }
            utf16 += ch.len_utf16() as u32;
            if utf16 > position.character {
                return Some(line_start + relative);
            }
        }
        (utf16 == position.character).then_some(line_end)
    }

    fn offset_to_position(&self, offset: usize) -> Position {
        let line = self
            .line_starts
            .partition_point(|line_start| *line_start <= offset)
            .saturating_sub(1);
        let line_start = self.line_starts[line];
        let character = self.text[line_start..offset.min(self.text.len())]
            .chars()
            .map(|ch| ch.len_utf16() as u32)
            .sum();
        Position {
            line: line as u32,
            character,
        }
    }

    fn span_to_range(&self, span: Span) -> Range {
        Range {
            start: self.offset_to_position(span.start),
            end: self.offset_to_position(span.end),
        }
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        state: Arc::new(Mutex::new(ServerState::default())),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}

#[cfg(test)]
mod tests {
    use super::LineIndex;
    use fx_syntax::span::Span;
    use tower_lsp::lsp_types::Position;

    #[test]
    fn line_index_converts_multiline_offsets() {
        let index = LineIndex::new("one\ntwo\nthree");

        assert_eq!(
            index.offset_to_position(5),
            Position {
                line: 1,
                character: 1
            }
        );
        assert_eq!(
            index.position_to_offset(Position {
                line: 2,
                character: 2
            }),
            Some(10)
        );
        assert_eq!(
            index.span_to_range(Span::new(4, 7)).start,
            Position {
                line: 1,
                character: 0
            }
        );
    }

    #[test]
    fn line_index_uses_utf16_character_offsets() {
        let index = LineIndex::new("a😀b");

        assert_eq!(
            index.offset_to_position("a😀".len()),
            Position {
                line: 0,
                character: 3
            }
        );
        assert_eq!(
            index.position_to_offset(Position {
                line: 0,
                character: 3
            }),
            Some("a😀".len())
        );
    }
}
