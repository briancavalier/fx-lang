use fx_core::{
    Analysis, AnalysisDiagnostic, SourceFile, Symbol, SymbolKind as FxSymbolKind, analyze_workspace,
};
use fx_syntax::span::Span;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidOpenTextDocumentParams, DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse,
    GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverContents, HoverParams,
    InitializeParams, InitializeResult, Location, MarkedString, OneOf, Position, Range,
    ServerCapabilities, SymbolInformation, SymbolKind as LspSymbolKind, TextDocumentSyncCapability,
    TextDocumentSyncKind, Url, WorkspaceSymbolParams,
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

#[derive(Debug)]
struct AnalysisSnapshot {
    analysis: Analysis,
    document: Document,
    open_documents: Vec<Document>,
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
        let Some(snapshot) = self.analysis_for_uri(&uri).await else {
            return Ok(None);
        };
        let Some(module) = snapshot.analysis.module_for_path(&snapshot.document.path) else {
            return Ok(None);
        };
        let Some(offset) = snapshot.document.line_index.position_to_offset(position) else {
            return Ok(None);
        };
        let Some(hover) = snapshot.analysis.hover_at(module, offset) else {
            return Ok(None);
        };

        Ok(Some(Hover {
            contents: HoverContents::Scalar(MarkedString::String(hover.contents)),
            range: Some(snapshot.document.line_index.span_to_range(hover.span)),
        }))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let Some(snapshot) = self.analysis_for_uri(&uri).await else {
            return Ok(None);
        };
        let Some(module) = snapshot.analysis.module_for_path(&snapshot.document.path) else {
            return Ok(None);
        };
        let Some(offset) = snapshot.document.line_index.position_to_offset(position) else {
            return Ok(None);
        };
        let Some(definition) = snapshot.analysis.definition_at(module, offset) else {
            return Ok(None);
        };
        let Some(target_module) = snapshot.analysis.graph.modules.get(&definition.module) else {
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

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;
        let Some(snapshot) = self.analysis_for_uri(&uri).await else {
            return Ok(None);
        };
        Ok(Some(DocumentSymbolResponse::Nested(
            document_symbols_for_document(&snapshot.analysis, &snapshot.document),
        )))
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let Some(analysis) = self.analysis_for_workspace().await else {
            return Ok(None);
        };
        Ok(Some(workspace_symbols_for_analysis(
            &analysis,
            params.query.as_str(),
        )))
    }
}

impl Backend {
    async fn publish(&self, uri: Url) {
        let Some(snapshot) = self.analysis_for_uri(&uri).await else {
            return;
        };
        let diagnostics_by_path = snapshot.analysis.diagnostics_by_path();
        for document in snapshot.open_documents {
            if !is_fx_path(&document.path) {
                continue;
            }
            let diagnostics = diagnostics_by_path
                .get(&document.path)
                .map(|diagnostics| lsp_diagnostics_for_document(&document, diagnostics))
                .unwrap_or_default();
            self.client
                .publish_diagnostics(document.uri, diagnostics, None)
                .await;
        }
    }

    async fn analysis_for_uri(&self, uri: &Url) -> Option<AnalysisSnapshot> {
        let state = self.state.lock().await;
        let document = state.documents.get(uri)?.clone();
        let root = state
            .root
            .clone()
            .or_else(|| document.path.parent().map(Path::to_path_buf))?;
        let open_documents = state.documents.values().cloned().collect::<Vec<_>>();
        drop(state);

        let mut files = collect_workspace_files(&root);
        for document in &open_documents {
            if let Some(existing) = files.iter_mut().find(|file| file.path == document.path) {
                existing.source = document.text.clone();
            } else {
                files.push(SourceFile {
                    path: document.path.clone(),
                    source: document.text.clone(),
                });
            }
        }

        Some(AnalysisSnapshot {
            analysis: analyze_workspace(&root, files),
            document,
            open_documents,
        })
    }

    async fn analysis_for_workspace(&self) -> Option<Analysis> {
        let state = self.state.lock().await;
        let open_documents = state.documents.values().cloned().collect::<Vec<_>>();
        let root = state.root.clone().or_else(|| {
            open_documents
                .first()
                .and_then(|document| document.path.parent().map(Path::to_path_buf))
        })?;
        drop(state);

        Some(analyze_workspace(
            &root,
            workspace_files_with_open_overlays(&root, &open_documents),
        ))
    }
}

fn workspace_files_with_open_overlays(root: &Path, open_documents: &[Document]) -> Vec<SourceFile> {
    let mut files = collect_workspace_files(root);
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
    files
}

fn document_symbols_for_document(analysis: &Analysis, document: &Document) -> Vec<DocumentSymbol> {
    let Some(module) = analysis.module_for_path(&document.path) else {
        return Vec::new();
    };
    let Some(indexed) = analysis.graph.modules.get(module) else {
        return Vec::new();
    };
    let children = indexed
        .symbols
        .iter()
        .map(|symbol| document_symbol_for_indexed_symbol(symbol, &document.line_index))
        .collect::<Vec<_>>();
    vec![module_document_symbol(
        module,
        indexed.parsed.source.len(),
        &document.line_index,
        children,
    )]
}

#[allow(deprecated)]
fn module_document_symbol(
    module: &str,
    source_len: usize,
    line_index: &LineIndex,
    children: Vec<DocumentSymbol>,
) -> DocumentSymbol {
    DocumentSymbol {
        name: module.to_string(),
        detail: Some("module".to_string()),
        kind: LspSymbolKind::MODULE,
        tags: None,
        deprecated: None,
        range: line_index.span_to_range(Span::new(0, source_len)),
        selection_range: line_index.span_to_range(Span::point(0)),
        children: Some(children),
    }
}

#[allow(deprecated)]
fn document_symbol_for_indexed_symbol(symbol: &Symbol, line_index: &LineIndex) -> DocumentSymbol {
    DocumentSymbol {
        name: symbol.name.clone(),
        detail: Some(symbol.qualified_name.clone()),
        kind: lsp_symbol_kind(symbol.kind),
        tags: None,
        deprecated: None,
        range: line_index.span_to_range(symbol.span),
        selection_range: line_index.span_to_range(symbol.span),
        children: None,
    }
}

fn workspace_symbols_for_analysis(analysis: &Analysis, query: &str) -> Vec<SymbolInformation> {
    let query = query.to_lowercase();
    let mut symbols = Vec::new();
    for module in analysis.graph.modules.values() {
        if module.name.to_lowercase().contains(&query)
            && let Some(symbol) = module_symbol_information(module)
        {
            symbols.push(symbol);
        }
        for symbol in &module.symbols {
            if workspace_symbol_matches(symbol, &query)
                && let Some(info) = symbol_information_for_indexed_symbol(module, symbol)
            {
                symbols.push(info);
            }
        }
    }
    symbols
}

fn workspace_symbol_matches(symbol: &Symbol, query: &str) -> bool {
    symbol.name.to_lowercase().contains(query)
        || symbol.qualified_name.to_lowercase().contains(query)
        || symbol.module.to_lowercase().contains(query)
}

#[allow(deprecated)]
fn module_symbol_information(module: &fx_core::module::IndexedModule) -> Option<SymbolInformation> {
    let uri = Url::from_file_path(&module.path).ok()?;
    let line_index = LineIndex::new(&module.parsed.source);
    Some(SymbolInformation {
        name: module.name.clone(),
        kind: LspSymbolKind::MODULE,
        tags: None,
        deprecated: None,
        location: Location {
            uri,
            range: line_index.span_to_range(Span::new(0, module.parsed.source.len())),
        },
        container_name: None,
    })
}

#[allow(deprecated)]
fn symbol_information_for_indexed_symbol(
    module: &fx_core::module::IndexedModule,
    symbol: &Symbol,
) -> Option<SymbolInformation> {
    let uri = Url::from_file_path(&module.path).ok()?;
    let line_index = LineIndex::new(&module.parsed.source);
    Some(SymbolInformation {
        name: symbol.name.clone(),
        kind: lsp_symbol_kind(symbol.kind),
        tags: None,
        deprecated: None,
        location: Location {
            uri,
            range: line_index.span_to_range(symbol.span),
        },
        container_name: Some(symbol.module.clone()),
    })
}

fn lsp_symbol_kind(kind: FxSymbolKind) -> LspSymbolKind {
    match kind {
        FxSymbolKind::Value => LspSymbolKind::FUNCTION,
        FxSymbolKind::Type => LspSymbolKind::STRUCT,
        FxSymbolKind::Constructor => LspSymbolKind::CONSTRUCTOR,
        FxSymbolKind::Effect => LspSymbolKind::INTERFACE,
        FxSymbolKind::Operation => LspSymbolKind::FUNCTION,
    }
}

fn lsp_diagnostics_for_document(
    document: &Document,
    diagnostics: &[AnalysisDiagnostic],
) -> Vec<LspDiagnostic> {
    diagnostics
        .iter()
        .map(|diagnostic| LspDiagnostic {
            range: document.line_index.span_to_range(diagnostic.span),
            severity: Some(DiagnosticSeverity::ERROR),
            message: diagnostic.message.clone(),
            ..LspDiagnostic::default()
        })
        .collect()
}

fn is_fx_path(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "fx")
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
    use super::{
        Document, LineIndex, document_symbols_for_document, lsp_diagnostics_for_document,
        workspace_symbols_for_analysis,
    };
    use fx_core::{AnalysisDiagnostic, SourceFile, analyze_workspace};
    use fx_syntax::span::Span;
    use std::path::PathBuf;
    use tower_lsp::lsp_types::Url;
    use tower_lsp::lsp_types::{Position, SymbolKind};

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

    #[test]
    fn diagnostic_conversion_uses_each_document_line_index() {
        let first = document("/private/tmp/one.fx", "bad\n");
        let second = document("/private/tmp/two.fx", "\n\nbad\n");
        let span = Span::new(0, 3);

        let first_diagnostics = lsp_diagnostics_for_document(&first, &[analysis_diagnostic(span)]);
        let second_diagnostics =
            lsp_diagnostics_for_document(&second, &[analysis_diagnostic(Span::new(2, 5))]);

        assert_eq!(
            first_diagnostics[0].range.start,
            Position {
                line: 0,
                character: 0
            }
        );
        assert_eq!(
            second_diagnostics[0].range.start,
            Position {
                line: 2,
                character: 0
            }
        );
    }

    #[test]
    fn document_symbols_include_module_and_indexed_children() {
        let source = "export type Todo = < Done() >\n\nsummarize(todo: Todo) : Text\n  \"ok\"\n";
        let analysis = analysis_with_files(vec![source_file("/private/tmp/main.fx", source)]);
        let document = document("/private/tmp/main.fx", source);

        let symbols = document_symbols_for_document(&analysis, &document);

        assert_eq!(symbols[0].name, "main");
        assert_eq!(symbols[0].kind, SymbolKind::MODULE);
        let children = symbols[0].children.as_ref().expect("children");
        assert!(children.iter().any(|symbol| symbol.name == "Todo"));
        assert!(children.iter().any(|symbol| symbol.name == "Done"));
        assert!(children.iter().any(|symbol| symbol.name == "summarize"));
    }

    #[test]
    fn workspace_symbols_include_modules_and_filter_indexed_symbols() {
        let analysis = analysis_with_files(vec![
            source_file("/private/tmp/main.fx", "main() : Int\n  1\n"),
            source_file("/private/tmp/todo.fx", "export type Todo = < Done() >\n"),
        ]);

        let todo_symbols = workspace_symbols_for_analysis(&analysis, "todo");
        let done_symbols = workspace_symbols_for_analysis(&analysis, "done");

        assert!(
            todo_symbols
                .iter()
                .any(|symbol| { symbol.name == "todo" && symbol.kind == SymbolKind::MODULE })
        );
        assert!(todo_symbols.iter().any(|symbol| symbol.name == "Todo"));
        assert!(done_symbols.iter().any(|symbol| symbol.name == "Done"));
        assert!(!done_symbols.iter().any(|symbol| symbol.name == "main"));
    }

    #[test]
    fn phase_2_rows_fixture_has_no_lsp_diagnostics_and_has_symbols() {
        let source = include_str!("../../../examples/phase-2/rows.fx");
        let path = "/private/tmp/rows.fx";
        let analysis = analysis_with_files(vec![source_file(path, source)]);
        let document = document(path, source);
        let diagnostics = analysis
            .diagnostics_by_path()
            .get(&PathBuf::from(path))
            .map(|diagnostics| lsp_diagnostics_for_document(&document, diagnostics))
            .unwrap_or_default();

        assert_eq!(diagnostics, []);

        let symbols = document_symbols_for_document(&analysis, &document);
        let children = symbols[0].children.as_ref().expect("children");
        assert!(children.iter().any(|symbol| symbol.name == "greet"));
        assert!(children.iter().any(|symbol| symbol.name == "message"));

        let workspace_symbols = workspace_symbols_for_analysis(&analysis, "message");
        assert!(
            workspace_symbols
                .iter()
                .any(|symbol| symbol.name == "message")
        );
    }

    fn document(path: &str, text: &str) -> Document {
        Document::new(
            Url::from_file_path(path).expect("file url"),
            PathBuf::from(path),
            text.to_string(),
        )
    }

    fn analysis_diagnostic(span: Span) -> AnalysisDiagnostic {
        AnalysisDiagnostic {
            module: "main".to_string(),
            path: PathBuf::from("/private/tmp/main.fx"),
            span,
            message: "problem".to_string(),
        }
    }

    fn analysis_with_files(files: Vec<SourceFile>) -> fx_core::Analysis {
        analyze_workspace(&PathBuf::from("/private/tmp"), files)
    }

    fn source_file(path: &str, source: &str) -> SourceFile {
        SourceFile {
            path: PathBuf::from(path),
            source: source.to_string(),
        }
    }
}
