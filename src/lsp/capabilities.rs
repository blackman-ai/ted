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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_capabilities_includes_completion() {
        let caps = server_capabilities();
        assert!(caps.completion_provider.is_some());
    }

    #[test]
    fn test_server_capabilities_includes_hover() {
        let caps = server_capabilities();
        assert!(caps.hover_provider.is_some());
    }

    #[test]
    fn test_server_capabilities_includes_definition() {
        let caps = server_capabilities();
        assert!(caps.definition_provider.is_some());
    }

    #[test]
    fn test_completion_trigger_characters() {
        let caps = server_capabilities();
        let completion = caps.completion_provider.unwrap();
        let triggers = completion.trigger_characters.unwrap();
        assert!(triggers.contains(&".".to_string()));
        assert!(triggers.contains(&":".to_string()));
        assert!(triggers.contains(&"/".to_string()));
    }

    #[test]
    fn test_text_document_sync_enabled() {
        let caps = server_capabilities();
        assert!(caps.text_document_sync.is_some());
    }

    #[test]
    fn test_text_document_sync_full_mode() {
        let caps = server_capabilities();
        if let Some(TextDocumentSyncCapability::Options(opts)) = caps.text_document_sync {
            assert_eq!(opts.change, Some(TextDocumentSyncKind::FULL));
            assert_eq!(opts.open_close, Some(true));
        } else {
            panic!("Expected TextDocumentSyncOptions");
        }
    }

    #[test]
    fn test_completion_resolve_provider_disabled() {
        let caps = server_capabilities();
        let completion = caps.completion_provider.unwrap();
        assert_eq!(completion.resolve_provider, Some(false));
    }

    #[test]
    fn test_hover_provider_simple() {
        let caps = server_capabilities();
        match caps.hover_provider {
            Some(HoverProviderCapability::Simple(enabled)) => assert!(enabled),
            _ => panic!("Expected simple hover provider"),
        }
    }

    #[test]
    fn test_definition_provider_enabled() {
        let caps = server_capabilities();
        match caps.definition_provider {
            Some(OneOf::Left(enabled)) => assert!(enabled),
            _ => panic!("Expected definition provider enabled"),
        }
    }
}
