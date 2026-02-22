// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { useState, useEffect } from 'react';
import './Memory.css';

export interface ConversationMemory {
  id: string;
  timestamp: string;
  summary: string;
  files_changed: string[];
  tags: string[];
  content: string;
}

interface MemoryProps {
  projectPath: string;
}

export function Memory({ projectPath }: MemoryProps) {
  const [memories, setMemories] = useState<ConversationMemory[]>([]);
  const [searchQuery, setSearchQuery] = useState('');
  const [searchResults, setSearchResults] = useState<ConversationMemory[]>([]);
  const [selectedMemory, setSelectedMemory] = useState<ConversationMemory | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Load recent memories on mount
  useEffect(() => {
    loadRecentMemories();
  }, [projectPath]);

  const loadRecentMemories = async () => {
    setIsLoading(true);
    setError(null);
    try {
      const recentMemories = await window.teddy.memoryGetRecent(50);
      setMemories(recentMemories);
      setSearchResults(recentMemories);
      if (recentMemories.length > 0) {
        setSelectedMemory(recentMemories[0]);
      } else {
        setSelectedMemory(null);
      }
    } catch (err) {
      setError('Failed to load memories');
      console.error('Error loading memories:', err);
    } finally {
      setIsLoading(false);
    }
  };

  const handleSearch = async () => {
    if (!searchQuery.trim()) {
      setSearchResults(memories);
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      const results = await window.teddy.memorySearch(searchQuery.trim(), 50);
      setSearchResults(results);
      if (results.length > 0) {
        setSelectedMemory(results[0]);
      } else {
        setSelectedMemory(null);
      }
    } catch (err) {
      setError('Search failed');
      console.error('Error searching memories:', err);
    } finally {
      setIsLoading(false);
    }
  };

  const formatTimestamp = (timestamp: string): string => {
    const date = new Date(timestamp);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

    if (diffDays === 0) {
      return 'Today';
    } else if (diffDays === 1) {
      return 'Yesterday';
    } else if (diffDays < 7) {
      return `${diffDays} days ago`;
    } else {
      return date.toLocaleDateString();
    }
  };

  const handleKeyPress = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      handleSearch();
    }
  };

  return (
    <div className="memory-viewer">
      <div className="memory-header">
        <h2 className="memory-title">Conversation Memory</h2>
        <div className="memory-search">
          <input
            type="text"
            className="memory-search-input"
            placeholder="Search past conversations..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            onKeyPress={handleKeyPress}
          />
          <button
            className="btn-search"
            onClick={handleSearch}
            disabled={isLoading}
          >
            🔍 Search
          </button>
        </div>
      </div>

      <div className="memory-content">
        <div className="memory-list">
          {isLoading && (
            <div className="memory-loading">
              <div className="loading-spinner"></div>
              <p>Loading memories...</p>
            </div>
          )}

          {error && (
            <div className="memory-error">
              <p>⚠️ {error}</p>
              <button className="btn-retry" onClick={loadRecentMemories}>
                Retry
              </button>
            </div>
          )}

          {!isLoading && !error && searchResults.length === 0 && (
            <div className="memory-empty">
              <div className="empty-icon">🧠</div>
              <p>No conversations found</p>
              <p className="empty-hint">
                {searchQuery
                  ? 'Try a different search term'
                  : 'Start chatting with Ted to build conversation memory'}
              </p>
            </div>
          )}

          {!isLoading && !error && searchResults.length > 0 && (
            <>
              {searchResults.map((memory) => (
                <div
                  key={memory.id}
                  className={`memory-item ${selectedMemory?.id === memory.id ? 'selected' : ''}`}
                  onClick={() => setSelectedMemory(memory)}
                >
                  <div className="memory-item-header">
                    <span className="memory-item-date">
                      {formatTimestamp(memory.timestamp)}
                    </span>
                    {memory.tags.length > 0 && (
                      <div className="memory-item-tags">
                        {memory.tags.slice(0, 3).map((tag, idx) => (
                          <span key={idx} className="memory-tag">
                            {tag}
                          </span>
                        ))}
                      </div>
                    )}
                  </div>
                  <div className="memory-item-summary">{memory.summary}</div>
                  {memory.files_changed.length > 0 && (
                    <div className="memory-item-files">
                      📁 {memory.files_changed.length} file{memory.files_changed.length > 1 ? 's' : ''} changed
                    </div>
                  )}
                </div>
              ))}
            </>
          )}
        </div>

        <div className="memory-detail">
          {selectedMemory ? (
            <>
              <div className="memory-detail-header">
                <div className="memory-detail-title">
                  <span className="memory-detail-date">
                    {formatTimestamp(selectedMemory.timestamp)}
                  </span>
                  <h3>{selectedMemory.summary}</h3>
                </div>
                {selectedMemory.tags.length > 0 && (
                  <div className="memory-detail-tags">
                    {selectedMemory.tags.map((tag, idx) => (
                      <span key={idx} className="memory-tag memory-tag-large">
                        {tag}
                      </span>
                    ))}
                  </div>
                )}
              </div>

              <div className="memory-detail-content">
                {selectedMemory.files_changed.length > 0 && (
                  <div className="memory-detail-section">
                    <h4>Files Changed</h4>
                    <ul className="memory-files-list">
                      {selectedMemory.files_changed.map((file, idx) => (
                        <li key={idx} className="memory-file-item">
                          {file}
                        </li>
                      ))}
                    </ul>
                  </div>
                )}

                <div className="memory-detail-section">
                  <h4>Conversation</h4>
                  <pre className="memory-conversation">
                    {selectedMemory.content}
                  </pre>
                </div>
              </div>
            </>
          ) : (
            <div className="memory-detail-empty">
              <p>Select a conversation from the list to view details</p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
