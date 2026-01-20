// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use tower_lsp::lsp_types::*;

/// Define server capabilities
pub fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        // Text document sync - full document sync for simplicity
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::FULL),
                will_save: Some(false),
                will_save_wait_until: Some(false),
                save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                    include_text: Some(false),
                })),
            },
        )),

        // Completion
        completion_provider: Some(CompletionOptions {
            resolve_provider: Some(false),
            trigger_characters: Some(vec![
                ".".to_string(),
                ":".to_string(),
                "<".to_string(),
                "/".to_string(),
                "\"".to_string(),
                "'".to_string(),
            ]),
            all_commit_characters: None,
            work_done_progress_options: WorkDoneProgressOptions {
                work_done_progress: Some(false),
            },
            completion_item: None,
        }),

        // Go to definition
        definition_provider: Some(OneOf::Left(true)),

        // Hover
        hover_provider: Some(HoverProviderCapability::Simple(true)),

        // We don't support these yet, but could add them later:
        // - references_provider
        // - document_symbol_provider
        // - code_action_provider
        // - document_formatting_provider
        // - rename_provider
        // - diagnostic_provider

        ..Default::default()
    }
}
