# Shared Workspace Design for Token-Efficient Agent Collaboration

## Overview

This design adds lightweight agent collaboration to Ted while minimizing token usage.
Instead of each agent maintaining full conversation context with peer messages,
agents write structured artifacts to a shared workspace and read summaries on-demand.

## Key Principles

1. **Structured over prose** - Artifacts are typed JSON, not natural language
2. **Pull over push** - Agents request what they need, not broadcast everything
3. **Summaries by default** - Full content only loaded when explicitly needed
4. **Event-driven coordination** - Lightweight signals, not full message passing

## Token Comparison

| Approach | Per-Agent Context | 4-Agent Total |
|----------|-------------------|---------------|
| Claude Code (full peer messaging) | ~50k tokens | ~200k |
| Ted current (hierarchical) | ~20k main + ~10k sub | ~50k |
| **Shared workspace (this design)** | ~15k main + ~3k sub | ~24k |

---

## Core Types

### `src/agents/workspace.rs`

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// The shared workspace for multi-agent collaboration
pub struct SharedWorkspace {
    /// Unique workspace identifier
    pub id: Uuid,

    /// Artifacts written by agents
    artifacts: RwLock<HashMap<String, Artifact>>,

    /// Lightweight signals between agents
    signals: RwLock<Vec<Signal>>,

    /// Task claims (integrates with beads)
    claims: RwLock<HashMap<String, Uuid>>,  // bead_id -> agent_id

    /// Persistence path
    storage_path: PathBuf,
}

/// A structured artifact written by an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    /// Unique artifact key (e.g., "security-review", "perf-findings")
    pub key: String,

    /// Which agent wrote this
    pub agent_id: Uuid,
    pub agent_name: String,

    /// What type of artifact
    pub artifact_type: ArtifactType,

    /// The structured content (compact JSON)
    pub content: serde_json::Value,

    /// One-line summary (loaded by default)
    pub summary: String,

    /// Token estimate for the full content
    pub content_tokens: u32,

    /// When written
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ArtifactType {
    /// Code review findings
    Review {
        severity_counts: HashMap<String, u32>,  // high: 2, medium: 5, etc.
        files_reviewed: Vec<String>,
    },
    /// Implementation plan
    Plan {
        steps: Vec<String>,
        estimated_files: Vec<String>,
    },
    /// Research/exploration results
    Research {
        files_found: Vec<String>,
        patterns_identified: Vec<String>,
    },
    /// Implementation result
    Implementation {
        files_changed: Vec<String>,
        tests_added: Vec<String>,
    },
    /// Generic structured data
    Custom {
        schema: String,  // hint about structure
    },
}

/// Lightweight signal between agents (not full messages)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    pub id: Uuid,
    pub from_agent: Uuid,
    pub signal_type: SignalType,
    pub timestamp: DateTime<Utc>,
    /// Already read by these agents
    pub read_by: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignalType {
    /// Agent completed its task
    Completed { artifact_key: String },

    /// Agent found something relevant to another's work
    Hint {
        target_agent: Option<Uuid>,  // None = broadcast
        hint: String,  // Max 100 chars
    },

    /// Agent challenges another's finding
    Challenge {
        artifact_key: String,
        challenge: String,  // Max 200 chars
    },

    /// Agent claims a task
    Claimed { bead_id: String },

    /// Agent is blocked
    Blocked { reason: String },
}
```

---

## Workspace Operations

```rust
impl SharedWorkspace {
    /// Create a new workspace
    pub fn new(storage_path: PathBuf) -> Self { ... }

    /// Load existing workspace from disk
    pub async fn load(storage_path: PathBuf) -> Result<Self> { ... }

    // === Artifact Operations ===

    /// Write an artifact (creates or updates)
    pub async fn write_artifact(&self, artifact: Artifact) -> Result<()> {
        let mut artifacts = self.artifacts.write().await;
        artifacts.insert(artifact.key.clone(), artifact);
        self.persist().await
    }

    /// Get artifact summary only (~20 tokens)
    pub async fn get_summary(&self, key: &str) -> Option<ArtifactSummary> {
        let artifacts = self.artifacts.read().await;
        artifacts.get(key).map(|a| ArtifactSummary {
            key: a.key.clone(),
            agent_name: a.agent_name.clone(),
            artifact_type: a.artifact_type.type_name(),
            summary: a.summary.clone(),
            content_tokens: a.content_tokens,
            updated_at: a.updated_at,
        })
    }

    /// Get full artifact content (only when needed)
    pub async fn get_full(&self, key: &str) -> Option<Artifact> {
        let artifacts = self.artifacts.read().await;
        artifacts.get(key).cloned()
    }

    /// List all artifact summaries
    pub async fn list_summaries(&self) -> Vec<ArtifactSummary> {
        let artifacts = self.artifacts.read().await;
        artifacts.values().map(|a| ArtifactSummary::from(a)).collect()
    }

    // === Signal Operations ===

    /// Send a signal
    pub async fn signal(&self, from: Uuid, signal_type: SignalType) -> Result<()> {
        let mut signals = self.signals.write().await;
        signals.push(Signal {
            id: Uuid::new_v4(),
            from_agent: from,
            signal_type,
            timestamp: Utc::now(),
            read_by: vec![],
        });
        self.persist().await
    }

    /// Get unread signals for an agent
    pub async fn unread_signals(&self, agent_id: Uuid) -> Vec<Signal> {
        let signals = self.signals.read().await;
        signals.iter()
            .filter(|s| !s.read_by.contains(&agent_id))
            .cloned()
            .collect()
    }

    /// Mark signals as read
    pub async fn mark_read(&self, agent_id: Uuid, signal_ids: &[Uuid]) -> Result<()> {
        let mut signals = self.signals.write().await;
        for signal in signals.iter_mut() {
            if signal_ids.contains(&signal.id) {
                signal.read_by.push(agent_id);
            }
        }
        self.persist().await
    }

    // === Task Claiming ===

    /// Attempt to claim a task (returns false if already claimed)
    pub async fn claim_task(&self, bead_id: &str, agent_id: Uuid) -> Result<bool> {
        let mut claims = self.claims.write().await;
        if claims.contains_key(bead_id) {
            return Ok(false);
        }
        claims.insert(bead_id.to_string(), agent_id);
        self.signal(agent_id, SignalType::Claimed {
            bead_id: bead_id.to_string()
        }).await?;
        Ok(true)
    }

    /// Get unclaimed tasks from beads
    pub async fn unclaimed_tasks(&self, bead_store: &BeadStore) -> Vec<Bead> {
        let claims = self.claims.read().await;
        bead_store.get_actionable()
            .into_iter()
            .filter(|b| !claims.contains_key(&b.id.to_string()))
            .collect()
    }
}

/// Lightweight summary for listing (minimal tokens)
#[derive(Debug, Clone, Serialize)]
pub struct ArtifactSummary {
    pub key: String,
    pub agent_name: String,
    pub artifact_type: String,
    pub summary: String,
    pub content_tokens: u32,
    pub updated_at: DateTime<Utc>,
}
```

---

## Integration with AgentContext

```rust
// In src/agents/context.rs

impl AgentContext {
    /// Optional shared workspace for collaboration
    workspace: Option<Arc<SharedWorkspace>>,
}

impl AgentContextBuilder {
    /// Add shared workspace access
    pub fn with_workspace(mut self, workspace: Arc<SharedWorkspace>) -> Self {
        self.workspace = Some(workspace);
        self
    }
}
```

---

## New Tools for Agents

### `WorkspaceReadTool`

```rust
/// Tool for reading from the shared workspace
pub struct WorkspaceReadTool;

impl Tool for WorkspaceReadTool {
    fn name(&self) -> &str { "workspace_read" }

    fn description(&self) -> &str {
        "Read artifacts from the shared workspace. Use 'list' to see all artifacts, \
         'summary <key>' to get a brief summary, or 'full <key>' to get complete content."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "summary", "full", "signals"],
                    "description": "What to read"
                },
                "key": {
                    "type": "string",
                    "description": "Artifact key (for summary/full)"
                }
            },
            "required": ["action"]
        })
    }
}
```

### `WorkspaceWriteTool`

```rust
/// Tool for writing to the shared workspace
pub struct WorkspaceWriteTool;

impl Tool for WorkspaceWriteTool {
    fn name(&self) -> &str { "workspace_write" }

    fn description(&self) -> &str {
        "Write an artifact to the shared workspace for other agents to read."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "key": {
                    "type": "string",
                    "description": "Unique key for this artifact (e.g., 'security-findings')"
                },
                "artifact_type": {
                    "type": "string",
                    "enum": ["review", "plan", "research", "implementation", "custom"]
                },
                "summary": {
                    "type": "string",
                    "description": "One-line summary (max 100 chars)"
                },
                "content": {
                    "type": "object",
                    "description": "Structured content (will be serialized as JSON)"
                }
            },
            "required": ["key", "artifact_type", "summary", "content"]
        })
    }
}
```

### `WorkspaceSignalTool`

```rust
/// Tool for lightweight signaling between agents
pub struct WorkspaceSignalTool;

impl Tool for WorkspaceSignalTool {
    fn name(&self) -> &str { "workspace_signal" }

    fn description(&self) -> &str {
        "Send a lightweight signal to coordinate with other agents. Use for hints, \
         challenges, or status updates. Keep messages under 200 characters."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "signal_type": {
                    "type": "string",
                    "enum": ["hint", "challenge", "blocked"]
                },
                "message": {
                    "type": "string",
                    "maxLength": 200
                },
                "target_artifact": {
                    "type": "string",
                    "description": "For challenges: which artifact to challenge"
                }
            },
            "required": ["signal_type", "message"]
        })
    }
}
```

---

## Usage Example: Parallel Code Review

```rust
// Coordinator spawns 3 review agents with shared workspace
let workspace = Arc::new(SharedWorkspace::new(project_path.join(".ted/workspace")));

let security_agent = AgentConfig::new("review",
    "Review for security vulnerabilities. Focus on auth, injection, XSS.",
    project_path.clone()
).with_background(true);

let perf_agent = AgentConfig::new("review",
    "Review for performance issues. Focus on N+1 queries, memory leaks, blocking calls.",
    project_path.clone()
).with_background(true);

let test_agent = AgentConfig::new("review",
    "Review test coverage. Identify untested edge cases and missing assertions.",
    project_path.clone()
).with_background(true);

// Build contexts with shared workspace
let security_ctx = AgentContextBuilder::new(security_agent)
    .with_workspace(workspace.clone())
    .build().await?;

// ... spawn all agents ...

// Coordinator periodically checks workspace
loop {
    let summaries = workspace.list_summaries().await;

    // All done when we have 3 artifacts
    if summaries.len() == 3 {
        break;
    }

    // Check for challenges that need resolution
    let signals = workspace.unread_signals(coordinator_id).await;
    for signal in signals {
        if let SignalType::Challenge { artifact_key, challenge } = &signal.signal_type {
            // Could spawn another agent to investigate, or flag for human review
        }
    }

    tokio::time::sleep(Duration::from_secs(5)).await;
}

// Synthesize findings from summaries (not full content!)
let mut report = String::new();
for summary in workspace.list_summaries().await {
    report.push_str(&format!("## {}\n{}\n\n", summary.key, summary.summary));
}
```

---

## Token Budget Breakdown

### Per-Agent Context
- System prompt: ~500 tokens
- Task description: ~200 tokens
- Tool definitions: ~300 tokens
- Working memory: ~2000 tokens
- **Total: ~3000 tokens per subagent**

### Workspace Reads (on-demand)
- `list` (all summaries): ~20 tokens × N artifacts
- `summary <key>`: ~30 tokens
- `full <key>`: varies (agent chooses when to pay this cost)
- `signals`: ~15 tokens per unread signal

### Comparison
- **Claude Code style**: Each agent gets full peer conversation = ~15k tokens per peer × 3 peers = ~45k extra tokens
- **This design**: Each agent gets summaries on-demand = ~100-500 tokens for coordination

---

## Debate/Challenge Pattern

Instead of full conversations between agents, use structured challenges:

```rust
// Security agent finds an issue
workspace.write_artifact(Artifact {
    key: "security-findings".to_string(),
    content: json!({
        "issues": [{
            "severity": "high",
            "file": "src/auth.rs",
            "line": 142,
            "type": "sql_injection",
            "description": "User input passed directly to query"
        }]
    }),
    summary: "Found 1 high-severity SQL injection in auth.rs:142".to_string(),
    ..
}).await;

// Perf agent reads summary, disagrees
workspace.signal(perf_agent_id, SignalType::Challenge {
    artifact_key: "security-findings".to_string(),
    challenge: "auth.rs:142 uses parameterized query, not string concat. Check line 142 again.".to_string(),
}).await;

// Security agent sees challenge in next iteration, can re-examine
let signals = workspace.unread_signals(security_agent_id).await;
// ... re-check and update artifact if needed
```

This gives you the benefit of agents challenging each other without duplicating full conversation context.

---

## File Layout

```
src/agents/
├── mod.rs
├── types.rs
├── context.rs
├── builtin.rs
├── memory.rs
├── runner.rs
└── workspace.rs       # NEW: SharedWorkspace, Artifact, Signal

src/tools/builtin/
├── mod.rs
├── spawn_agent.rs
├── workspace_read.rs  # NEW
├── workspace_write.rs # NEW
└── workspace_signal.rs # NEW
```

---

## Migration Path

1. Add `SharedWorkspace` as optional (agents work fine without it)
2. Add workspace tools to implement/review agent types
3. Coordinator can opt-in to workspace-based coordination
4. Existing hierarchical spawning still works unchanged

This is additive - no breaking changes to current subagent system.
