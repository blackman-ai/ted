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
