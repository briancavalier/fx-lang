use fx_core::{
    Analysis, AnalysisDiagnostic, SourceFile, Symbol, SymbolKind as FxSymbolKind, analyze_workspace,
};
use fx_syntax::span::Span;
use fx_syntax::{SyntaxTokenKind, lex_source};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse,
    Diagnostic as LspDiagnostic, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidOpenTextDocumentParams, DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse,
    GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverContents, HoverParams,
    InitializeParams, InitializeResult, Location, MarkedString, OneOf, Position, Range,
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens,
    SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions, SemanticTokensParams,
    SemanticTokensResult, SemanticTokensServerCapabilities, ServerCapabilities, SymbolInformation,
    SymbolKind as LspSymbolKind, TextDocumentSyncCapability, TextDocumentSyncKind, Url,
    WorkDoneProgressOptions, WorkspaceSymbolParams,
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
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![".".to_string()]),
                    ..CompletionOptions::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            work_done_progress_options: WorkDoneProgressOptions::default(),
                            legend: semantic_tokens_legend(),
                            range: Some(false),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                        },
                    ),
                ),
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

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let Some(snapshot) = self.analysis_for_uri(&uri).await else {
            return Ok(None);
        };
        Ok(Some(CompletionResponse::Array(completions_for_analysis(
            &snapshot.analysis,
        ))))
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

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;
        let Some(snapshot) = self.analysis_for_uri(&uri).await else {
            return Ok(None);
        };
        Ok(Some(SemanticTokensResult::Tokens(
            semantic_tokens_for_document(&snapshot.analysis, &snapshot.document),
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

fn completions_for_analysis(analysis: &Analysis) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    for module in analysis.graph.modules.values() {
        for symbol in &module.symbols {
            let mut item =
                CompletionItem::new_simple(symbol.name.clone(), symbol.qualified_name.clone());
            item.kind = Some(completion_item_kind(symbol.kind));
            if let Some(ty) = analysis
                .check
                .module_types
                .get(&symbol.module)
                .and_then(|types| types.get(&symbol.name))
            {
                item.detail = Some(format!("{} : {ty}", symbol.name));
            }
            items.push(item);
        }
    }
    items
}

fn completion_item_kind(kind: FxSymbolKind) -> CompletionItemKind {
    match kind {
        FxSymbolKind::Value => CompletionItemKind::FUNCTION,
        FxSymbolKind::Type => CompletionItemKind::STRUCT,
        FxSymbolKind::Constructor => CompletionItemKind::CONSTRUCTOR,
        FxSymbolKind::Effect => CompletionItemKind::INTERFACE,
        FxSymbolKind::Operation => CompletionItemKind::FUNCTION,
        FxSymbolKind::Scope => CompletionItemKind::VARIABLE,
    }
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
        FxSymbolKind::Scope => LspSymbolKind::VARIABLE,
    }
}

const TOKEN_COMMENT: u32 = 0;
const TOKEN_KEYWORD: u32 = 1;
const TOKEN_STRING: u32 = 2;
const TOKEN_NUMBER: u32 = 3;
const TOKEN_OPERATOR: u32 = 4;
const TOKEN_FUNCTION: u32 = 5;
const TOKEN_VARIABLE: u32 = 6;
const TOKEN_PARAMETER: u32 = 7;
const TOKEN_PROPERTY: u32 = 8;
const TOKEN_TYPE: u32 = 9;
const TOKEN_ENUM_MEMBER: u32 = 10;
const TOKEN_INTERFACE: u32 = 11;
const TOKEN_NAMESPACE: u32 = 12;

const MOD_DECLARATION: u32 = 1 << 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RawSemanticToken {
    span: Span,
    token_type: u32,
    token_modifiers_bitset: u32,
    priority: u8,
}

fn semantic_tokens_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![
            SemanticTokenType::COMMENT,
            SemanticTokenType::KEYWORD,
            SemanticTokenType::STRING,
            SemanticTokenType::NUMBER,
            SemanticTokenType::OPERATOR,
            SemanticTokenType::FUNCTION,
            SemanticTokenType::VARIABLE,
            SemanticTokenType::PARAMETER,
            SemanticTokenType::PROPERTY,
            SemanticTokenType::TYPE,
            SemanticTokenType::ENUM_MEMBER,
            SemanticTokenType::INTERFACE,
            SemanticTokenType::NAMESPACE,
        ],
        token_modifiers: vec![
            SemanticTokenModifier::DECLARATION,
            SemanticTokenModifier::READONLY,
            SemanticTokenModifier::DEFAULT_LIBRARY,
        ],
    }
}

fn semantic_tokens_for_document(analysis: &Analysis, document: &Document) -> SemanticTokens {
    let mut raw = lexical_semantic_tokens(&document.text);
    if let Some(module_name) = analysis.module_for_path(&document.path) {
        if let Some(module) = analysis.graph.modules.get(module_name) {
            for symbol in &module.symbols {
                raw.push(RawSemanticToken {
                    span: symbol.span,
                    token_type: semantic_token_type_for_symbol_kind(symbol.kind),
                    token_modifiers_bitset: MOD_DECLARATION,
                    priority: 4,
                });
            }
        }

        if let Some(resolved) = analysis.resolved.modules.get(module_name) {
            for reference in &resolved.references {
                raw.push(RawSemanticToken {
                    span: reference.span,
                    token_type: semantic_token_type_for_symbol_kind(reference.target.kind),
                    token_modifiers_bitset: 0,
                    priority: 3,
                });
            }
        }

        if let Some(typed_spans) = analysis.check.typed_spans.get(module_name) {
            for typed in typed_spans {
                raw.push(RawSemanticToken {
                    span: typed.span,
                    token_type: semantic_token_type_for_typed_span_kind(typed.kind),
                    token_modifiers_bitset: 0,
                    priority: 2,
                });
            }
        }
    }

    SemanticTokens {
        result_id: None,
        data: encode_semantic_tokens(raw, &document.line_index),
    }
}

fn lexical_semantic_tokens(source: &str) -> Vec<RawSemanticToken> {
    lex_source(source)
        .tokens
        .into_iter()
        .filter_map(|token| {
            let token_type = match token.kind {
                SyntaxTokenKind::Comment => TOKEN_COMMENT,
                SyntaxTokenKind::Keyword(_) => TOKEN_KEYWORD,
                SyntaxTokenKind::Text(_) => TOKEN_STRING,
                SyntaxTokenKind::Int(_) => TOKEN_NUMBER,
                SyntaxTokenKind::PipeGreater
                | SyntaxTokenKind::Equal
                | SyntaxTokenKind::Arrow
                | SyntaxTokenKind::PlusPlus
                | SyntaxTokenKind::EqEq
                | SyntaxTokenKind::Plus
                | SyntaxTokenKind::Minus
                | SyntaxTokenKind::Spread => TOKEN_OPERATOR,
                _ => return None,
            };
            Some(RawSemanticToken {
                span: token.span,
                token_type,
                token_modifiers_bitset: 0,
                priority: 1,
            })
        })
        .collect()
}

fn encode_semantic_tokens(
    mut raw_tokens: Vec<RawSemanticToken>,
    line_index: &LineIndex,
) -> Vec<SemanticToken> {
    raw_tokens.retain(|token| token.span.start < token.span.end);
    raw_tokens.sort_by(|a, b| {
        a.span
            .start
            .cmp(&b.span.start)
            .then_with(|| b.priority.cmp(&a.priority))
            .then_with(|| a.span.end.cmp(&b.span.end))
            .then_with(|| a.token_type.cmp(&b.token_type))
    });

    let mut encoded = Vec::new();
    let mut covered_until = 0usize;
    let mut previous_line = 0u32;
    let mut previous_start = 0u32;

    for raw in raw_tokens {
        if raw.span.start < covered_until {
            continue;
        }
        let range = line_index.span_to_range(raw.span);
        if range.start.line != range.end.line || range.start.character == range.end.character {
            continue;
        }
        let delta_line = range.start.line - previous_line;
        let delta_start = if delta_line == 0 {
            range.start.character - previous_start
        } else {
            range.start.character
        };
        encoded.push(SemanticToken {
            delta_line,
            delta_start,
            length: range.end.character - range.start.character,
            token_type: raw.token_type,
            token_modifiers_bitset: raw.token_modifiers_bitset,
        });
        previous_line = range.start.line;
        previous_start = range.start.character;
        covered_until = raw.span.end;
    }

    encoded
}

fn semantic_token_type_for_symbol_kind(kind: FxSymbolKind) -> u32 {
    match kind {
        FxSymbolKind::Value => TOKEN_FUNCTION,
        FxSymbolKind::Type => TOKEN_TYPE,
        FxSymbolKind::Constructor => TOKEN_ENUM_MEMBER,
        FxSymbolKind::Effect => TOKEN_INTERFACE,
        FxSymbolKind::Operation => TOKEN_FUNCTION,
        FxSymbolKind::Scope => TOKEN_NAMESPACE,
    }
}

fn semantic_token_type_for_typed_span_kind(kind: fx_core::check::TypedSpanKind) -> u32 {
    match kind {
        fx_core::check::TypedSpanKind::Value => TOKEN_FUNCTION,
        fx_core::check::TypedSpanKind::Type => TOKEN_TYPE,
        fx_core::check::TypedSpanKind::Effect => TOKEN_INTERFACE,
        fx_core::check::TypedSpanKind::Operation => TOKEN_FUNCTION,
        fx_core::check::TypedSpanKind::Scope => TOKEN_NAMESPACE,
        fx_core::check::TypedSpanKind::Constructor => TOKEN_ENUM_MEMBER,
        fx_core::check::TypedSpanKind::Parameter => TOKEN_PARAMETER,
        fx_core::check::TypedSpanKind::Local => TOKEN_VARIABLE,
        fx_core::check::TypedSpanKind::Pattern => TOKEN_VARIABLE,
        fx_core::check::TypedSpanKind::Field => TOKEN_PROPERTY,
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
        Document, LineIndex, MOD_DECLARATION, TOKEN_COMMENT, TOKEN_ENUM_MEMBER, TOKEN_FUNCTION,
        TOKEN_INTERFACE, TOKEN_KEYWORD, TOKEN_NAMESPACE, TOKEN_NUMBER, TOKEN_OPERATOR,
        TOKEN_PARAMETER, TOKEN_PROPERTY, TOKEN_STRING, TOKEN_TYPE, TOKEN_VARIABLE,
        completions_for_analysis, document_symbols_for_document, encode_semantic_tokens,
        lexical_semantic_tokens, lsp_diagnostics_for_document, semantic_tokens_for_document,
        workspace_symbols_for_analysis,
    };
    use fx_core::{AnalysisDiagnostic, SourceFile, analyze_workspace};
    use fx_syntax::span::Span;
    use std::path::PathBuf;
    use tower_lsp::lsp_types::CompletionItemKind;
    use tower_lsp::lsp_types::SemanticToken;
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
    fn semantic_token_encoding_uses_utf16_deltas() {
        let index = LineIndex::new("a😀b\n  c\n");
        let raw = vec![
            super::RawSemanticToken {
                span: Span::new("a😀".len(), "a😀b".len()),
                token_type: TOKEN_VARIABLE,
                token_modifiers_bitset: 0,
                priority: 1,
            },
            super::RawSemanticToken {
                span: Span::new("a😀b\n  ".len(), "a😀b\n  c".len()),
                token_type: TOKEN_VARIABLE,
                token_modifiers_bitset: 0,
                priority: 1,
            },
        ];

        assert_eq!(
            encode_semantic_tokens(raw, &index),
            vec![
                SemanticToken {
                    delta_line: 0,
                    delta_start: 3,
                    length: 1,
                    token_type: TOKEN_VARIABLE,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 1,
                    delta_start: 2,
                    length: 1,
                    token_type: TOKEN_VARIABLE,
                    token_modifiers_bitset: 0,
                },
            ]
        );
    }

    #[test]
    fn semantic_token_encoding_sorts_and_skips_overlapping_tokens() {
        let index = LineIndex::new("alpha beta");
        let raw = vec![
            super::RawSemanticToken {
                span: Span::new(6, 10),
                token_type: TOKEN_VARIABLE,
                token_modifiers_bitset: 0,
                priority: 1,
            },
            super::RawSemanticToken {
                span: Span::new(0, 5),
                token_type: TOKEN_VARIABLE,
                token_modifiers_bitset: 0,
                priority: 1,
            },
            super::RawSemanticToken {
                span: Span::new(0, 5),
                token_type: TOKEN_FUNCTION,
                token_modifiers_bitset: MOD_DECLARATION,
                priority: 4,
            },
        ];

        assert_eq!(
            encode_semantic_tokens(raw, &index),
            vec![
                SemanticToken {
                    delta_line: 0,
                    delta_start: 0,
                    length: 5,
                    token_type: TOKEN_FUNCTION,
                    token_modifiers_bitset: MOD_DECLARATION,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 6,
                    length: 4,
                    token_type: TOKEN_VARIABLE,
                    token_modifiers_bitset: 0,
                },
            ]
        );
    }

    #[test]
    fn lexical_semantic_tokens_classify_basic_source_tokens() {
        let source = "# note\nanswer : Int\n  if 1 == 1 then \"yes\" else \"no\"\n  x |> f(1) ++ y -> ...z\n  y = y + 1\n";
        let tokens = lexical_semantic_tokens(source);
        let types = tokens
            .iter()
            .map(|token| token.token_type)
            .collect::<Vec<_>>();

        assert!(types.contains(&TOKEN_COMMENT));
        assert!(types.contains(&TOKEN_KEYWORD));
        assert!(types.contains(&TOKEN_NUMBER));
        assert!(types.contains(&TOKEN_STRING));
        assert!(
            types
                .iter()
                .filter(|token_type| **token_type == TOKEN_OPERATOR)
                .count()
                >= 6
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

    #[test]
    fn phase_3_effects_have_symbols_and_operation_completions() {
        let source = include_str!("../../../examples/phase-3/effects.fx");
        let path = "/private/tmp/effects.fx";
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
        assert!(
            children
                .iter()
                .any(|symbol| { symbol.name == "Raise" && symbol.kind == SymbolKind::INTERFACE })
        );
        assert!(
            children
                .iter()
                .any(|symbol| { symbol.name == "raise" && symbol.kind == SymbolKind::FUNCTION })
        );

        let workspace_symbols = workspace_symbols_for_analysis(&analysis, "raise");
        assert!(
            workspace_symbols
                .iter()
                .any(|symbol| symbol.name == "raise")
        );

        let completions = completions_for_analysis(&analysis);
        assert!(completions.iter().any(|item| {
            item.label == "raise" && item.kind == Some(CompletionItemKind::FUNCTION)
        }));
    }

    #[test]
    fn semantic_tokens_classify_phase_1_declarations_and_references() {
        let source = include_str!("../../../examples/phase-1/todo.fx");
        let path = "/private/tmp/todo.fx";
        let analysis = analysis_with_files(vec![source_file(path, source)]);
        let document = document(path, source);
        let tokens = semantic_tokens_for_document(&analysis, &document).data;

        assert_has_token(&tokens, TOKEN_TYPE, MOD_DECLARATION);
        assert_has_token(&tokens, TOKEN_ENUM_MEMBER, MOD_DECLARATION);
        assert_has_token(&tokens, TOKEN_FUNCTION, MOD_DECLARATION);
        assert_has_token(&tokens, TOKEN_TYPE, 0);
        assert_has_token(&tokens, TOKEN_ENUM_MEMBER, 0);
    }

    #[test]
    fn semantic_tokens_classify_phase_3_effects_and_operations() {
        let source = include_str!("../../../examples/phase-3/effects.fx");
        let path = "/private/tmp/effects.fx";
        let analysis = analysis_with_files(vec![source_file(path, source)]);
        let document = document(path, source);
        let tokens = semantic_tokens_for_document(&analysis, &document).data;

        assert_has_token(&tokens, TOKEN_INTERFACE, MOD_DECLARATION);
        assert_has_token(&tokens, TOKEN_FUNCTION, MOD_DECLARATION);
        assert_has_token(&tokens, TOKEN_INTERFACE, 0);
        assert_has_token(&tokens, TOKEN_FUNCTION, 0);
    }

    #[test]
    fn semantic_tokens_classify_phase_4_scopes_and_scoped_operations() {
        let source = include_str!("../../../examples/phase-4/state.fx");
        let path = "/private/tmp/state.fx";
        let analysis = analysis_with_files(vec![source_file(path, source)]);
        let document = document(path, source);
        let tokens = semantic_tokens_for_document(&analysis, &document).data;

        assert_has_token(&tokens, TOKEN_NAMESPACE, MOD_DECLARATION);
        assert_has_token(&tokens, TOKEN_NAMESPACE, 0);
        assert_has_token(&tokens, TOKEN_FUNCTION, 0);
    }

    #[test]
    fn semantic_tokens_classify_phase_5_contextual_typed_spans() {
        let source = "export type Clock = { now: Text }\n\nstamp(message: Text) : Text given { clock: Clock }\n  now = clock.now\n  now ++ message\n";
        let path = "/private/tmp/basic.fx";
        let analysis = analysis_with_files(vec![source_file(path, source)]);
        let document = document(path, source);
        let tokens = semantic_tokens_for_document(&analysis, &document).data;

        assert_has_token(&tokens, TOKEN_PARAMETER, 0);
        assert_has_token(&tokens, TOKEN_VARIABLE, 0);
        assert_has_token(&tokens, TOKEN_PROPERTY, 0);
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

    fn assert_has_token(tokens: &[SemanticToken], token_type: u32, modifier: u32) {
        assert!(
            tokens.iter().any(|token| {
                token.token_type == token_type && token.token_modifiers_bitset == modifier
            }),
            "expected token type {token_type} with modifier {modifier}; tokens: {tokens:?}"
        );
    }
}
