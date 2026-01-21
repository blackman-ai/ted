# Ted & Teddy - Product Roadmap

**Last Updated**: 2026-01-19
**Vision**: A fully local, offline-first AI coding environment that adapts to any hardware
**Mission**: Disrupt billion-dollar companies for free. Make app building accessible to everyone—including grandma.

---

## Part 1: Current State - Feature Inventory

### Ted CLI - What's Built

| Category | Feature | Status | Notes |
|----------|---------|--------|-------|
| **LLM Providers** | Anthropic Claude | ✅ Complete | claude-sonnet-4, claude-3.5-sonnet, claude-3.5-haiku |
| | Ollama (Local) | ✅ Complete | qwen2.5-coder, llama3.2, deepseek-coder, etc. |
| | OpenRouter | ✅ Complete | 100+ models (GPT-4, Claude, Gemini, DeepSeek, Llama, etc.) |
| | Blackman AI | ✅ Complete | OpenAI-compatible cloud proxy with optimization |
| | OpenAI | ❌ Skeleton | Not implemented (use OpenRouter instead) |
| | Google Gemini | ❌ Skeleton | Not implemented (use OpenRouter instead) |
| **Tools** | file_read | ✅ Complete | Line offset/limit, truncation |
| | file_write | ✅ Complete | Create new files, auto-mkdir |
| | file_edit | ✅ Complete | Find/replace with uniqueness check |
| | shell | ✅ Complete | Timeout, dangerous command blocking |
| | glob | ✅ Complete | File pattern matching |
| | grep | ✅ Complete | Regex search with context |
| | plan_update | ✅ Complete | Task tracking |
| | database_init | ✅ Complete | Initialize SQLite/Postgres with Prisma |
| | database_migrate | ✅ Complete | Run Prisma migrations |
| | database_query | ✅ Complete | Execute SQL queries (read-only by default) |
| | database_seed | ✅ Complete | Run seed scripts for sample data |
| | file_changeset | ✅ Complete | Multi-file atomic/incremental changes |
| | External tools | 🔶 Partial | JSON-RPC protocol defined, not tested |
| **Caps System** | Base cap | ✅ Complete | Always loaded |
| | rust-expert | ✅ Complete | Rust best practices |
| | python-senior | ✅ Complete | Python expertise |
| | typescript-expert | ✅ Complete | TS/Node.js expertise |
| | security-analyst | ✅ Complete | Security focus |
| | code-reviewer | ✅ Complete | Review persona |
| | documentation | ✅ Complete | Docs writer |
| | Custom caps (TOML) | ✅ Complete | User-defined personas |
| | Cap inheritance | ✅ Complete | `extends` field |
| | Tool permissions | ✅ Complete | Per-cap enable/disable |
| **Context/Memory** | WAL storage | ✅ Complete | Hot/warm/cold tiers |
| | Session resumption | ✅ Complete | `--resume <id>` |
| | Smart indexer | ✅ Complete | Recency + frequency scoring |
| | Git analysis | ✅ Complete | Blame, commit frequency |
| | Language parsing | ✅ Complete | Rust, Python, TS, Go |
| | Dependency graph | ✅ Complete | Import/export tracking |
| | Conversation memory | ✅ Complete | Embeddings + semantic search |
| | Summarization | ✅ Complete | Auto-summarize long conversations |
| | Recall/RAG | ✅ Complete | Retrieve relevant past context |
| **CLI** | Interactive chat | ✅ Complete | Full REPL |
| | Single-shot ask | ✅ Complete | `ted ask "question"` |
| | History management | ✅ Complete | List, search, show, delete |
| | Context management | ✅ Complete | Stats, prune, clear |
| | Settings TUI | ✅ Complete | Interactive config editor |
| | Self-update | ✅ Complete | `ted update` |
| | Custom commands | ✅ Complete | `.ted/commands/` |
| **Embedded Mode** | JSONL output | ✅ Complete | Full agent loop with streaming |
| | All event types | ✅ Complete | 9 event types for Teddy |

### Teddy Desktop App - What's Built

| Category | Feature | Status | Notes |
|----------|---------|--------|-------|
| **UI Components** | Project picker | ✅ Complete | Folder selection dialog |
| | File tree | ✅ Complete | Expand/collapse, refresh |
| | Monaco editor | ✅ Complete | Syntax highlighting, save |
| | Chat panel | ✅ Complete | Streaming messages, events |
| | Console | ✅ Complete | Logs, stderr output |
| | Preview iframe | ✅ Complete | Auto-detects dev server, Vite/Next.js support |
| **Ted Integration** | Subprocess spawning | ✅ Complete | TedRunner |
| | JSONL parsing | ✅ Complete | TedParser |
| | File operations | ✅ Complete | FileApplier |
| | Git auto-commit | ✅ Complete | AutoCommit |
| | Multi-turn chat | ✅ Complete | Conversation history |
| | Diff view | ✅ Complete | Monaco diff editor, accept/reject changes |
| | Review mode | ✅ Complete | Queue changes for review before applying |
| **Deploy & Infra** | Deploy integration | ✅ Complete | Vercel deploy with one-click, Cloudflare tunnel sharing |
| | Dev server detection | ✅ Complete | Auto-detects Vite/Next.js, sets correct ports |
| | Settings UI | ✅ Complete | Full settings UI with Hardware/Providers/Deployment tabs |
| **Future Features** | Docker support | ❌ Stubbed | Button exists, not functional (Phase 4) |
| | PostgreSQL | ❌ Stubbed | Button exists, not functional (Phase 4) |

---

## Part 2: Competitive Comparison

### Ted vs OpenCode (CLI Agents)

| Feature | Ted | OpenCode | Winner |
|---------|-----|----------|--------|
| **Language** | Rust | Go | Tie |
| **LLM Providers** | 4 (Anthropic, Ollama, OpenRouter, Blackman) | 75+ (via Models.dev) | Tie |
| **Local Models** | ✅ Ollama | ✅ Multiple | Tie |
| **Customization** | Caps (composable) | Agents (single mode) | **Ted** |
| **Stack Multiple Personas** | ✅ Yes | ❌ No | **Ted** |
| **Tool Permissions** | Per-cap granular | Per-agent | **Ted** |
| **Context Memory** | WAL + smart indexer | Unknown | **Ted** |
| **MCP Protocol** | ✅ Yes | ✅ Yes | Tie |
| **LSP Integration** | ✅ Yes | ✅ Yes | Tie |
| **GitHub Actions** | ❌ No | ✅ /opencode mentions | OpenCode |
| **IDE Extension** | ✅ Yes | ✅ Yes | Tie |
| **GUI Companion** | ✅ Teddy | ❌ No | **Ted** |
| **License** | AGPL-3.0 | MIT | Preference |
| **Multi-session** | ✅ Yes | ✅ Yes | Tie |
| **Shareable Sessions** | ❌ No | ✅ Yes | OpenCode |

**Summary**: Ted wins on customization (caps) and memory. OpenCode wins on provider breadth and integrations.

### Teddy vs Lovable vs v0 vs Bolt.new (App Builders)

| Feature | Teddy | Lovable | v0 | Bolt.new |
|---------|-------|---------|----|---------|
| **Runs Locally** | ✅ Yes | ❌ Cloud only | ❌ Cloud only | ❌ Cloud only |
| **Offline Mode** | ✅ Yes (Ollama) | ❌ No | ❌ No | ❌ No |
| **Open Source** | ✅ AGPL | ❌ Proprietary | ❌ Proprietary | ❌ Proprietary |
| **Self-Hosted** | ✅ Yes | ❌ No | ❌ No | ❌ No |
| **Pricing** | Free | Credit-based ($20+) | Credit-based ($20+) | Token-based ($20-200) |
| **Backend Generation** | 🔶 Via Ted | ✅ Supabase auto | 🔶 Limited | ✅ Node.js + Supabase |
| **Database Setup** | ✅ SQLite + Prisma | ✅ Auto Supabase | ❌ No | ✅ Auto Supabase |
| **Deployment** | ✅ Vercel, Netlify, CF Tunnel | ✅ One-click | ✅ Vercel native | ✅ Netlify native |
| **Tech Stack** | React + any | React + Tailwind | React + Next.js + Tailwind | React + Tailwind |
| **Agent Mode** | ✅ Yes (Ted) | ✅ Yes (autonomous) | ✅ Yes (agentic) | 🔶 Discussion mode |
| **Git Integration** | ✅ Auto-commit | ✅ GitHub sync | ✅ GitHub | ❌ Manual export |
| **Visual Editing** | ❌ Monaco only | ✅ Click-to-edit | 🔶 Limited | 🔶 Limited |
| **Figma Import** | ❌ No | ❌ No | ❌ No | ✅ Yes |
| **Code Ownership** | ✅ Full | ✅ Full | ✅ Full | ✅ Full |
| **Model Selection** | ✅ Any (Ollama) | ❌ Fixed | ❌ Claude only | ❌ Fixed |
| **Hardware Adaptive** | ✅ Yes (7 tiers) | ❌ N/A | ❌ N/A | ❌ N/A |

**Summary**: Teddy's differentiators are local-first, offline, open-source, and model flexibility. Competitors win on polish, integrations, and deployment.

---

## Part 3: Your Unique Value Proposition

### Ted's Moat
1. **Composable Caps** - Only system that lets you stack personas
2. **Smart Memory** - Sophisticated context prioritization
3. **Full Control** - AGPL ensures it stays open

### Teddy's Moat
1. **100% Local** - No cloud, no credits, no vendor lock-in
2. **Hardware Adaptive** - Works on any modern laptop (your vision)
3. **Model Flexibility** - Swap models freely (Ollama ecosystem)
4. **Open Source** - Hackable, self-hostable

---

## Part 4: Hardware-Adaptive Model Strategy

This is your key differentiator. Here's how to implement it:

### The "2010 Dell Benchmark" - Our IE 11 Moment

**Philosophy**: Just as web developers grudgingly supported IE 11 to reach everyone, Ted/Teddy will support 2010-era hardware to democratize AI coding.

**The Benchmark Machine**: A refurbished 2010 Dell OptiPlex ($100) with minimal upgrades ($50):
- Intel Core 2 Duo or Core i3 (2-4 cores, ~2.5-3.0 GHz)
- 16GB DDR3 RAM (upgraded from 4-8GB stock)
- 240GB SSD (upgraded from HDD)
- Integrated graphics (no dedicated GPU)
- **Total cost: $150**

**What it should do**:
- ✅ Build simple single-page apps (blogs, portfolios, to-do lists)
- ✅ Run qwen2.5-coder:1.5b at 5-10 tokens/sec
- ✅ Preview with Vite dev server
- ✅ Make iterative changes via chat
- ⚠️ 30-60 second AI response times (acceptable with proper UX)
- ❌ Cannot handle: Multi-page complex apps, large refactorings, 7b+ models

**Why this matters**: Proves that **anyone with a $150 computer can build software**. No other AI coding tool can claim this.

### System Detection Tiers

| Tier | RAM | VRAM | CPU | CPU Age | Recommended Models |
|------|-----|------|-----|---------|-------------------|
| **UltraTiny** | 8GB | None | 4 ARM cores | 2020+ | qwen2.5-coder:1.5b (Q3_K_M) |
| **Ancient** | 8-16GB | None | 2-4 cores x86 | 2010-2015 | qwen2.5-coder:1.5b |
| **Tiny** | 8-16GB | None | 4 cores | 2015-2020 | qwen2.5-coder:1.5b, phi-3-mini |
| **Small** | 8-16GB | ≤4GB | 4-8 cores | 2018+ | qwen2.5-coder:3b, codellama:7b |
| **Medium** | 16-32GB | 4-8GB | 8+ cores | 2020+ | qwen2.5-coder:7b, deepseek-coder:6.7b |
| **Large** | 32GB+ | 8-16GB | 8+ cores | 2021+ | qwen2.5-coder:14b, codellama:34b |
| **Cloud** | Any | Any | Any | Any | claude-sonnet-4, gpt-4o |

### Adaptive Behavior

| Tier | Max Context | Response Strategy | Cap Adjustments | Background Tasks |
|------|-------------|-------------------|-----------------|------------------|
| **UltraTiny** | 512 tokens | Extremely directive, one-shot | Minimal, tutorial mode | All disabled, thermal aware |
| **Ancient** | 1K tokens | Ultra-directive, single task | Minimal caps, heavy examples | All disabled |
| **Tiny** | 2K tokens | More directive prompts, step-by-step | Simplified caps, more examples | Indexer disabled |
| **Small** | 4K tokens | Focused context, single file | Standard caps | Indexer low priority |
| **Medium** | 8K tokens | Multi-file context | Full caps | Full indexer |
| **Large** | 16K+ tokens | Full project context | All features | Full indexer |
| **Cloud** | 100K+ tokens | Unlimited context | Maximum capability | All features |

### Implementation Plan

```rust
// src/hardware/detector.rs
pub struct SystemProfile {
    pub ram_gb: u32,
    pub vram_gb: Option<u32>,
    pub cpu_cores: u32,
    pub cpu_year: Option<u32>,  // Estimated CPU generation year
    pub has_ssd: bool,
    pub architecture: CpuArchitecture,  // x86_64, ARM64, etc.
    pub is_sbc: bool,  // Single-board computer (Raspberry Pi, etc.)
    pub tier: HardwareTier,
}

pub enum CpuArchitecture {
    X86_64,
    ARM64,
    ARM32,
}

pub enum HardwareTier {
    UltraTiny,  // 2020+: Raspberry Pi 5, ARM SBCs - education & embedded
    Ancient,    // 2010-2015: The "IE 11 Benchmark" - refurbished PCs
    Tiny,       // 2015-2020: Chromebooks, old laptops
    Small,      // 2018+: Entry MacBook Air, basic laptops
    Medium,     // 2020+: MacBook Pro M1/M2, gaming laptops
    Large,      // 2021+: Pro workstations, Mac Studio
    Cloud,      // Using API providers
}

impl SystemProfile {
    pub fn detect() -> Self { ... }
    pub fn recommended_model(&self) -> &str { ... }
    pub fn max_context_tokens(&self) -> usize { ... }
    pub fn should_use_streaming(&self) -> bool { ... }
    pub fn get_upgrade_suggestions(&self) -> Vec<Upgrade> { ... }
    pub fn meets_minimum_requirements(&self) -> Result<(), String> { ... }
    pub fn is_raspberry_pi(&self) -> bool { ... }
    pub fn thermal_throttle_risk(&self) -> bool { ... }
}

pub struct Upgrade {
    pub component: String,  // "RAM", "Storage", etc.
    pub current: String,
    pub recommended: String,
    pub estimated_cost: String,
    pub performance_gain: String,
}
```

### Guardrails for UltraTiny/Ancient/Tiny Systems

#### 1. **Hardware Detection & Soft Blocking**
```rust
if profile.ram_gb < 8 {
    return Err("Minimum 8GB RAM required. Current: {}GB. Upgrade cost: ~$30-40.", profile.ram_gb);
}

// Raspberry Pi specific checks
if profile.is_raspberry_pi() {
    if !profile.has_ssd {
        warn!("microSD detected. NVMe HAT + SSD ($45) will improve loading by 10x.");
    }
    if profile.thermal_throttle_risk() {
        warn!("Active cooling recommended for sustained AI workloads ($10-15).");
    }
}

// x86 PC specific checks
if profile.tier == Ancient && !profile.has_ssd {
    warn!("HDD detected. Upgrading to SSD ($25-35) will improve loading by 10x.");
}
```

#### 2. **Automatic Model Selection with User Education**
- Detect hardware tier on first launch
- Show what the system can handle with visual examples:
  ```
  ✅ Your PC can build: Blogs, portfolios, simple tools
  ⚠️ Expect: 30-60 second responses (be patient!)
  ❌ Cannot build: Complex multi-page apps, enterprise software

  Recommended model: qwen2.5-coder:1.5b (2GB)
  Why: Optimized for older CPUs, still surprisingly capable
  ```

#### 3. **Aggressive Resource Management**
```rust
match profile.tier {
    UltraTiny => Config {
        max_context_tokens: 512,
        max_warm_chunks: 5,       // Extremely limited
        disable_background_tasks: true,
        streaming_only: true,
        single_file_mode: true,
        quantization: "Q3_K_M",   // Ultra-heavy quantization for ARM
        monitor_thermal: true,    // Pause on thermal throttling
        electron_optimization: ElectronMode::Minimal,  // Reduce UI overhead
    },
    Ancient => Config {
        max_context_tokens: 1024,
        max_warm_chunks: 10,      // vs 100 default
        disable_background_tasks: true,
        streaming_only: true,
        single_file_mode: true,
        quantization: "Q4_K_M",   // Heavy quantization
    },
    Tiny => Config {
        max_context_tokens: 2048,
        max_warm_chunks: 20,
        disable_indexer: true,
        streaming_only: true,
    },
    // ... other tiers
}
```

#### 4. **Prompt Optimization for Small Models**
Ancient/Tiny tiers get ultra-directive system prompts:
- Explicit step-by-step instructions
- No creative freedom (model is too small)
- More examples in prompts
- Single file focus only

#### 5. **UX Adaptations for Slow Hardware**
```
🐢 Ancient Hardware Detected

Expected response time: 30-60 seconds
Your PC is working hard! While you wait:
• Grab some coffee ☕
• Review the last change
• Think about your next request

[Why so slow?] [Upgrade guide: Get to 5 seconds for $50]
```

#### 6. **Memory Monitoring & Graceful Degradation**
- Monitor Ollama process memory
- If approaching system limits, pause and warn
- Suggest closing other apps
- Never crash - always degrade gracefully

#### 7. **Upgrade Path Guidance**
When Ancient tier detected:
```
💡 Upgrade Suggestions for Better Performance

Current: 8GB RAM, HDD, Core 2 Duo (2010)

Priority 1: SSD ($25-35 used)
  ▸ 10x faster model loading
  ▸ Snappier file operations

Priority 2: RAM to 16GB ($30-40 used DDR3)
  ▸ Use larger 3b models
  ▸ 2-3x faster responses

Total cost: $55-75 → Moves you to "Tiny" tier

[Show me compatible parts] [I'll upgrade later]
```

### Testing the "2010 Dell Benchmark"

We commit to:
1. **Acquire a real 2010 Dell OptiPlex** for testing
2. **Test every release** on this benchmark machine
3. **Document the experience** with screen recordings
4. **Publish "The $150 Coding PC"** build guide
5. **Never break backwards compatibility** without major version bump

This is our contract with the community: *If it doesn't work on the benchmark, we don't ship.*

### The Raspberry Pi Opportunity - UltraTiny Tier

**Why Raspberry Pi matters**: While the 2010 Dell proves we support "anyone with an old PC," Raspberry Pi opens entirely different use cases:

**Target audiences**:
- **Education**: Schools already have Raspberry Pis in classrooms
- **Developing countries**: Easier to ship/import than desktop PCs
- **Makers**: Integrate AI coding into robotics/IoT projects
- **Always-on**: Low power consumption for background tasks
- **Kids**: Less intimidating than a "real computer"

**The Raspberry Pi 5 Build** ($125 total):
- Raspberry Pi 5 8GB: $80
- NVMe HAT: $15 (PiNVMe, Argon ONE, etc.)
- 256GB NVMe SSD: $30
- Active cooling: Optional but recommended ($10-15)

**Performance expectations**:
- qwen2.5-coder:1.5b: 5-15 tokens/sec (CPU only, skip AI HAT+ 2)
- Response time: 15-40 seconds
- **Key advantage**: Doesn't hog CPU, so Teddy UI stays responsive

**Raspberry Pi-specific optimizations**:
1. **ARM64 builds** - Native ARM compilation for Ted CLI and Teddy
2. **Thermal monitoring** - Pause inference if CPU temp >80°C
3. **Power-aware scheduling** - Detect if running on battery/portable power
4. **Minimal Electron mode** - Reduce UI overhead on limited RAM
5. **SD card detection** - Warn if using microSD instead of NVMe

**Why NOT the AI HAT+ 2** ($130):
- Benchmarks show it's **slower than the Pi's CPU** for LLM inference
- Adds $130 to cost (more than the Pi itself!)
- Only helps if you need CPU free for GPIO/robotics
- Models are limited to 1.5B and heavily quantized (INT4)

**The education pitch**:
> "Every classroom Raspberry Pi can now teach kids to build websites with AI assistance. No cloud required. No API keys. No subscriptions."

**Implementation priority**: Phase 2-3
- Phase 1: Focus on 2010 Dell (x86_64 baseline)
- Phase 2: Add ARM64 builds for Pi 5
- Phase 3: Raspberry Pi-specific optimizations (thermal, storage detection)

---

## Part 5: Critical Feature Gaps Analysis

These are the features we **must** have to compete. Prioritized by user impact.

### ✅ Closed Gaps (Previously Critical)

| Feature | Status | Notes |
|---------|--------|-------|
| **Database setup** | ✅ Complete | SQLite + Prisma with 4 tools |
| **Diff view** | ✅ Complete | Monaco diff editor, accept/reject |
| **One-click deploy** | ✅ Complete | Vercel, Netlify, Cloudflare Tunnel |
| **MCP protocol** | ✅ Complete | Full stdio server implementation |
| **OpenRouter provider** | ✅ Complete | 100+ models accessible |
| **Multi-session** | ✅ Complete | Full session CRUD + UI |
| **LSP integration** | ✅ Complete | Autocomplete, go-to-def, hover |
| **VS Code extension** | ✅ Complete | Full IDE integration |
| **Settings UI** | ✅ Complete | Hardware/Providers/Deployment/Database tabs |
| **File watching** | ✅ Complete | chokidar with ignore patterns |
| **PostgreSQL + Docker** | ✅ Complete | Docker container management, database_query support |

### 🟢 Remaining Gaps (Nice-to-Have / Differentiation)

| Gap | Who Has It | Why It Matters | Effort | Phase |
|-----|------------|----------------|--------|-------|
| **Visual editing** | Lovable | Click-to-edit without chat | 2 weeks | 4 |
| **GitHub Actions bot** | OpenCode | `/ted` mentions in PRs | 3 days | 5 |
| **Figma import** | Bolt.new | Design-to-code workflow | 1 week | 5 |
| **Shareable sessions** | OpenCode | Collaboration features | 4 days | 3 |

Note: Plugin system removed - Teddy is not a full IDE. Users who want extensibility should use Ted via the VS Code extension.

### Gap Closure Strategy

**Phase 1-2**: ✅ COMPLETE - Database, Diff, OpenRouter, Deploy, MCP all done
**Phase 3 Focus**: Pi optimizations, shareable sessions, polish
**Phase 4 Focus**: Visual editing + SQLite→PostgreSQL migration = "Differentiation" (Docker/Postgres core done)
**Phase 5 Focus**: GitHub Actions bot, Figma import = "Nice-to-have"

---

## Part 6: Database Strategy

### Philosophy: SQLite First, Postgres When Needed

| Consideration | SQLite | PostgreSQL |
|---------------|--------|------------|
| **Setup** | Zero - it's a file | Requires Docker |
| **Non-coder friendly** | Perfect - invisible | "What's a container?" |
| **Local-first ethos** | Aligns perfectly | Feels "server-y" |
| **Teddy's target user** | Blogs, small apps | Production apps |
| **Offline support** | Works completely offline | Needs running server |

### Implementation

**Phase 1: SQLite (Default)**
```
User: "Create a recipe blog with favorites"
Ted: Creates SQLite database, Prisma schema, migrations
Teddy: Database just works - no setup, no Docker
```

**Phase 2: PostgreSQL (Upgrade Path)**
```
User: "I need user authentication and real-time updates"
Teddy: "This needs PostgreSQL. [Start Docker Container]"
Ted: Migrates schema from SQLite → Postgres
```

### Database Tools for Ted

| Tool | Purpose | Priority |
|------|---------|----------|
| `database_init` | Create SQLite DB + Prisma schema | P0 |
| `database_migrate` | Run Prisma migrations | P0 |
| `database_query` | Execute SQL (read-only by default) | P1 |
| `database_seed` | Populate with sample data | P2 |

### ORM Strategy

**Prisma** as the default because:
- Works with both SQLite and Postgres
- Type-safe queries (matches our TypeScript focus)
- Migrations built-in
- Most LLMs know it well

```prisma
// Example: What Ted generates for "recipe blog with favorites"
datasource db {
  provider = "sqlite"  // Upgradeable to "postgresql"
  url      = "file:./dev.db"
}

model Recipe {
  id          Int      @id @default(autoincrement())
  title       String
  ingredients String
  steps       String
  createdAt   DateTime @default(now())
  favorites   Favorite[]
}

model Favorite {
  id       Int    @id @default(autoincrement())
  recipeId Int
  recipe   Recipe @relation(fields: [recipeId], references: [id])
}
```

---

## Part 7: Roadmap

### Phase 1: Foundation + Critical Gaps (Weeks 1-2)

**Goal**: Real apps, built safely, with model choice. **Passes the "2010 Dell Benchmark".**

| Task | Priority | Effort | Status | Notes |
|------|----------|--------|--------|-------|
| **SQLite + Prisma integration** | P0 | 5 days | ✅ Done | `database_init`, `database_migrate`, `database_query`, `database_seed` tools |
| **Diff view in Teddy** | P0 | 4 days | ✅ Done | Monaco diff editor, accept/reject individual or all changes |
| **OpenRouter provider** | P0 | 3 days | ✅ Done | 100+ models via single API, streaming support |
| **Hardware detection module** | P0 | 4 days | ✅ Done | Full detection: RAM/VRAM/CPU/CPU age/SSD/architecture/SBC detection + `ted system` command |
| **Tier-based config system** | P0 | 3 days | ✅ Done | 7 tiers with adaptive config, model recommendations, upgrade suggestions |
| **Settings UI in Teddy** | P1 | 3 days | ✅ Done | Provider/model/API key config + hardware display with 2 tabs |
| **Model recommendation engine** | P1 | 2 days | ✅ Done | Built into tier system with per-tier model lists |
| **Upgrade suggestions UI** | P1 | 2 days | ✅ Done | CLI via `ted system --upgrades` + Hardware tab in Teddy UI |
| **Fix Teddy preview** | P2 | 2 days | ✅ Done | Auto-detects Vite/Next.js, sets correct ports, shows project type badge |
| End-to-end Teddy testing | P0 | 2 days | 🟡 In progress | Verify full agent loop |
| Acquire 2010 Dell for testing | P2 | 1 day | 🔴 Not started | eBay/Craigslist, $100-150 budget |

**Phase 1 Deliverable**: User can build a blog with SQLite database, review AI changes before applying, choose from 100+ models. **Works acceptably on a $150 refurbished 2010 Dell OptiPlex.**

### Phase 2: Deploy + Ecosystem (Weeks 3-5)

**Goal**: Ship to the world, connect to tools, **support Raspberry Pi**

| Task | Priority | Effort | Status | Notes |
|------|----------|--------|--------|-------|
| **MCP protocol support** | P0 | 5 days | ✅ Done | External tool ecosystem - exposes all built-in tools via stdio |
| **Vercel deploy integration** | P0 | 3 days | ✅ Done | One-click deploy via API, auto-detection, token verification |
| **Cloudflare Tunnel sharing** | P0 | 3 days | ✅ Done | Instant share links via cloudflared, auto-download, auto-copy to clipboard |
| **ARM64 builds** | P0 | 3 days | ✅ Done | Raspberry Pi 5 support - electron-builder config, cross-compile script, GitHub Actions |
| **Bundled dependencies** | P0 | 2 days | ✅ Done | Auto-download cloudflared, no manual installs needed, batteries-included |
| Blackman AI provider | P1 | 2 days | ✅ Done | OpenAI-compatible, full streaming support |
| Netlify deploy | P1 | 2 days | ✅ Done | One-click deploy, token verification, dropdown menu in Preview |
| Multi-session support | P1 | 3 days | ✅ Done | Full session CRUD, UI, backend storage |
| File watching (chokidar) | P2 | 2 days | ✅ Done | Full chokidar implementation with ignore patterns |
| teddy.rocks subdomain service | P2 | 4 days | ✅ Done | CF Workers + KV, zero-config sharing via `*.teddy.rocks` |
| Acquire Raspberry Pi 5 for testing | P2 | 1 day | ✅ Done | Have hardware |

**Phase 2 Deliverable**: User can deploy to Vercel or Netlify with one click, share preview links via teddy.rocks, use MCP tools. **Works on Raspberry Pi 5 (ARM64).** ✅ **COMPLETE**

### Phase 3: Developer Experience + Pi Optimizations (Weeks 6-8)

**Goal**: Pro developers feel at home, **Raspberry Pi gets optimized**

| Task | Priority | Effort | Status | Notes |
|------|----------|--------|--------|-------|
| **Tool output streaming** | P0 | 2 days | ✅ Done | Real-time command output in chat (already complete in codebase) |
| **Error recovery + retry** | P0 | 2 days | ✅ Done | Exponential backoff, circuit breaker, smart defaults |
| **Multi-file context** | P0 | 3 days | ✅ Done | FileChangeSetTool with atomic/incremental modes, integrated with embedded_runner |
| **Conversation memory/RAG** | P0 | 5 days | ✅ Done | Embeddings, SQLite storage, summarization, recall integration complete |
| **LSP integration** | P0 | 1 week | ✅ Done | Autocomplete, go-to-def, hover - `ted lsp` command |
| **VS Code extension** | P0 | 1 week | ✅ Done | Full IDE integration in `vscode-extension/` |
| Thermal monitoring (Pi) | P1 | 2 days | 🔴 Not started | Pause inference at 80°C, UltraTiny tier |
| Minimal Electron mode (Pi) | P1 | 3 days | 🔴 Not started | Reduce UI overhead for 8GB RAM |
| Storage detection (Pi) | P1 | 1 day | 🔴 Not started | Warn on microSD, recommend NVMe |
| Undo/redo for file ops | P1 | 3 days | 🔴 Not started | Beyond Git |
| Keyboard shortcuts | P1 | 2 days | 🔴 Not started | Power user UX |
| File tree search | P1 | 2 days | 🔴 Not started | Find files quickly |
| Shareable sessions | P2 | 4 days | 🔴 Not started | Collaboration |
| Dark/light theme toggle | P2 | 1 day | 🔴 Not started | User preference |

**Phase 3 Deliverable**: Developers get IDE-quality experience, can use Ted from VS Code. **Raspberry Pi 5 optimized for education use cases.**

### Phase 4: Backend + Advanced (Weeks 9-12)

**Goal**: Full-stack apps, differentiated features

| Task | Priority | Effort | Status | Notes |
|------|----------|--------|--------|-------|
| **Visual editing** | P0 | 2 weeks | 🔴 Not started | Click-to-edit UI (see detailed plan below) |
| **Docker detection** | P0 | 2 days | ✅ Done | `teddy/electron/docker/detector.ts` |
| **PostgreSQL container management** | P0 | 3 days | ✅ Done | `teddy/electron/docker/container-manager.ts` |
| **PostgreSQL query support** | P0 | 2 days | ✅ Done | `database_query` tool updated in `database.rs` |
| **Database Settings UI tab** | P1 | 2 days | ✅ Done | Settings → Database tab with Docker/Postgres controls |
| **SQLite → PostgreSQL migration** | P1 | 2 days | 🔴 Not started | Data export/import tooling |
| Domain purchase flow | P2 | 4 days | 🔴 Not started | Affiliate integration |

#### Visual Editing Implementation Plan

**Architecture**: Injection script + postMessage bridge

1. **Element Detection** (2-3 days)
   - Create `injection-script.ts` that detects clicks in preview iframe
   - Build XPath generator to identify element location in DOM
   - Implement postMessage communication between iframe and React app
   - Add "Inspection Mode" toggle button to Preview toolbar

2. **Element Display** (1-2 days)
   - Show selection overlay on clicked elements with bounds highlighting
   - Display element properties panel (tag, classes, id, text content)
   - Create visual selection highlight effect

3. **Text Editing** (2-3 days)
   - Build file search to locate element's text content in source
   - Create inline edit UI (popup or modal)
   - Implement file write and auto-refresh flow

4. **Style Editing** (3-4 days)
   - Extract computed styles from selected element
   - Build style editor UI (color picker, font size, spacing, etc.)
   - Implement CSS class/inline style updates in source
   - Add instant visual feedback in preview

5. **Source Mapping** (4-5 days)
   - Use regex patterns to find component definitions
   - Match CSS class names to relevant source files
   - Fall back to user manual file selection when mapping fails
   - Optional: Support source maps for projects that have them

**Technical Constraints**:
- Preview iframe runs on localhost (cross-origin from Electron)
- Must inject script that communicates via postMessage
- Cannot directly access iframe DOM from parent
- Source mapping is heuristic-based (not 100% accurate like Lovable's Babel plugin)

#### PostgreSQL + Docker Implementation Plan

**Philosophy**: SQLite is default. Docker+PostgreSQL are optional, not forced.

1. **Docker Detection** (2 days)
   - Create `teddy/electron/docker/detector.ts`
   - Functions: `isDockerInstalled()`, `isDockerDaemonRunning()`, `getDockerVersion()`
   - Clear error messages with install guides when Docker missing

2. **Container Management** (3 days)
   - Create `teddy/electron/docker/container-manager.ts`
   - Start/stop/restart PostgreSQL container via Docker CLI
   - Service registry in `~/.teddy/docker/services.json`
   - Volume mapping to `~/.teddy/docker/postgres-data/`
   - Auto-configure `DATABASE_URL` in project's `.env`

3. **Complete PostgreSQL Query Support** (2 days)
   - Finish `database_query` tool for PostgreSQL (currently returns error)
   - Use `psql` CLI or add `pg` npm dependency
   - Connection validation with timeout and retry logic

4. **Settings UI** (2 days)
   - Add "Database & Services" tab to Settings
   - Show Docker status and version
   - PostgreSQL service controls (Start/Stop)
   - Connection string display and edit
   - Data backup/restore buttons

5. **Migration Tooling** (2 days)
   - Export SQLite data to SQL dump
   - Import into PostgreSQL container
   - Schema sync via Prisma migrations
   - Backup original SQLite file

**Hardware Tier Warnings**:
```
UltraTiny/Ancient: "Docker not recommended. Use SQLite."
Tiny: "PostgreSQL may be slow. SQLite preferred."
Small+: "PostgreSQL fully supported."
```

**Phase 4 Deliverable**: Full-stack apps with PostgreSQL, visual editing for non-coders.

### Phase 5: Distribution + Polish (Weeks 13-16)

**Goal**: Easy installation, professional packaging, nice-to-have integrations

| Task | Priority | Effort | Status | Notes |
|------|----------|--------|--------|-------|
| macOS DMG packaging | P0 | 2 days | 🔴 Not started | Signed + notarized |
| Windows installer | P0 | 2 days | 🔴 Not started | MSI/NSIS |
| Linux AppImage | P1 | 1 day | 🔴 Not started | Universal Linux |
| Auto-update for Teddy | P1 | 3 days | 🔴 Not started | Electron auto-updater |
| Homebrew formula | P2 | 1 day | 🔴 Not started | `brew install ted` |
| GitHub Actions bot | P2 | 3 days | 🔴 Not started | `/ted` mentions in PR comments |
| Figma import | P3 | 1 week | 🔴 Not started | Design-to-code workflow |
| Multi-agent orchestration | P3 | 1 week | 🔴 Not started | Parallel agents |
| ted_core library extraction | P3 | 2 weeks | 🔴 Not started | Eliminate subprocess overhead |

**Phase 5 Deliverable**: One-click install on all platforms, auto-updates, GitHub/Figma integrations.

### Share Flow Architecture

The "Grandma Test" flow:
```
1. "Create a blog about home gardening"
2. Teddy builds it locally, auto-starts preview
3. Chat to refine: "Make the header green"
4. Click "Share" → Instant public URL via tunnel
5. Send link to friends (no more "localhost:3000" mistakes!)
6. Click "Go Live" → Pick/buy domain → Deployed
```

#### teddy.rocks Subdomain Service

Share domain: **teddy.rocks** (because "your app rocks!")

Architecture (runs on Cloudflare free tier):
```
User clicks "Share" in Teddy
         ↓
Teddy generates slug: "garden-blog-7x3k"
         ↓
Teddy starts cloudflared tunnel locally
         ↓
Teddy calls API: POST teddy.rocks/register { slug, tunnel_id }
         ↓
Cloudflare Worker stores mapping in KV
         ↓
garden-blog-7x3k.teddy.rocks → routes to user's tunnel
         ↓
When Teddy closes, tunnel dies, link expires
```

For permanent deploys:
- Free tier: `myapp.teddy.rocks` → CF Pages static hosting
- Custom domain: User buys via Cloudflare Registrar (affiliate link)

#### Share UI Design

```
┌─────────────────────────────────────────┐
│  🌐 Share Your App                      │
│                                         │
│  ○ Preview Link (while Teddy is open)   │
│    garden-blog-7x3k.teddy.rocks         │
│    [Create Preview Link]                │
│                                         │
│  ○ Permanent Link (free, always on)     │
│    garden-blog.teddy.rocks              │
│    [Deploy Free →]                      │
│                                         │
│  ○ Custom Domain                        │
│    [ mygardenblog.com ]                 │
│    [Check Availability - $12/yr]        │
│                                         │
└─────────────────────────────────────────┘
```

---

## Part 8: Success Metrics

### The "2010 Dell Benchmark" - Mandatory Testing
Every release MUST pass these tests on the benchmark machine:

**Benchmark Hardware**: 2010 Dell OptiPlex
- Intel Core 2 Duo / Core i3 (2-4 cores)
- 16GB DDR3 RAM (upgraded)
- 240GB SSD (upgraded)
- Integrated graphics

**Required Tests**:
- [ ] Teddy launches in <10 seconds
- [ ] Hardware detection correctly identifies as "Ancient" tier
- [ ] Shows appropriate upgrade suggestions
- [ ] Ollama + qwen2.5-coder:1.5b installs successfully
- [ ] Can build a simple blog (single page with form)
- [ ] AI responses complete in <90 seconds
- [ ] Preview launches and refreshes
- [ ] Can make 3 iterative changes without crashing
- [ ] Memory usage stays under 12GB throughout session
- [ ] No background processes consume CPU when idle

**Pass Criteria**: 9/10 tests pass. If <9, release is BLOCKED until fixed.

### Short-term (1 month)
- [ ] Phase 1 complete: SQLite, Diff view, OpenRouter, Hardware detection working
- [ ] Teddy works end-to-end on all 3 platforms
- [ ] **Passes 2010 Dell Benchmark (9/10 tests)**
- [ ] Hardware detection correctly identifies 90% of systems
- [ ] Published "The $150 Coding PC" build guide
- [ ] 50+ GitHub stars

### Medium-term (3 months)
- [ ] Phase 2 complete: Deploy, MCP, sharing, ARM64 builds working
- [ ] 5+ LLM providers supported
- [ ] One-click deploy to Vercel
- [ ] **2010 Dell Benchmark maintained across all updates**
- [ ] **Raspberry Pi 5 passes UltraTiny tier tests**
- [ ] Community testing on 10+ different ancient hardware configs
- [ ] Published "The $125 Raspberry Pi AI Coding Computer" guide
- [ ] 500+ GitHub stars
- [ ] 100+ active users (20%+ on "Ancient", "Tiny", or "UltraTiny" tier)

### Long-term (6 months)
- [ ] Phase 4 complete: PostgreSQL, visual editing
- [ ] Feature parity with Lovable core features
- [ ] MCP ecosystem compatibility
- [ ] **Every ancient tier feature maintains <90s response time on benchmark**
- [ ] **Raspberry Pi optimizations complete (thermal, power-aware, minimal UI)**
- [ ] 2000+ GitHub stars
- [ ] Community-contributed caps
- [ ] Documentary evidence: "I built my first app on a $150 PC" stories
- [ ] At least one school/classroom using Ted/Teddy on Raspberry Pi

---

## Part 9: Technical Debt to Address

### High Priority
1. **Type safety across IPC** - Shared types between Electron main/renderer
2. **Error boundaries in React** - Prevent UI crashes
3. **File tree performance** - Slow on large projects (>1000 files)
4. **Test coverage** - Need unit tests for TedParser, FileApplier

### Medium Priority
1. **State management** - Add Zustand/Jotai before complexity grows
2. **File watching** - chokidar for external change detection
3. **Logging framework** - Structured logging for debugging
4. **Metrics/telemetry** - Opt-in usage analytics

### Low Priority
1. **Code splitting** - Lazy load Monaco editor
2. **Accessibility** - Keyboard navigation, screen readers
3. **i18n** - Internationalization framework
4. **Documentation site** - Beyond README

---

## Part 10: Sustainability Model

### Philosophy

**100% Free. 100% Open Source. Forever.**

We're not building a business—we're building a public good that happens to disrupt billion-dollar companies. The goal is sustainability, not profit.

### Revenue Streams (All Optional/Passive)

| Source | How It Works | Expected Revenue |
|--------|--------------|------------------|
| **Domain Affiliates** | Cloudflare Registrar referral links | $2-5 per domain purchase |
| **Hosting Affiliates** | Vercel/Netlify/DO referral programs | Varies |
| **GitHub Sponsors** | Individual and org sponsors | Community-dependent |
| **Open Collective** | Transparent community funding | Community-dependent |
| **Ko-fi / Buy Me a Coffee** | One-time donations | Small but appreciated |

### Transparency Commitment

All funding and spending will be public via Open Collective. No dark patterns. No upsells. No "premium tiers" that make the free version feel broken.

### Infrastructure Costs (Target: $0/month)

| Component | Provider | Cost |
|-----------|----------|------|
| teddy.rocks DNS | Cloudflare | Free |
| Subdomain routing | Cloudflare Workers | Free (100K req/day) |
| KV storage | Cloudflare Workers KV | Free tier |
| Static hosting | Cloudflare Pages | Free |
| Tunnels | cloudflared | Free (runs locally) |

The only real cost is the domain registration (~$10/year) and time maintaining it.

### Why This Works

1. **Cloudflare's free tier is absurd** - Workers, Pages, KV, R2, Tunnels... all free for our scale
2. **No servers to maintain** - Everything is edge/serverless or runs on user's machine
3. **Community motivation** - People will sponsor projects they love and use daily
4. **Affiliate revenue is passive** - Domain purchases are a natural part of the flow

### The "Fuck You" Model

Lovable: $6.6B valuation, charges per prompt
v0: Vercel-backed, credit-based pricing
Bolt.new: Token-based, $20-200/month

Teddy: Free. Open source. Runs on your hardware. Funded by vibes and affiliate links.

*This isn't a business. It's a statement.*

---

## Part 11: Blackman AI Integration (Optional Cloud Provider)

Ted/Teddy is local-first, but users who want cloud models have options.

### Provider Options

| Provider | How It Works | Cost | Best For |
|----------|--------------|------|----------|
| **Ollama (Local)** | Runs entirely on your machine | Free | Privacy, offline, no API keys |
| **Blackman AI** | First-party optimized cloud proxy | Usage-based | Best of both worlds |
| **Bring Your Own Key** | Direct to Anthropic/OpenAI/etc | Provider pricing | Power users with existing keys |

### What is Blackman AI?

[Blackman AI](https://useblackman.ai) is a sister project—an intelligent LLM routing proxy that:

- **Reduces token costs 15-30%** through prompt optimization
- **Semantic caching** eliminates redundant API calls
- **Multi-provider routing** (OpenAI, Anthropic, Google, Mistral, Groq)
- **OpenAI-compatible API** - drop-in replacement
- **Enterprise features** - analytics, alerts, content policies

It's built by the same team (Blackman AI Technologies) and exists to provide a great cloud experience for users who want it, while funding continued open source development.

### Why Blackman AI Instead of Direct API Keys?

| Direct API Key | Via Blackman AI |
|----------------|-----------------|
| Full price per token | 15-30% savings via optimization |
| Manage multiple keys (OpenAI, Anthropic, etc.) | One key for all providers |
| No caching | Semantic cache reduces calls |
| No analytics | Full usage dashboard |
| DIY rate limit handling | Built-in rate limiting |

### Integration in Ted/Teddy

```
┌─────────────────────────────────────────┐
│  🤖 AI Provider                         │
│                                         │
│  ○ Local (Ollama)          [Default]    │
│    Free, private, runs on your machine  │
│    Recommended: qwen2.5-coder:7b        │
│                                         │
│  ○ Blackman AI             [Cloud]      │
│    Optimized cloud with all models      │
│    [Sign up at useblackman.ai]          │
│                                         │
│  ○ Bring Your Own Key                   │
│    Use your Anthropic/OpenAI keys       │
│    [Configure API Keys →]               │
│                                         │
└─────────────────────────────────────────┘
```

### Implementation (Phase 2)

Adding Blackman AI as a provider is straightforward since it's OpenAI-compatible:

```rust
// src/llm/providers/blackman.rs
pub struct BlackmanProvider {
    base_url: String,  // https://app.useblackman.ai/v1
    api_key: String,   // User's Blackman AI API key
}

// Reuses OpenAI message format - minimal new code
```

| Task | Priority | Effort | Notes |
|------|----------|--------|-------|
| Add `blackman` provider to Ted | P1 | 2 days | OpenAI-compatible, reuse existing code |
| Provider selection UI in Teddy | P1 | 1 day | Settings panel |
| Blackman AI signup flow | P2 | 2 days | In-app OAuth or link out |
| Usage analytics display | P3 | 2 days | Show savings in Teddy UI |

### The Relationship

```
Ted/Teddy (Open Source)          Blackman AI (Commercial)
        │                                │
        │  100% Free                     │  Paid service
        │  AGPL licensed                 │  Funds open source work
        │  Local-first                   │  Optional cloud provider
        │  Community project             │  One of many provider options
        │                                │
        └────────────────────────────────┘
                    │
        Users choose what works for them
        No lock-in, no dark patterns
```

**The deal**: Ted/Teddy stays completely free and functional without Blackman AI. Blackman AI is just one option among many (Ollama, direct API keys, OpenRouter, etc.). Revenue from Blackman AI helps fund continued open source development, but there's no artificial limitation of the free version to push people toward paid.

---

## Appendix A: Competitive Intelligence Sources

- [Lovable](https://lovable.dev/) - $6.6B valuation, $200M ARR
- [v0 by Vercel](https://v0.app/) - Agentic UI builder
- [Bolt.new](https://bolt.new/) - Browser-based full-stack builder
- [OpenCode](https://opencode.ai/) - Open source CLI agent

---

## Appendix B: Domain Assets

| Domain | Purpose | Status |
|--------|---------|--------|
| **teddy.rocks** | Primary share subdomain | Owned |
| **teddy.diy** | Alternative/future use | Owned |
| **teddy.technology** | Alternative/future use | Owned |

---

## Appendix C: The Grandma Test

The ultimate success metric: Can a non-technical person do this?

```
1. Download Teddy                           ← Must be one-click install
2. Open it, pick a folder                   ← No terminal, no config
3. Type "Make me a recipe blog"             ← Natural language
4. See it build and preview automatically   ← No "npm run dev"
5. Chat to make changes                     ← No code editing
6. Click Share, send link to family         ← No localhost confusion
7. Family loves it, click "Go Live"         ← No Vercel dashboard
8. Type desired domain, click Buy           ← No DNS configuration
9. Done. Real website. Grandma did it.      ← Victory
```

If any step requires technical knowledge, we've failed.

**Hardware requirement for Grandma Test**: Must work on a $150 refurbished 2010 Dell OptiPlex with 16GB RAM and SSD. If Grandma can't afford a new computer, she can use an old one.

---

## Appendix D: The 2010 Dell Benchmark - Our Commitment

### Why This Matters

Every other AI coding assistant assumes users have:
- Modern MacBook Pros ($2000+)
- High-speed internet for cloud APIs
- Money for subscriptions ($20-200/month)
- Technical knowledge to set up dev environments

**We refuse to make these assumptions.**

### The Commitment

1. **We will acquire and maintain a real 2010 Dell OptiPlex** as our benchmark test machine
2. **Every release will be tested on this machine** before shipping
3. **Performance regressions on ancient hardware block releases** just like security issues
4. **We will publish video evidence** of Teddy working on this hardware
5. **We will help users acquire and upgrade old hardware** with detailed guides

### The "$150 Coding PC" Build Guide (Future)

**Where to buy**:
- eBay, Craigslist, Facebook Marketplace
- Search: "Dell OptiPlex 2010-2015" or "HP EliteDesk 2010-2015"
- Target price: $50-100

**Required upgrades** (do-it-yourself friendly):
1. **RAM to 16GB** ($30-40 used DDR3)
   - YouTube tutorial: "How to upgrade Dell OptiPlex RAM"
   - Tools needed: Just your hands (no screws)
   - Difficulty: 1/10

2. **SSD 240GB** ($25-35)
   - YouTube tutorial: "Replace HDD with SSD in Dell OptiPlex"
   - Tools needed: Screwdriver
   - Difficulty: 2/10

**Total cost**: $105-175 for a fully functional AI coding computer

**What you can build on it**:
- Personal blogs and portfolios
- Small business websites
- To-do lists and productivity apps
- Recipe sites, photo galleries
- Simple e-commerce stores
- Learning projects

**What you CAN'T build** (upgrade needed):
- Complex enterprise software
- Real-time multiplayer games
- Large-scale data processing apps
- Anything requiring 7b+ models

### The Philosophy

This isn't about being cheap. It's about **accessibility**.

- Students in developing countries
- Retirees on fixed incomes
- People who lost everything and are rebuilding
- Teachers in underfunded schools
- Anyone who deserves a chance to create

If we can run on a $150 computer, we remove the hardware barrier entirely. The only limit becomes imagination.

### Inspiration: The Raspberry Pi Ethos

The Raspberry Pi Foundation proved that $35 computers could change education globally. We're doing the same for AI-assisted coding.

**Their philosophy**: "A computer for everyone, regardless of means."
**Our philosophy**: "AI coding assistance for everyone, regardless of hardware."

---

---

## Appendix E: The "$125 Raspberry Pi AI Coding Computer" (Future)

### Why Raspberry Pi Complements the 2010 Dell

The 2010 Dell proves we support **anyone with an old PC**.
The Raspberry Pi 5 proves we support **anyone, anywhere, with minimal hardware**.

**Different strengths**:

| Feature | 2010 Dell OptiPlex | Raspberry Pi 5 |
|---------|-------------------|----------------|
| **Cost** | $105-175 | $125 |
| **Performance** | Better (x86 optimized) | Good enough (ARM) |
| **Power draw** | 65-90W | 5-10W |
| **Size** | Desktop tower | Credit card |
| **Availability** | eBay/local | Global shipping |
| **Education use** | One per classroom | One per student |
| **Maker integration** | No GPIO | GPIO for robotics/IoT |
| **Always-on viability** | High power cost | Perfect (low power) |
| **International shipping** | Expensive/difficult | Easy/cheap |

### The Build Guide

**What to buy**:
1. **Raspberry Pi 5 8GB** - $80 ([raspberrypi.com](https://raspberrypi.com))
2. **NVMe HAT** - $15 (Argon ONE M.2, PiNVMe, Geekworm X1001, etc.)
3. **256GB NVMe SSD** - $30 (Any M.2 2280 drive)
4. **27W USB-C Power Supply** - Included or $12 official
5. **Active cooling** - Optional but recommended ($10-15)
6. **Case** - Optional, many NVMe HATs include cases

**Total**: $125-140 for a fully functional AI coding computer

**Why NVMe over microSD**:
- Model loading: 10-15 seconds vs 60-90 seconds
- File operations: Near-instant vs laggy
- Reliability: SSDs don't corrupt like SD cards
- Cost: Only $15 more for HAT + $30 for SSD

**Assembly** (30 minutes, no soldering):
1. Attach NVMe HAT to Pi 5 GPIO header
2. Install M.2 SSD into HAT
3. Flash Raspberry Pi OS to SSD
4. Boot and install Ollama
5. Install Teddy

### Education Use Cases

**Why classrooms love Raspberry Pi**:
- Many schools already have Pi 5s for teaching
- Students can take them home (portable)
- Low power = safe to leave running
- GPIO integration for robotics projects
- Teach "AI + hardware" in one device

**Example curriculum integration**:
```
Week 1-2: Build a personal website with Teddy
Week 3-4: Add LED status lights via GPIO (build in progress = blink)
Week 5-6: Create a temperature sensor dashboard
Week 7-8: Build a smart home control panel
```

**The magic**: Same hardware teaches web development AND physical computing.

### What You Can Build

**Perfect for**:
- Learning to code (Python, JavaScript, HTML/CSS)
- Personal websites and blogs
- School projects
- Maker projects with UI (weather station, smart mirror, etc.)
- Always-on local tools (home automation dashboard)
- Offline documentation sites

**Not ideal for**:
- Professional development work (upgrade to Ancient/Tiny tier)
- Complex multi-page apps (need more RAM/CPU)
- Large codebases (context too limited)

### The Developing Country Angle

**Shipping a desktop PC internationally**: $100-300 in shipping
**Shipping a Raspberry Pi 5**: $10-30

For students in developing countries:
- Pi 5 is easier to import (small, light)
- Lower customs duties (cheaper declared value)
- Can run on solar power (5-10W)
- Works with any HDMI monitor/TV
- Can be powered by laptop USB-C charger

### Performance Expectations

**With qwen2.5-coder:1.5b (Q3_K_M quantization)**:
- Token generation: 5-15 tokens/sec
- Simple request: 15-40 seconds
- Complex request: 40-90 seconds
- Memory usage: 3-4GB (plenty of headroom)

**UX adaptations**:
```
🎓 Raspberry Pi Detected (Education Mode)

Your Pi can build: Simple apps, learning projects, maker tools
Expected AI response: 20-40 seconds
Pro tip: While waiting, review your code or plan your next feature!

[Why is it slow?] [Upgrade to faster hardware ($150 Dell)]
```

### The Marketing Story

**2010 Dell**: "We support the hardware you already have"
**Raspberry Pi 5**: "We support the hardware educators trust"

Together: **"AI coding for everyone, on any budget, anywhere in the world"**

---

*This roadmap is a living document. Update as priorities shift.*

*Built with love and spite for the VC-industrial complex.*

*Tested on a 2010 Dell OptiPlex and Raspberry Pi 5 because everyone deserves to build software.*
