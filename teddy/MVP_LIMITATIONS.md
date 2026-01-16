# MVP Limitations & Workarounds

This document tracks known limitations in the Teddy MVP and provides workarounds.

---

## ⚠️ CRITICAL: Embedded Mode Not Yet Implemented

### Issue

The `--embedded` flag is accepted by Ted but **not yet functional**. This means:
- Ted will run in normal TUI (terminal UI) mode
- JSONL events are NOT yet emitted
- Teddy cannot currently communicate with Ted via the protocol

### Why This Happened

The Teddy UI was scaffolded first to validate the architecture. The Ted embedded mode implementation requires:
1. Disabling the TUI when `--embedded` is set
2. Converting all agent actions to JSONL events
3. Streaming events to stdout
4. This is approximately 500-1000 lines of Rust code

### Timeline

**Estimated effort**: 2-3 days
**Priority**: HIGH (blocks core functionality)

### Workaround for Testing Teddy UI

For now, you can test the Teddy UI without Ted integration:

1. **Mock Mode** (add to Teddy):
   ```typescript
   // In main.ts, simulate Ted events
   if (process.env.TEDDY_MOCK_MODE) {
     simulateTedEvents();
   }
   ```

2. **Manual Testing**:
   - Test file tree navigation
   - Test Monaco editor
   - Test project picker
   - Test UI layout and styling

---

## Implementation Plan for Embedded Mode

### Phase 1: Basic JSONL Output (Day 1)

**File**: `src/main.rs`

```rust
async fn run_chat(args: ChatArgs, mut settings: Settings) -> Result<()> {
    if args.embedded {
        return run_embedded_chat(args, settings).await;
    }

    // ... existing TUI code ...
}

async fn run_embedded_chat(args: ChatArgs, settings: Settings) -> Result<()> {
    // Disable TUI, enable JSONL output
    let jsonl_mode = true;

    // Create provider, conversation, etc. (same as TUI mode)

    // But instead of TUI loop, use simple request/response:
    if let Some(prompt) = args.prompt {
        process_prompt_embedded(&prompt, &provider, &conversation).await?;
    }

    Ok(())
}
```

### Phase 2: Event Emission (Day 1-2)

**New file**: `src/embedded/mod.rs`

```rust
pub struct JsonLEmitter {
    session_id: String,
}

impl JsonLEmitter {
    pub fn emit_plan(&self, steps: Vec<Step>) {
        let event = json!({
            "type": "plan",
            "timestamp": current_timestamp(),
            "session_id": self.session_id,
            "data": {
                "steps": steps
            }
        });
        println!("{}", serde_json::to_string(&event).unwrap());
    }

    pub fn emit_file_create(&self, path: &str, content: &str) {
        // ... similar structure
    }

    // ... other event types
}
```

### Phase 3: Integration with Agent Loop (Day 2)

**File**: `src/tui/app.rs` or new `src/embedded/agent.rs`

Extract agent logic from TUI into shared module:

```rust
pub async fn run_agent_loop(
    provider: &dyn LlmProvider,
    conversation: &mut Conversation,
    output: &mut dyn AgentOutput,  // Trait for TUI or JSONL
) -> Result<()> {
    // Existing agent logic, but output-agnostic
}

trait AgentOutput {
    fn emit_plan(&mut self, steps: Vec<Step>);
    fn emit_file_create(&mut self, path: &str, content: &str);
    // ...
}
```

### Phase 4: Testing (Day 3)

```bash
# Test JSONL output
./target/release/ted chat --embedded "Create hello.txt" 2>/dev/null | jq

# Expected output:
# {"type":"status","timestamp":...,"session_id":"...","data":{"state":"thinking",...}}
# {"type":"plan",...}
# {"type":"file_create","data":{"path":"hello.txt","content":"..."}}
# {"type":"completion",...}
```

---

## Alternative Approach: Simpler MVP

Instead of full embedded mode, implement a **minimal JSONL output**:

### Option A: Post-hoc JSONL

Ted runs normally, but at the END, outputs a JSONL summary:

```rust
if args.embedded {
    // Run TUI silently (suppress output)
    let result = run_tui_silent(args, settings).await?;

    // Output final JSONL
    emit_completion_event(&result);
}
```

**Pros**:
- Much simpler (50 lines of code)
- Teddy gets file changes after Ted finishes

**Cons**:
- No streaming
- No real-time progress
- Teddy shows "loading..." until done

### Option B: Log Scraping

Teddy parses Ted's normal terminal output:

```typescript
// In TedRunner, parse TUI output
tedProcess.stdout.on('data', (data) => {
  const text = data.toString();

  // Look for patterns like:
  // "Creating file: src/app.tsx"
  // "Updated: package.json"

  if (text.includes('Creating file:')) {
    const path = extractPath(text);
    this.emit('file_create', { path });
  }
});
```

**Pros**:
- Zero Ted changes needed
- Works with current Ted

**Cons**:
- Fragile (depends on output format)
- Not deterministic
- Misses information

---

## Recommended Path Forward

### For This Week

**Use Option B** (log scraping) to unblock Teddy testing:

1. Update `TedRunner` to parse TUI output
2. Extract file operations from terminal text
3. Test the UI flow without perfect data

### Next Week

**Implement proper embedded mode** (Phase 1-4 above):

1. Day 1: Basic JSONL structure
2. Day 2: Event emission + agent integration
3. Day 3: Testing + refinement

---

## How to Test Teddy Now

Since embedded mode isn't ready, test these aspects:

### ✅ Can Test Now

- [x] Project picker UI
- [x] File tree navigation
- [x] Monaco editor (load/save files manually)
- [x] Chat panel UI (type messages, no responses)
- [x] Preview panel (manually start dev server)
- [x] Console panel (show mock logs)
- [x] Layout and styling

### ❌ Cannot Test Yet

- [ ] End-to-end prompt → file changes
- [ ] Ted event streaming
- [ ] Auto file updates
- [ ] Git auto-commit (can test manually)

---

## Updated Timeline

### Week 1 (Current)
- [x] Teddy UI scaffolded
- [x] Ted accepts `--embedded` flag (stub)
- [ ] Implement embedded mode in Ted ← **NEXT TASK**

### Week 2
- [ ] Test full integration
- [ ] Fix bugs
- [ ] Add Docker runtime detection

### Week 3-4
- [ ] Polish UX
- [ ] Package for distribution
- [ ] User testing

---

## Action Items

**Immediate (Today)**:
1. Decide: Quick hack (log scraping) or proper implementation?
2. If proper: Start Phase 1 of embedded mode
3. If quick: Implement log scraper in TedRunner

**This Week**:
1. Complete embedded mode implementation
2. Test Teddy end-to-end
3. Document any issues

---

**Status**: Documented
**Owner**: Development team
**Last Updated**: 2026-01-12
