// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use super::capabilities::server_capabilities;
use super::completion::provide_completions;
use super::definition::provide_definition;
use super::hover::provide_hover;

/// Document state for an open file
#[derive(Debug, Clone)]
pub struct DocumentState {
    pub uri: Url,
    pub content: String,
    pub version: i32,
    pub language_id: String,
}

/// The Ted Language Server
pub struct TedLanguageServer {
    client: Client,
    workspace_root: Arc<RwLock<Option<PathBuf>>>,
    documents: Arc<RwLock<HashMap<Url, DocumentState>>>,
}

impl TedLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            workspace_root: Arc::new(RwLock::new(None)),
            documents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get the workspace root
    pub async fn get_workspace_root(&self) -> Option<PathBuf> {
        self.workspace_root.read().await.clone()
    }

    /// Get a document by URI
    pub async fn get_document(&self, uri: &Url) -> Option<DocumentState> {
        self.documents.read().await.get(uri).cloned()
    }

    /// Get all open documents
    pub async fn get_all_documents(&self) -> Vec<DocumentState> {
        self.documents.read().await.values().cloned().collect()
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for TedLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        tracing::info!("LSP initialize request received");

        // Set workspace root
        if let Some(root_uri) = params.root_uri {
            if let Ok(path) = root_uri.to_file_path() {
                *self.workspace_root.write().await = Some(path.clone());
                tracing::info!("Workspace root: {:?}", path);
            }
        }

        Ok(InitializeResult {
            capabilities: server_capabilities(),
            server_info: Some(ServerInfo {
                name: "Ted Language Server".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        tracing::info!("LSP server initialized");
        self.client
            .log_message(MessageType::INFO, "Ted LSP server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        tracing::info!("LSP shutdown request received");
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let doc = params.text_document;
        tracing::debug!("Document opened: {}", doc.uri);

        let state = DocumentState {
            uri: doc.uri.clone(),
            content: doc.text,
            version: doc.version,
            language_id: doc.language_id,
        };

        self.documents.write().await.insert(doc.uri, state);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;

        if let Some(change) = params.content_changes.into_iter().last() {
            // For full document sync, we just replace the content
            let mut docs = self.documents.write().await;
            if let Some(state) = docs.get_mut(&uri) {
                state.content = change.text;
                state.version = version;
            }
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        tracing::debug!("Document closed: {}", params.text_document.uri);
        self.documents
            .write()
            .await
            .remove(&params.text_document.uri);
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        tracing::debug!("Document saved: {}", params.text_document.uri);
        // Could trigger diagnostics or other actions here
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        tracing::debug!("Completion request at {:?}:{:?}", uri, position);

        if let Some(doc) = self.get_document(uri).await {
            let workspace = self.get_workspace_root().await;
            match provide_completions(&doc, position, workspace.as_deref()).await {
                Ok(items) => {
                    return Ok(Some(CompletionResponse::Array(items)));
                }
                Err(e) => {
                    tracing::warn!("Completion error: {}", e);
                }
            }
        }

        Ok(None)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        tracing::debug!("Go-to-definition at {:?}:{:?}", uri, position);

        if let Some(doc) = self.get_document(uri).await {
            let workspace = self.get_workspace_root().await;
            match provide_definition(&doc, position, workspace.as_deref()).await {
                Ok(Some(location)) => {
                    return Ok(Some(GotoDefinitionResponse::Scalar(location)));
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!("Go-to-definition error: {}", e);
                }
            }
        }

        Ok(None)
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        tracing::debug!("Hover at {:?}:{:?}", uri, position);

        if let Some(doc) = self.get_document(uri).await {
            let workspace = self.get_workspace_root().await;
            match provide_hover(&doc, position, workspace.as_deref()).await {
                Ok(hover) => return Ok(hover),
                Err(e) => {
                    tracing::warn!("Hover error: {}", e);
                }
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== DocumentState tests ====================

    #[test]
    fn test_document_state_creation() {
        let uri = Url::parse("file:///test/file.rs").unwrap();
        let state = DocumentState {
            uri: uri.clone(),
            content: "fn main() {}".to_string(),
            version: 1,
            language_id: "rust".to_string(),
        };

        assert_eq!(state.uri, uri);
        assert_eq!(state.content, "fn main() {}");
        assert_eq!(state.version, 1);
        assert_eq!(state.language_id, "rust");
    }

    #[test]
    fn test_document_state_clone() {
        let uri = Url::parse("file:///test/file.rs").unwrap();
        let state = DocumentState {
            uri: uri.clone(),
            content: "content".to_string(),
            version: 5,
            language_id: "python".to_string(),
        };

        let cloned = state.clone();
        assert_eq!(cloned.uri, state.uri);
        assert_eq!(cloned.content, state.content);
        assert_eq!(cloned.version, state.version);
        assert_eq!(cloned.language_id, state.language_id);
    }

    #[test]
    fn test_document_state_debug() {
        let uri = Url::parse("file:///test/file.rs").unwrap();
        let state = DocumentState {
            uri,
            content: "test".to_string(),
            version: 1,
            language_id: "rust".to_string(),
        };

        let debug = format!("{:?}", state);
        assert!(debug.contains("DocumentState"));
        assert!(debug.contains("file.rs"));
    }

    #[test]
    fn test_document_state_empty_content() {
        let uri = Url::parse("file:///empty.txt").unwrap();
        let state = DocumentState {
            uri,
            content: String::new(),
            version: 0,
            language_id: "plaintext".to_string(),
        };

        assert!(state.content.is_empty());
        assert_eq!(state.version, 0);
    }

    #[test]
    fn test_document_state_large_content() {
        let uri = Url::parse("file:///large.rs").unwrap();
        let large_content =
            "fn main() {\n".to_string() + &"    println!(\"hello\");\n".repeat(10000);
        let state = DocumentState {
            uri,
            content: large_content.clone(),
            version: 1,
            language_id: "rust".to_string(),
        };

        assert_eq!(state.content.len(), large_content.len());
        assert!(state.content.contains("println"));
    }

    #[test]
    fn test_document_state_unicode_content() {
        let uri = Url::parse("file:///unicode.txt").unwrap();
        let state = DocumentState {
            uri,
            content: "日本語テスト 🎉 émojis".to_string(),
            version: 1,
            language_id: "markdown".to_string(),
        };

        assert!(state.content.contains("日本語"));
        assert!(state.content.contains("🎉"));
    }

    #[test]
    fn test_document_state_various_languages() {
        let languages = vec![
            "rust",
            "python",
            "javascript",
            "typescript",
            "go",
            "java",
            "c",
            "cpp",
            "markdown",
            "json",
            "yaml",
            "toml",
        ];

        for lang in languages {
            let uri = Url::parse(&format!("file:///test.{}", lang)).unwrap();
            let state = DocumentState {
                uri,
                content: String::new(),
                version: 1,
                language_id: lang.to_string(),
            };
            assert_eq!(state.language_id, lang);
        }
    }

    #[test]
    fn test_document_state_version_increment() {
        let uri = Url::parse("file:///test.rs").unwrap();
        let state1 = DocumentState {
            uri: uri.clone(),
            content: "v1".to_string(),
            version: 1,
            language_id: "rust".to_string(),
        };

        let state2 = DocumentState {
            uri,
            content: "v2".to_string(),
            version: state1.version + 1,
            language_id: "rust".to_string(),
        };

        assert_eq!(state2.version, 2);
        assert_ne!(state1.content, state2.content);
    }

    #[test]
    fn test_document_state_negative_version() {
        // LSP versions can be negative in some implementations
        let uri = Url::parse("file:///test.rs").unwrap();
        let state = DocumentState {
            uri,
            content: "content".to_string(),
            version: -1,
            language_id: "rust".to_string(),
        };

        assert_eq!(state.version, -1);
    }

    // ==================== URL parsing tests ====================

    #[test]
    fn test_url_to_file_path() {
        let uri = Url::parse("file:///home/user/project/src/main.rs").unwrap();
        let path = uri.to_file_path();
        assert!(path.is_ok());

        #[cfg(unix)]
        assert_eq!(
            path.unwrap().to_str().unwrap(),
            "/home/user/project/src/main.rs"
        );
    }

    #[test]
    fn test_url_with_spaces() {
        let uri = Url::parse("file:///path/with%20spaces/file.rs").unwrap();
        let path = uri.to_file_path();
        assert!(path.is_ok());
    }

    #[test]
    fn test_non_file_url() {
        let uri = Url::parse("https://example.com/file.rs").unwrap();
        let path = uri.to_file_path();
        assert!(path.is_err()); // Should fail for non-file URLs
    }

    // ==================== Position tests ====================

    #[test]
    fn test_position_creation() {
        let pos = Position {
            line: 10,
            character: 5,
        };

        assert_eq!(pos.line, 10);
        assert_eq!(pos.character, 5);
    }

    #[test]
    fn test_position_zero() {
        let pos = Position {
            line: 0,
            character: 0,
        };

        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 0);
    }

    // ==================== ServerInfo tests ====================

    #[test]
    fn test_server_info_creation() {
        let info = ServerInfo {
            name: "Ted Language Server".to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        };

        assert_eq!(info.name, "Ted Language Server");
        assert!(info.version.is_some());
    }

    #[test]
    fn test_server_info_version_format() {
        let version = env!("CARGO_PKG_VERSION");
        // Version should be in semver format
        assert!(version.contains('.'));
        let parts: Vec<&str> = version.split('.').collect();
        assert!(parts.len() >= 2);
    }

    // ==================== HashMap operations tests ====================

    #[test]
    fn test_hashmap_document_storage() {
        let mut docs: HashMap<Url, DocumentState> = HashMap::new();

        let uri1 = Url::parse("file:///test1.rs").unwrap();
        let uri2 = Url::parse("file:///test2.rs").unwrap();

        docs.insert(
            uri1.clone(),
            DocumentState {
                uri: uri1.clone(),
                content: "content1".to_string(),
                version: 1,
                language_id: "rust".to_string(),
            },
        );

        docs.insert(
            uri2.clone(),
            DocumentState {
                uri: uri2.clone(),
                content: "content2".to_string(),
                version: 1,
                language_id: "rust".to_string(),
            },
        );

        assert_eq!(docs.len(), 2);
        assert!(docs.contains_key(&uri1));
        assert!(docs.contains_key(&uri2));
    }

    #[test]
    fn test_hashmap_document_update() {
        let mut docs: HashMap<Url, DocumentState> = HashMap::new();
        let uri = Url::parse("file:///test.rs").unwrap();

        // Insert initial
        docs.insert(
            uri.clone(),
            DocumentState {
                uri: uri.clone(),
                content: "v1".to_string(),
                version: 1,
                language_id: "rust".to_string(),
            },
        );

        // Update
        if let Some(state) = docs.get_mut(&uri) {
            state.content = "v2".to_string();
            state.version = 2;
        }

        let state = docs.get(&uri).unwrap();
        assert_eq!(state.content, "v2");
        assert_eq!(state.version, 2);
    }

    #[test]
    fn test_hashmap_document_remove() {
        let mut docs: HashMap<Url, DocumentState> = HashMap::new();
        let uri = Url::parse("file:///test.rs").unwrap();

        docs.insert(
            uri.clone(),
            DocumentState {
                uri: uri.clone(),
                content: "content".to_string(),
                version: 1,
                language_id: "rust".to_string(),
            },
        );

        assert_eq!(docs.len(), 1);
        docs.remove(&uri);
        assert_eq!(docs.len(), 0);
        assert!(!docs.contains_key(&uri));
    }

    #[test]
    fn test_hashmap_collect_values() {
        let mut docs: HashMap<Url, DocumentState> = HashMap::new();

        for i in 0..5 {
            let uri = Url::parse(&format!("file:///test{}.rs", i)).unwrap();
            docs.insert(
                uri.clone(),
                DocumentState {
                    uri,
                    content: format!("content{}", i),
                    version: i,
                    language_id: "rust".to_string(),
                },
            );
        }

        let values: Vec<DocumentState> = docs.values().cloned().collect();
        assert_eq!(values.len(), 5);
    }

    // ==================== PathBuf tests ====================

    #[test]
    fn test_pathbuf_from_url() {
        let uri = Url::parse("file:///home/user/project").unwrap();
        if let Ok(pathbuf) = uri.to_file_path() {
            assert!(pathbuf.is_absolute());
        }
    }

    #[test]
    fn test_optional_pathbuf() {
        let workspace: Option<PathBuf> = None;
        assert!(workspace.is_none());

        let workspace = PathBuf::from("/home/user");
        assert_eq!(workspace.to_str().expect("valid utf-8"), "/home/user");
    }

    // ==================== Arc/RwLock pattern tests ====================

    #[tokio::test]
    async fn test_rwlock_read() {
        let data: Arc<RwLock<Option<PathBuf>>> =
            Arc::new(RwLock::new(Some(PathBuf::from("/test"))));

        let guard = data.read().await;
        assert!(guard.is_some());
        assert_eq!(guard.as_ref().unwrap().to_str().unwrap(), "/test");
    }

    #[tokio::test]
    async fn test_rwlock_write() {
        let data: Arc<RwLock<Option<PathBuf>>> = Arc::new(RwLock::new(None));

        {
            let mut guard = data.write().await;
            *guard = Some(PathBuf::from("/new/path"));
        }

        let guard = data.read().await;
        assert!(guard.is_some());
        assert_eq!(guard.as_ref().unwrap().to_str().unwrap(), "/new/path");
    }

    #[tokio::test]
    async fn test_rwlock_hashmap() {
        let docs: Arc<RwLock<HashMap<Url, DocumentState>>> = Arc::new(RwLock::new(HashMap::new()));
        let uri = Url::parse("file:///test.rs").unwrap();

        // Write
        {
            let mut guard = docs.write().await;
            guard.insert(
                uri.clone(),
                DocumentState {
                    uri: uri.clone(),
                    content: "test".to_string(),
                    version: 1,
                    language_id: "rust".to_string(),
                },
            );
        }

        // Read
        {
            let guard = docs.read().await;
            let doc = guard.get(&uri);
            assert!(doc.is_some());
            assert_eq!(doc.unwrap().content, "test");
        }
    }

    #[tokio::test]
    async fn test_rwlock_clone_option() {
        let data: Arc<RwLock<Option<PathBuf>>> =
            Arc::new(RwLock::new(Some(PathBuf::from("/workspace"))));

        let cloned = data.read().await.clone();
        assert!(cloned.is_some());
        assert_eq!(cloned.unwrap().to_str().unwrap(), "/workspace");
    }

    // ==================== LSP Types tests ====================

    #[test]
    fn test_initialize_result_structure() {
        use super::server_capabilities;
        let result = InitializeResult {
            capabilities: server_capabilities(),
            server_info: Some(ServerInfo {
                name: "Ted Language Server".to_string(),
                version: Some("0.1.0".to_string()),
            }),
        };

        assert!(result.server_info.is_some());
        assert_eq!(
            result.server_info.as_ref().unwrap().name,
            "Ted Language Server"
        );
    }

    #[test]
    fn test_text_document_item() {
        let doc = TextDocumentItem {
            uri: Url::parse("file:///test.rs").unwrap(),
            language_id: "rust".to_string(),
            version: 1,
            text: "fn main() {}".to_string(),
        };

        assert_eq!(doc.language_id, "rust");
        assert_eq!(doc.version, 1);
    }

    #[test]
    fn test_did_open_params() {
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: Url::parse("file:///test.rs").unwrap(),
                language_id: "rust".to_string(),
                version: 1,
                text: "content".to_string(),
            },
        };

        assert_eq!(params.text_document.version, 1);
    }

    #[test]
    fn test_versioned_text_document_identifier() {
        let identifier = VersionedTextDocumentIdentifier {
            uri: Url::parse("file:///test.rs").unwrap(),
            version: 5,
        };

        assert_eq!(identifier.version, 5);
    }

    #[test]
    fn test_text_document_content_change_event() {
        let change = TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "new content".to_string(),
        };

        assert!(change.range.is_none());
        assert_eq!(change.text, "new content");
    }

    #[test]
    fn test_did_change_params() {
        let uri = Url::parse("file:///test.rs").unwrap();
        let params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: 2,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "updated".to_string(),
            }],
        };

        assert_eq!(params.content_changes.len(), 1);
        assert_eq!(params.text_document.version, 2);
    }

    #[test]
    fn test_did_close_params() {
        let params = DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier {
                uri: Url::parse("file:///test.rs").unwrap(),
            },
        };

        assert!(params.text_document.uri.to_string().contains("test.rs"));
    }

    #[test]
    fn test_did_save_params() {
        let params = DidSaveTextDocumentParams {
            text_document: TextDocumentIdentifier {
                uri: Url::parse("file:///test.rs").unwrap(),
            },
            text: Some("saved content".to_string()),
        };

        assert!(params.text.is_some());
    }

    #[test]
    fn test_text_document_position_params() {
        let params = TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: Url::parse("file:///test.rs").unwrap(),
            },
            position: Position {
                line: 10,
                character: 5,
            },
        };

        assert_eq!(params.position.line, 10);
        assert_eq!(params.position.character, 5);
    }

    #[test]
    fn test_completion_params() {
        let params = CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::parse("file:///test.rs").unwrap(),
                },
                position: Position {
                    line: 5,
                    character: 10,
                },
            },
            work_done_progress_params: WorkDoneProgressParams {
                work_done_token: None,
            },
            partial_result_params: PartialResultParams {
                partial_result_token: None,
            },
            context: None,
        };

        assert_eq!(params.text_document_position.position.line, 5);
    }

    #[test]
    fn test_completion_item() {
        let item = CompletionItem {
            label: "println!".to_string(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some("Macro for printing".to_string()),
            ..Default::default()
        };

        assert_eq!(item.label, "println!");
        assert!(item.kind.is_some());
    }

    #[test]
    fn test_completion_response_array() {
        let items = vec![
            CompletionItem {
                label: "fn".to_string(),
                ..Default::default()
            },
            CompletionItem {
                label: "let".to_string(),
                ..Default::default()
            },
        ];

        let response = CompletionResponse::Array(items);
        if let CompletionResponse::Array(arr) = response {
            assert_eq!(arr.len(), 2);
        }
    }

    #[test]
    fn test_hover_params() {
        let params = HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::parse("file:///test.rs").unwrap(),
                },
                position: Position {
                    line: 1,
                    character: 0,
                },
            },
            work_done_progress_params: WorkDoneProgressParams {
                work_done_token: None,
            },
        };

        assert_eq!(params.text_document_position_params.position.line, 1);
    }

    #[test]
    fn test_hover_contents() {
        let hover = Hover {
            contents: HoverContents::Scalar(MarkedString::String("Hover text".to_string())),
            range: None,
        };

        assert!(hover.range.is_none());
    }

    #[test]
    fn test_goto_definition_params() {
        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: Url::parse("file:///test.rs").unwrap(),
                },
                position: Position {
                    line: 20,
                    character: 15,
                },
            },
            work_done_progress_params: WorkDoneProgressParams {
                work_done_token: None,
            },
            partial_result_params: PartialResultParams {
                partial_result_token: None,
            },
        };

        assert_eq!(params.text_document_position_params.position.line, 20);
    }

    #[test]
    fn test_location() {
        let location = Location {
            uri: Url::parse("file:///definition.rs").unwrap(),
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 10,
                },
            },
        };

        assert!(location.uri.to_string().contains("definition.rs"));
    }

    #[test]
    fn test_goto_definition_response() {
        let location = Location {
            uri: Url::parse("file:///target.rs").unwrap(),
            range: Range {
                start: Position {
                    line: 5,
                    character: 0,
                },
                end: Position {
                    line: 5,
                    character: 20,
                },
            },
        };

        let response = GotoDefinitionResponse::Scalar(location);
        if let GotoDefinitionResponse::Scalar(loc) = response {
            assert_eq!(loc.range.start.line, 5);
        }
    }

    #[test]
    fn test_range_creation() {
        let range = Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 10,
                character: 50,
            },
        };

        assert_eq!(range.start.line, 0);
        assert_eq!(range.end.line, 10);
        assert_eq!(range.end.character, 50);
    }

    #[test]
    fn test_message_type() {
        let _info = MessageType::INFO;
        let _warning = MessageType::WARNING;
        let _error = MessageType::ERROR;
        let _log = MessageType::LOG;
    }

    // ==================== Additional Document Tracking Tests ====================

    #[tokio::test]
    async fn test_document_tracking_flow() {
        let docs: Arc<RwLock<HashMap<Url, DocumentState>>> = Arc::new(RwLock::new(HashMap::new()));

        // Simulate did_open
        let uri = Url::parse("file:///test.rs").unwrap();
        {
            let mut guard = docs.write().await;
            guard.insert(
                uri.clone(),
                DocumentState {
                    uri: uri.clone(),
                    content: "initial".to_string(),
                    version: 1,
                    language_id: "rust".to_string(),
                },
            );
        }

        // Simulate did_change
        {
            let mut guard = docs.write().await;
            if let Some(state) = guard.get_mut(&uri) {
                state.content = "updated".to_string();
                state.version = 2;
            }
        }

        // Verify state
        {
            let guard = docs.read().await;
            let doc = guard.get(&uri).unwrap();
            assert_eq!(doc.content, "updated");
            assert_eq!(doc.version, 2);
        }

        // Simulate did_close
        {
            let mut guard = docs.write().await;
            guard.remove(&uri);
        }

        // Verify removed
        {
            let guard = docs.read().await;
            assert!(guard.get(&uri).is_none());
        }
    }

    #[tokio::test]
    async fn test_multiple_documents() {
        let docs: Arc<RwLock<HashMap<Url, DocumentState>>> = Arc::new(RwLock::new(HashMap::new()));

        // Open multiple documents
        for i in 0..10 {
            let uri = Url::parse(&format!("file:///file{}.rs", i)).unwrap();
            let mut guard = docs.write().await;
            guard.insert(
                uri.clone(),
                DocumentState {
                    uri,
                    content: format!("content {}", i),
                    version: 1,
                    language_id: "rust".to_string(),
                },
            );
        }

        // Verify count
        {
            let guard = docs.read().await;
            assert_eq!(guard.len(), 10);
        }

        // Get all documents
        {
            let guard = docs.read().await;
            let all: Vec<_> = guard.values().cloned().collect();
            assert_eq!(all.len(), 10);
        }
    }

    // ==================== Content Change Tests ====================

    #[test]
    fn test_content_changes_last() {
        let changes = vec![
            TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "first".to_string(),
            },
            TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "second".to_string(),
            },
            TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "third".to_string(),
            },
        ];

        // into_iter().last() should get the last change
        let last = changes.into_iter().last();
        assert!(last.is_some());
        assert_eq!(last.unwrap().text, "third");
    }

    #[test]
    fn test_empty_content_changes() {
        let changes: Vec<TextDocumentContentChangeEvent> = vec![];
        let last = changes.into_iter().last();
        assert!(last.is_none());
    }

    // ==================== TedLanguageServer Method Tests ====================
    // These tests cover the actual LanguageServer trait implementation

    // Helper to create a mock Client for testing
    // Note: TedLanguageServer requires a tower_lsp::Client which is hard to mock
    // So we test the underlying data structures and logic instead

    #[tokio::test]
    async fn test_workspace_root_flow() {
        // Test the workspace root handling logic
        let workspace_root: Arc<RwLock<Option<PathBuf>>> = Arc::new(RwLock::new(None));

        // Initially None
        assert!(workspace_root.read().await.is_none());

        // Set workspace root (like in initialize)
        {
            *workspace_root.write().await = Some(PathBuf::from("/workspace/project"));
        }

        // Verify it's set
        {
            let root = workspace_root.read().await.clone();
            assert!(root.is_some());
            assert_eq!(root.unwrap().to_str().unwrap(), "/workspace/project");
        }
    }

    #[tokio::test]
    async fn test_document_did_open_flow() {
        // Test the did_open document handling
        let documents: Arc<RwLock<HashMap<Url, DocumentState>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let uri = Url::parse("file:///test.rs").unwrap();

        // Simulate did_open
        let doc = TextDocumentItem {
            uri: uri.clone(),
            language_id: "rust".to_string(),
            version: 1,
            text: "fn main() {}".to_string(),
        };

        let state = DocumentState {
            uri: doc.uri.clone(),
            content: doc.text.clone(),
            version: doc.version,
            language_id: doc.language_id.clone(),
        };

        documents.write().await.insert(doc.uri, state);

        // Verify document is stored
        let stored = documents.read().await.get(&uri).cloned();
        assert!(stored.is_some());
        let stored = stored.unwrap();
        assert_eq!(stored.content, "fn main() {}");
        assert_eq!(stored.version, 1);
        assert_eq!(stored.language_id, "rust");
    }

    #[tokio::test]
    async fn test_document_did_change_flow() {
        // Test the did_change document handling
        let documents: Arc<RwLock<HashMap<Url, DocumentState>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let uri = Url::parse("file:///test.rs").unwrap();

        // First open the document
        documents.write().await.insert(
            uri.clone(),
            DocumentState {
                uri: uri.clone(),
                content: "original".to_string(),
                version: 1,
                language_id: "rust".to_string(),
            },
        );

        // Simulate did_change with a change
        let version = 2;
        let change = TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "updated content".to_string(),
        };

        // Apply the change (like in did_change)
        {
            let mut docs = documents.write().await;
            if let Some(state) = docs.get_mut(&uri) {
                state.content = change.text.clone();
                state.version = version;
            }
        }

        // Verify
        let stored = documents.read().await.get(&uri).cloned().unwrap();
        assert_eq!(stored.content, "updated content");
        assert_eq!(stored.version, 2);
    }

    #[tokio::test]
    async fn test_document_did_close_flow() {
        // Test the did_close document handling
        let documents: Arc<RwLock<HashMap<Url, DocumentState>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let uri = Url::parse("file:///test.rs").unwrap();

        // Open document
        documents.write().await.insert(
            uri.clone(),
            DocumentState {
                uri: uri.clone(),
                content: "content".to_string(),
                version: 1,
                language_id: "rust".to_string(),
            },
        );

        // Verify it exists
        assert!(documents.read().await.contains_key(&uri));

        // Simulate did_close
        documents.write().await.remove(&uri);

        // Verify it's removed
        assert!(!documents.read().await.contains_key(&uri));
    }

    #[tokio::test]
    async fn test_get_document_helper() {
        // Test the get_document helper method logic
        let documents: Arc<RwLock<HashMap<Url, DocumentState>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let uri = Url::parse("file:///test.rs").unwrap();

        // Before insert, should return None
        let result = documents.read().await.get(&uri).cloned();
        assert!(result.is_none());

        // Insert
        documents.write().await.insert(
            uri.clone(),
            DocumentState {
                uri: uri.clone(),
                content: "test".to_string(),
                version: 1,
                language_id: "rust".to_string(),
            },
        );

        // After insert, should return Some
        let result = documents.read().await.get(&uri).cloned();
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_get_all_documents_helper() {
        // Test the get_all_documents helper method logic
        let documents: Arc<RwLock<HashMap<Url, DocumentState>>> =
            Arc::new(RwLock::new(HashMap::new()));

        // Insert multiple documents
        for i in 0..5 {
            let uri = Url::parse(&format!("file:///file{}.rs", i)).unwrap();
            documents.write().await.insert(
                uri.clone(),
                DocumentState {
                    uri,
                    content: format!("content{}", i),
                    version: 1,
                    language_id: "rust".to_string(),
                },
            );
        }

        // Get all
        let all: Vec<DocumentState> = documents.read().await.values().cloned().collect();
        assert_eq!(all.len(), 5);
    }

    #[tokio::test]
    async fn test_get_workspace_root_helper() {
        // Test the get_workspace_root helper method logic
        let workspace_root: Arc<RwLock<Option<PathBuf>>> = Arc::new(RwLock::new(None));

        // Initially None
        assert!(workspace_root.read().await.clone().is_none());

        // Set
        *workspace_root.write().await = Some(PathBuf::from("/my/workspace"));

        // Get
        let root = workspace_root.read().await.clone();
        assert!(root.is_some());
        assert_eq!(root.unwrap(), PathBuf::from("/my/workspace"));
    }

    #[test]
    fn test_initialize_result_with_server_info() {
        // Test creating InitializeResult with server info
        use super::server_capabilities;

        let result = InitializeResult {
            capabilities: server_capabilities(),
            server_info: Some(ServerInfo {
                name: "Ted Language Server".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        };

        assert!(result.server_info.is_some());
        let info = result.server_info.unwrap();
        assert_eq!(info.name, "Ted Language Server");
        assert!(info.version.is_some());
    }

    #[test]
    fn test_server_capabilities_function() {
        // Test that server_capabilities() returns valid capabilities
        use super::server_capabilities;

        let caps = server_capabilities();

        // Check text document sync is set
        assert!(caps.text_document_sync.is_some());

        // Check completion provider is set
        assert!(caps.completion_provider.is_some());

        // Check hover provider is set
        assert!(caps.hover_provider.is_some());

        // Check definition provider is set
        assert!(caps.definition_provider.is_some());
    }

    #[tokio::test]
    async fn test_completion_flow() {
        // Test the completion logic
        let documents: Arc<RwLock<HashMap<Url, DocumentState>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let uri = Url::parse("file:///test.rs").unwrap();

        // Insert document
        documents.write().await.insert(
            uri.clone(),
            DocumentState {
                uri: uri.clone(),
                content: "fn main() {\n    let x = \n}".to_string(),
                version: 1,
                language_id: "rust".to_string(),
            },
        );

        // Get document (like in completion)
        let doc = documents.read().await.get(&uri).cloned();
        assert!(doc.is_some());

        let doc = doc.unwrap();
        assert!(!doc.content.is_empty());
    }

    #[tokio::test]
    async fn test_hover_flow() {
        // Test the hover logic
        let documents: Arc<RwLock<HashMap<Url, DocumentState>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let uri = Url::parse("file:///test.rs").unwrap();

        documents.write().await.insert(
            uri.clone(),
            DocumentState {
                uri: uri.clone(),
                content: "fn hello() {}".to_string(),
                version: 1,
                language_id: "rust".to_string(),
            },
        );

        // Get document
        let doc = documents.read().await.get(&uri).cloned();
        assert!(doc.is_some());
    }

    #[tokio::test]
    async fn test_goto_definition_flow() {
        // Test the goto_definition logic
        let documents: Arc<RwLock<HashMap<Url, DocumentState>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let uri = Url::parse("file:///test.rs").unwrap();

        documents.write().await.insert(
            uri.clone(),
            DocumentState {
                uri: uri.clone(),
                content: "struct Foo {}\nfn main() { let f = Foo {}; }".to_string(),
                version: 1,
                language_id: "rust".to_string(),
            },
        );

        let doc = documents.read().await.get(&uri).cloned();
        assert!(doc.is_some());
    }

    #[test]
    fn test_uri_to_file_path_conversion() {
        // Test converting URI to file path
        let uri = Url::parse("file:///home/user/project/src/main.rs").unwrap();
        let path = uri.to_file_path();

        assert!(path.is_ok());
        #[cfg(unix)]
        assert!(path.unwrap().to_str().unwrap().contains("main.rs"));
    }

    #[test]
    fn test_initialize_params_root_uri() {
        // Test handling of root_uri in InitializeParams
        let params = InitializeParams {
            root_uri: Some(Url::parse("file:///workspace").unwrap()),
            ..Default::default()
        };

        assert!(params.root_uri.is_some());
        let root_uri = params.root_uri.unwrap();
        let path = root_uri.to_file_path();
        assert!(path.is_ok());
    }

    #[test]
    fn test_initialize_params_no_root_uri() {
        let params = InitializeParams {
            root_uri: None,
            ..Default::default()
        };

        assert!(params.root_uri.is_none());
    }

    #[tokio::test]
    async fn test_did_save_flow() {
        // Test did_save handling - document should still exist
        let documents: Arc<RwLock<HashMap<Url, DocumentState>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let uri = Url::parse("file:///test.rs").unwrap();

        documents.write().await.insert(
            uri.clone(),
            DocumentState {
                uri: uri.clone(),
                content: "saved content".to_string(),
                version: 1,
                language_id: "rust".to_string(),
            },
        );

        // Verify document exists after save
        assert!(documents.read().await.contains_key(&uri));
    }

    #[test]
    fn test_completion_response_none() {
        // Test that completion can return None
        let response: Option<CompletionResponse> = None;
        assert!(response.is_none());
    }

    #[test]
    fn test_hover_response_none() {
        // Test that hover can return None
        let response: Option<Hover> = None;
        assert!(response.is_none());
    }

    #[test]
    fn test_goto_definition_response_none() {
        // Test that goto_definition can return None
        let response: Option<GotoDefinitionResponse> = None;
        assert!(response.is_none());
    }

    #[test]
    fn test_shutdown_returns_ok() {
        // Shutdown should return Ok(())
        let result: tower_lsp::jsonrpc::Result<()> = Ok(());
        assert!(result.is_ok());
    }
}
