# Ted Chat TUI Design

## Overview

A full ratatui-based terminal interface for the main chat experience, replacing the current println!-based output with a structured, interactive UI that provides clear visibility into agent activity.

## Visual Layout

```
┌─ ted ─────────────────────────────────── claude-sonnet-4 ─ session:a5749264 ─┐
│                                                                               │
│  you: Can you spawn agents to fix the unwrap issues and improve error        │
│       handling across the codebase?                                          │
│                                                                               │
│  ted: I'll spawn multiple specialized agents to tackle these improvements    │
│       in parallel. Let me create agents for each area...                     │
│                                                                               │
│       ╭─ spawn_agent                                                         │
│       │  type: implement                                                     │
│       │  task: "Audit and fix .unwrap() usage"                               │
│       ╰─ ✓ Spawned implement-d6ed                                            │
│                                                                               │
│       ╭─ spawn_agent                                                         │
│       │  type: implement                                                     │
│       │  task: "Standardize error handling with thiserror"                   │
│       ╰─ ✓ Spawned implement-a6ec                                            │
│                                                                               │
├─ Agents ──────────────────────────────────────────── 3 running │ 1 done ─────┤
│                                                                               │
│  ● implement-d6ed   [████████░░░░] 8/30   Editing src/tools/mod.rs          │
│  ● implement-a6ec   [████░░░░░░░░] 4/30   Reading src/error.rs              │
│  ● implement-4154   [██░░░░░░░░░░] 2/30   Searching for patterns...         │
│  ✓ implement-c05f   done in 23s          Changed 12 files                    │
│                                                                               │
├──────────────────────────────────────────────────────────────────────────────┤
│ > _                                                                          │
│                                                                               │
│ ↑↓ history │ Tab expand │ Ctrl+A toggle agents │ Ctrl+C cancel │ /help      │
└──────────────────────────────────────────────────────────────────────────────┘
```

## Component Architecture

```
src/tui/
├── mod.rs              # Module exports
├── chat/
│   ├── mod.rs          # Chat TUI module
│   ├── app.rs          # ChatApp state machine
│   ├── ui.rs           # Main rendering logic
│   ├── input.rs        # Input handling & key bindings
│   ├── widgets/
│   │   ├── mod.rs
│   │   ├── message.rs      # Message bubble rendering
│   │   ├── tool_call.rs    # Tool invocation display
│   │   ├── agent_pane.rs   # Agent status panel
│   │   ├── input_area.rs   # Multi-line input with cursor
│   │   └── status_bar.rs   # Bottom status/help bar
│   └── state/
│       ├── mod.rs
│       ├── messages.rs     # Conversation state
│       ├── agents.rs       # AgentTracker
│       └── input.rs        # Input buffer state
└── settings/           # Existing settings TUI (keep as-is)
    ├── app.rs
    ├── ui.rs
    └── ...
```

## Core Data Structures

### AgentTracker

```rust
/// Tracks all running and completed agents
pub struct AgentTracker {
    agents: HashMap<Uuid, TrackedAgent>,
    /// Order agents were spawned (for display)
    spawn_order: Vec<Uuid>,
}

#[derive(Debug, Clone)]
pub struct TrackedAgent {
    pub id: Uuid,
    pub name: String,           // e.g., "implement-d6ed"
    pub agent_type: String,     // e.g., "implement", "explore"
    pub task: String,           // Task description
    pub status: AgentStatus,
    pub progress: AgentProgress,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub current_action: Option<String>,  // "Reading src/main.rs"
    pub files_changed: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentStatus {
    Pending,        // Queued, waiting for rate budget
    Running,        // Actively executing
    RateLimited,    // Waiting for rate budget
    Completed,      // Finished successfully
    Failed,         // Finished with error
    Cancelled,      // User cancelled
}

#[derive(Debug, Clone)]
pub struct AgentProgress {
    pub iteration: u32,
    pub max_iterations: u32,
    pub tokens_used: u64,
    pub token_budget: u64,
}
```

### ChatApp State

```rust
pub struct ChatApp {
    // UI State
    pub mode: ChatMode,
    pub scroll_offset: usize,
    pub agent_pane_visible: bool,
    pub agent_pane_height: u16,  // Resizable

    // Content
    pub messages: Vec<DisplayMessage>,
    pub agents: AgentTracker,

    // Input
    pub input: InputState,
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,

    // Session
    pub session_id: Uuid,
    pub provider: String,
    pub model: String,
    pub caps: Vec<String>,

    // Async communication
    pub event_rx: mpsc::UnboundedReceiver<ChatEvent>,
    pub event_tx: mpsc::UnboundedSender<ChatEvent>,
}

#[derive(Debug, Clone)]
pub enum ChatMode {
    Normal,         // Viewing chat, can scroll
    Input,          // Typing in input area
    AgentFocus,     // Navigating agent list
    Help,           // Showing help overlay
}

#[derive(Debug, Clone)]
pub struct DisplayMessage {
    pub id: Uuid,
    pub role: Role,
    pub content: MessageContent,
    pub timestamp: DateTime<Utc>,
    pub tool_calls: Vec<DisplayToolCall>,
    pub is_streaming: bool,
}

#[derive(Debug, Clone)]
pub struct DisplayToolCall {
    pub name: String,
    pub input_summary: String,
    pub status: ToolCallStatus,
    pub result_preview: Option<String>,
    pub expanded: bool,  // User can expand/collapse
}
```

### Event System

```rust
/// Events for async communication between LLM/agent threads and UI
pub enum ChatEvent {
    // User input
    UserMessage(String),

    // LLM responses
    StreamStart,
    StreamDelta(String),
    StreamEnd,

    // Tool calls
    ToolCallStart { name: String, input: Value },
    ToolCallEnd { name: String, result: ToolResult },

    // Agent lifecycle
    AgentSpawned { id: Uuid, name: String, agent_type: String, task: String },
    AgentProgress { id: Uuid, iteration: u32, action: String },
    AgentRateLimited { id: Uuid, wait_secs: f64 },
    AgentCompleted { id: Uuid, files_changed: Vec<String> },
    AgentFailed { id: Uuid, error: String },

    // System
    Error(String),
    SessionEnded,
}
```

## Key Bindings

### Normal Mode (viewing chat)
| Key | Action |
|-----|--------|
| `Enter` | Focus input area |
| `↑/↓` or `j/k` | Scroll chat history |
| `PgUp/PgDn` | Scroll faster |
| `g/G` | Go to top/bottom |
| `Tab` | Toggle agent pane |
| `Ctrl+A` | Focus agent pane |
| `q` | Quit (with confirmation if agents running) |
| `/` | Command mode |
| `?` | Show help overlay |

### Input Mode
| Key | Action |
|-----|--------|
| `Enter` | Send message (or newline with Shift) |
| `Ctrl+Enter` | Force send (multiline) |
| `↑/↓` | Input history navigation |
| `Esc` | Cancel input / return to normal |
| `Tab` | Autocomplete command |
| `Ctrl+C` | Cancel current operation |
| `Ctrl+L` | Clear screen |

### Agent Focus Mode
| Key | Action |
|-----|--------|
| `↑/↓` or `j/k` | Navigate agents |
| `Enter` | Expand/view agent details |
| `c` | Cancel selected agent |
| `Esc` | Return to normal mode |

## Rendering Details

### Message Bubbles

```
  you: Can you help me fix this bug?        │ Right-aligned, cyan
       The function returns None when       │ for user messages
       it should return Some(value).        │

  ted: I'll analyze the issue...            │ Left-aligned, white
                                            │ for assistant
       Looking at the code, the problem     │
       is in the `process()` function:      │
                                            │
       ```rust                              │ Code blocks with
       fn process() -> Option<T> {          │ syntax highlighting
           // Bug: early return             │
           return None;                     │
       }                                    │
       ```                                  │
```

### Tool Call Display (Collapsed)

```
       ╭─ file_read → src/main.rs
       ╰─ ✓ 142 lines

       ╭─ file_edit → src/lib.rs
       │  old: "return None"
       │  new: "return Some(result)"
       ╰─ ✓ Applied

       ╭─ shell → cargo test
       ╰─ ⏳ Running... (5s)
```

### Tool Call Display (Expanded)

```
       ╭─ file_read ────────────────────────────────────────╮
       │ path: src/main.rs                                  │
       ├────────────────────────────────────────────────────┤
       │  1 │ fn main() {                                   │
       │  2 │     let result = process();                   │
       │  3 │     println!("{:?}", result);                 │
       │  4 │ }                                             │
       │    │ ... (138 more lines)                          │
       ╰────────────────────────────────────────────────────╯
```

### Agent Pane States

**Minimal (default when few agents):**
```
├─ Agents ─────────────────────────────────────────────────────────────────────┤
│ ● implement-d6ed [████░░] 8/30 Editing... │ ✓ explore-c05f done (12 files)  │
```

**Expanded (many agents or user expanded):**
```
├─ Agents ──────────────────────────────────────────── 3 running │ 1 done ─────┤
│                                                                               │
│  ● implement-d6ed   [████████░░░░] 8/30   Editing src/tools/mod.rs           │
│    └─ Rate: 2.3k tok/min │ Changed: 3 files                                  │
│                                                                               │
│  ● implement-a6ec   [████░░░░░░░░] 4/30   Reading src/error.rs               │
│    └─ Rate: 1.8k tok/min │ Waiting for budget (2.1s)                         │
│                                                                               │
│  ✓ implement-c05f   completed in 23s                                         │
│    └─ Changed: src/main.rs, src/lib.rs, src/error.rs (+9 more)               │
│                                                                               │
```

### Progress Bar Rendering

```rust
fn render_progress_bar(current: u32, max: u32, width: u16) -> String {
    let filled = ((current as f32 / max as f32) * width as f32) as u16;
    let empty = width - filled;
    format!("[{}{}]", "█".repeat(filled as usize), "░".repeat(empty as usize))
}
```

## Integration Points

### 1. Spawn Agent Hook

Modify `SpawnAgentTool::execute()` to emit events:

```rust
// In spawn_agent.rs
impl SpawnAgentTool {
    async fn execute(&self, ...) -> Result<ToolResult> {
        // Emit spawn event
        if let Some(tx) = context.chat_event_tx() {
            tx.send(ChatEvent::AgentSpawned {
                id: agent_id,
                name: agent_name.clone(),
                agent_type: agent_type.clone(),
                task: task.clone(),
            });
        }

        // ... spawn logic ...
    }
}
```

### 2. Agent Runner Progress

Modify `AgentRunner::run()` to emit progress:

```rust
// In runner.rs
loop {
    context.increment_iteration();

    // Emit progress
    if let Some(tx) = context.chat_event_tx() {
        tx.send(ChatEvent::AgentProgress {
            id: context.config.id,
            iteration: context.iterations(),
            action: current_action.clone(),
        });
    }

    // ... rest of loop ...
}
```

### 3. Main Loop Integration

```rust
// In main.rs or new chat_tui.rs
async fn run_chat_tui(/* args */) -> Result<()> {
    let mut terminal = setup_terminal()?;
    let (event_tx, event_rx) = mpsc::unbounded_channel();

    let mut app = ChatApp::new(event_tx.clone(), event_rx);

    // Spawn input handler task
    let input_tx = event_tx.clone();
    tokio::spawn(async move {
        handle_input_events(input_tx).await
    });

    loop {
        // Render UI
        terminal.draw(|f| ui::draw(f, &app))?;

        // Handle events with timeout for smooth updates
        tokio::select! {
            Some(event) = app.event_rx.recv() => {
                match event {
                    ChatEvent::UserMessage(msg) => {
                        app.handle_user_message(msg).await?;
                    }
                    ChatEvent::AgentSpawned { .. } => {
                        app.agents.track(/* ... */);
                    }
                    // ... handle other events
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                // Tick for animations/updates
            }
        }

        if app.should_quit {
            break;
        }
    }

    restore_terminal()?;
    Ok(())
}
```

## File Changes Required

### New Files
- `src/tui/chat/mod.rs`
- `src/tui/chat/app.rs`
- `src/tui/chat/ui.rs`
- `src/tui/chat/input.rs`
- `src/tui/chat/widgets/*.rs`
- `src/tui/chat/state/*.rs`

### Modified Files
- `src/main.rs` - Add `--no-tui` flag, call chat TUI
- `src/tools/builtin/spawn_agent.rs` - Emit agent events
- `src/agents/runner.rs` - Emit progress events
- `src/tools/mod.rs` - Add event sender to ToolContext
- `Cargo.toml` - May need additional ratatui features

## Implementation Phases

### Phase 1: Core Structure (MVP)
- [ ] Basic ChatApp state machine
- [ ] Simple message rendering (no streaming yet)
- [ ] Basic input handling
- [ ] Static agent pane (shows spawned agents)

### Phase 2: Agent Integration
- [ ] Event system for agent updates
- [ ] Real-time agent progress display
- [ ] Agent pane expand/collapse
- [ ] Cancel agent functionality

### Phase 3: Polish
- [ ] Streaming response rendering
- [ ] Syntax highlighting in code blocks
- [ ] Tool call expand/collapse
- [ ] Input history with search
- [ ] Resize handling
- [ ] Themes/colors configuration

### Phase 4: Advanced Features
- [ ] Split pane view (agent details)
- [ ] Log viewer for agent output
- [ ] Export conversation
- [ ] Mouse support
- [ ] Vim mode for navigation

## Fallback Mode

For scripting/piping, add a `--no-tui` or `--simple` flag:

```bash
# Interactive TUI (default)
ted

# Simple mode for scripts
ted --simple
echo "explain this code" | ted --simple

# Pipe output
ted --simple "list files" | grep ".rs"
```

The simple mode keeps the current println!-based output for compatibility.
