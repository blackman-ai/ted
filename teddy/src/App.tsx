// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

import { useState, useEffect } from 'react';
import { FileTree } from './components/FileTree';
import { Editor } from './components/Editor';
import { ChatPanel } from './components/ChatPanel';
import { Console } from './components/Console';
import { Preview } from './components/Preview';
import { ProjectPicker } from './components/ProjectPicker';
import { useTed } from './hooks/useTed';
import { useProject } from './hooks/useProject';
import './App.css';

function App() {
  const { project, setProject } = useProject();
  const { sendPrompt, stop, events, isRunning, logs } = useTed();
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [showPreview, setShowPreview] = useState(false);

  if (!project) {
    return <ProjectPicker onProjectSelected={setProject} />;
  }

  return (
    <div className="app">
      <div className="titlebar">
        <div className="titlebar-left">
          <span className="app-title">Teddy</span>
          <span className="project-name">{project.name}</span>
        </div>
        <div className="titlebar-right">
          <button
            className="btn-secondary"
            onClick={() => setProject(null)}
          >
            Change Project
          </button>
        </div>
      </div>

      <div className="main-content">
        <div className="sidebar">
          <FileTree
            projectPath={project.path}
            selectedFile={selectedFile}
            onFileSelect={setSelectedFile}
          />
        </div>

        <div className="editor-area">
          <div className="editor-tabs">
            <button
              className={`tab ${!showPreview ? 'active' : ''}`}
              onClick={() => setShowPreview(false)}
            >
              Editor
            </button>
            <button
              className={`tab ${showPreview ? 'active' : ''}`}
              onClick={() => setShowPreview(true)}
            >
              Preview
            </button>
            <div className="tab-actions">
              <button className="btn-icon" title="Docker (Coming Soon)" disabled>
                🐳
              </button>
              <button className="btn-icon" title="PostgreSQL (Coming Soon)" disabled>
                🐘
              </button>
              <button className="btn-icon" title="Deploy (Coming Soon)" disabled>
                🚀
              </button>
            </div>
          </div>

          {!showPreview ? (
            <Editor
              projectPath={project.path}
              selectedFile={selectedFile}
              onFileChange={() => {
                // Refresh file tree or handle change
              }}
            />
          ) : (
            <Preview projectPath={project.path} />
          )}
        </div>

        <div className="right-panel">
          <div className="panel-tabs">
            <button className="tab active">Chat</button>
          </div>
          <ChatPanel
            onSendMessage={sendPrompt}
            onStop={stop}
            events={events}
            isRunning={isRunning}
          />
        </div>
      </div>

      <div className="bottom-panel">
        <Console logs={logs} />
      </div>
    </div>
  );
}

export default App;
