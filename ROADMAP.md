# Ted & Teddy - Product Roadmap

**Last Updated**: 2026-01-15
**Vision**: A fully local, offline-first AI coding environment that adapts to any hardware
**Mission**: Disrupt billion-dollar companies for free. Make app building accessible to everyone—including grandma.

---

## Part 1: Current State - Feature Inventory

### Ted CLI - What's Built

| Category | Feature | Status | Notes |
|----------|---------|--------|-------|
| **LLM Providers** | Anthropic Claude | ✅ Complete | claude-sonnet-4, claude-3.5-sonnet, claude-3.5-haiku |
| | Ollama (Local) | ✅ Complete | qwen2.5-coder, llama3.2, deepseek-coder, etc. |
| | OpenAI | ❌ Skeleton | Not implemented |
| | Google Gemini | ❌ Skeleton | Not implemented |
| | OpenRouter | ❌ Not started | Would enable 100+ models |
| **Tools** | file_read | ✅ Complete | Line offset/limit, truncation |
| | file_write | ✅ Complete | Create new files, auto-mkdir |
| | file_edit | ✅ Complete | Find/replace with uniqueness check |
| | shell | ✅ Complete | Timeout, dangerous command blocking |
| | glob | ✅ Complete | File pattern matching |
| | grep | ✅ Complete | Regex search with context |
| | plan_update | ✅ Complete | Task tracking |
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
| | Preview iframe | 🔶 Partial | Manual URL only |
| **Ted Integration** | Subprocess spawning | ✅ Complete | TedRunner |
| | JSONL parsing | ✅ Complete | TedParser |
| | File operations | ✅ Complete | FileApplier |
| | Git auto-commit | ✅ Complete | AutoCommit |
| | Multi-turn chat | ✅ Complete | Conversation history |
| **Planned Features** | Docker support | ❌ Stubbed | Button exists, not functional |
| | PostgreSQL | ❌ Stubbed | Button exists, not functional |
| | Deploy integration | ❌ Stubbed | Button exists, not functional |
| | Dev server detection | ❌ Not started | Manual URL required |
| | Diff view | ❌ Not started | No change review |
| | Settings UI | ❌ Not started | Runtime options only |

---

## Part 2: Competitive Comparison

### Ted vs OpenCode (CLI Agents)

| Feature | Ted | OpenCode | Winner |
|---------|-----|----------|--------|
| **Language** | Rust | Go | Tie |
| **LLM Providers** | 2 (Anthropic, Ollama) | 75+ (via Models.dev) | OpenCode |
| **Local Models** | ✅ Ollama | ✅ Multiple | Tie |
| **Customization** | Caps (composable) | Agents (single mode) | **Ted** |
| **Stack Multiple Personas** | ✅ Yes | ❌ No | **Ted** |
| **Tool Permissions** | Per-cap granular | Per-agent | **Ted** |
| **Context Memory** | WAL + smart indexer | Unknown | **Ted** |
| **MCP Protocol** | ❌ No | ✅ Yes | OpenCode |
| **LSP Integration** | ❌ No | ✅ Yes | OpenCode |
| **GitHub Actions** | ❌ No | ✅ /opencode mentions | OpenCode |
| **IDE Extension** | ❌ No | ✅ Yes | OpenCode |
| **GUI Companion** | ✅ Teddy | ❌ No | **Ted** |
| **License** | AGPL-3.0 | MIT | Preference |
| **Multi-session** | ❌ No | ✅ Yes | OpenCode |
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
| **Database Setup** | ❌ Not yet | ✅ Auto Supabase | ❌ No | ✅ Auto Supabase |
| **Deployment** | ❌ Not yet | ✅ One-click | ✅ Vercel native | ✅ Netlify native |
| **Tech Stack** | React + any | React + Tailwind | React + Next.js + Tailwind | React + Tailwind |
| **Agent Mode** | ✅ Yes (Ted) | ✅ Yes (autonomous) | ✅ Yes (agentic) | 🔶 Discussion mode |
| **Git Integration** | ✅ Auto-commit | ✅ GitHub sync | ✅ GitHub | ❌ Manual export |
| **Visual Editing** | ❌ Monaco only | ✅ Click-to-edit | 🔶 Limited | 🔶 Limited |
| **Figma Import** | ❌ No | ❌ No | ❌ No | ✅ Yes |
| **Code Ownership** | ✅ Full | ✅ Full | ✅ Full | ✅ Full |
| **Model Selection** | ✅ Any (Ollama) | ❌ Fixed | ❌ Claude only | ❌ Fixed |
| **Hardware Adaptive** | 🔶 Planned | ❌ N/A | ❌ N/A | ❌ N/A |

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

### System Detection Tiers

| Tier | RAM | VRAM | CPU | Recommended Models |
|------|-----|------|-----|-------------------|
| **Tiny** | ≤8GB | None | 4 cores | qwen2.5-coder:1.5b, phi-3-mini |
| **Small** | 8-16GB | ≤4GB | 4-8 cores | qwen2.5-coder:3b, codellama:7b |
| **Medium** | 16-32GB | 4-8GB | 8+ cores | qwen2.5-coder:7b, deepseek-coder:6.7b |
| **Large** | 32GB+ | 8-16GB | 8+ cores | qwen2.5-coder:14b, codellama:34b |
| **Cloud** | Any | Any | Any | claude-sonnet-4, gpt-4o |

### Adaptive Behavior

| Tier | Max Context | Response Strategy | Cap Adjustments |
|------|-------------|-------------------|-----------------|
| **Tiny** | 2K tokens | More directive prompts, step-by-step | Simplified caps, more examples |
| **Small** | 4K tokens | Focused context, single file | Standard caps |
| **Medium** | 8K tokens | Multi-file context | Full caps |
| **Large** | 16K+ tokens | Full project context | All features |
| **Cloud** | 100K+ tokens | Unlimited context | Maximum capability |

### Implementation Plan

```rust
// src/hardware/detector.rs
pub struct SystemProfile {
    pub ram_gb: u32,
    pub vram_gb: Option<u32>,
    pub cpu_cores: u32,
    pub tier: HardwareTier,
}

pub enum HardwareTier {
    Tiny,   // Chromebook, old laptops
    Small,  // Entry MacBook Air, basic laptops
    Medium, // MacBook Pro M1/M2, gaming laptops
    Large,  // Pro workstations, Mac Studio
    Cloud,  // Using API providers
}

impl SystemProfile {
    pub fn detect() -> Self { ... }
    pub fn recommended_model(&self) -> &str { ... }
    pub fn max_context_tokens(&self) -> usize { ... }
    pub fn should_use_streaming(&self) -> bool { ... }
}
```

### Guardrails for Small Systems

1. **Automatic model selection** - Detect hardware, suggest appropriate model
2. **Context limiting** - Reduce context window on constrained systems
3. **Prompt optimization** - More directive, structured prompts for smaller models
4. **Streaming always** - Never buffer full responses on low-RAM systems
5. **Graceful degradation** - If model too slow, suggest smaller alternative
6. **Memory monitoring** - Warn before OOM situations

---

## Part 5: Roadmap

### Phase 1: Foundation (Current → 2 weeks)

**Goal**: Production-ready local experience

| Task | Priority | Effort | Notes |
|------|----------|--------|-------|
| End-to-end Teddy testing | P0 | 2 days | Verify full agent loop works |
| Hardware detection module | P0 | 3 days | Detect RAM/VRAM/CPU |
| Model recommendation engine | P0 | 2 days | Map hardware → models |
| Context limiting by tier | P1 | 2 days | Reduce context for small systems |
| Fix Teddy preview (dev server detection) | P1 | 2 days | Auto-detect Vite/Next.js |
| Add settings UI to Teddy | P2 | 3 days | Provider/model selection |

### Phase 2: Provider Expansion (Weeks 3-4)

**Goal**: Support more models and providers

| Task | Priority | Effort | Notes |
|------|----------|--------|-------|
| OpenRouter integration | P0 | 3 days | 100+ models via single API |
| OpenAI provider | P1 | 2 days | GPT-4o, GPT-4o-mini |
| Google Gemini provider | P1 | 2 days | Gemini Pro, Flash |
| Provider auto-selection | P1 | 2 days | Based on hardware + availability |
| Model speed benchmarking | P2 | 2 days | Track tokens/sec per model |

### Phase 3: Teddy Polish (Weeks 5-6)

**Goal**: Feature parity with competitors on core UX

| Task | Priority | Effort | Notes |
|------|----------|--------|-------|
| Diff view before apply | P0 | 4 days | Review AI changes |
| Undo/redo for file ops | P1 | 3 days | Beyond just Git |
| File tree search | P1 | 2 days | Find files quickly |
| Multi-file selection | P2 | 2 days | Bulk operations |
| Dark/light theme toggle | P2 | 1 day | User preference |
| Keyboard shortcuts | P2 | 2 days | Power user UX |

### Phase 4: Backend & Deploy (Weeks 7-10)

**Goal**: Full-stack app building capability

| Task | Priority | Effort | Notes |
|------|----------|--------|-------|
| Supabase integration | P0 | 5 days | Auth + database |
| Docker detection & management | P1 | 4 days | Container runtime |
| PostgreSQL local setup | P1 | 3 days | Via Docker |
| Vercel deploy | P1 | 3 days | One-click deploy |
| Netlify deploy | P2 | 2 days | Alternative deploy |
| Environment variable management | P2 | 2 days | Secrets handling |

### Phase 5: Share & Deploy (Weeks 11-14)

**Goal**: Grandma can build an app and share it with the world

The complete flow:
```
1. "Create a blog about home gardening"
2. Teddy builds it locally, auto-starts preview
3. Chat to refine: "Make the header green"
4. Click "Share" → Instant public URL via tunnel
5. Send link to friends (no more "localhost:3000" mistakes!)
6. Click "Go Live" → Pick/buy domain → Deployed
```

| Task | Priority | Effort | Notes |
|------|----------|--------|-------|
| Dev server auto-detection | P0 | 2 days | Detect Vite/Next.js/etc, auto-start |
| Cloudflare Tunnel integration | P0 | 3 days | Embed cloudflared binary, manage tunnels |
| Share Link UI | P0 | 2 days | One-click share, copy link, QR code |
| teddy.rocks subdomain service | P1 | 4 days | Cloudflare Workers + KV routing |
| Permanent free hosting | P1 | 3 days | Static deploy to CF Pages (free tier) |
| Vercel deploy integration | P1 | 3 days | OAuth + deploy API |
| Netlify deploy integration | P2 | 2 days | Alternative option |
| Cloudflare Pages deploy | P2 | 2 days | Another free option |
| Domain availability check | P2 | 2 days | Cloudflare Registrar API |
| Domain purchase flow | P3 | 4 days | Buy + auto-configure DNS (affiliate) |

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

#### Share UI Flow

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

### Phase 6: Advanced Features (Weeks 15-20)

**Goal**: Differentiated capabilities

| Task | Priority | Effort | Notes |
|------|----------|--------|-------|
| MCP protocol support | P1 | 5 days | External tool ecosystem |
| Multi-agent orchestration | P2 | 1 week | Parallel agents |
| Collaborative mode | P2 | 1 week | Multiple users |
| Plugin system | P2 | 1 week | User extensions |
| ted_core library extraction | P3 | 2 weeks | Eliminate subprocess overhead |

### Phase 7: Distribution (Ongoing)

**Goal**: Easy installation and updates

| Task | Priority | Effort | Notes |
|------|----------|--------|-------|
| macOS DMG packaging | P0 | 2 days | Signed + notarized |
| Windows installer | P0 | 2 days | MSI/NSIS |
| Linux AppImage | P1 | 1 day | Universal Linux |
| Auto-update for Teddy | P1 | 3 days | Electron auto-updater |
| Homebrew formula | P2 | 1 day | `brew install ted` |

---

## Part 6: Success Metrics

### Short-term (1 month)
- [ ] Teddy works end-to-end on all 3 platforms
- [ ] Hardware detection correctly identifies 90% of systems
- [ ] Model recommendations feel appropriate to users
- [ ] 50+ GitHub stars

### Medium-term (3 months)
- [ ] 5+ LLM providers supported
- [ ] Supabase integration working
- [ ] One-click deploy to Vercel
- [ ] 500+ GitHub stars
- [ ] 100+ active users

### Long-term (6 months)
- [ ] Feature parity with Lovable core features
- [ ] MCP ecosystem compatibility
- [ ] 2000+ GitHub stars
- [ ] Community-contributed caps
- [ ] Self-sustaining open source project

---

## Part 7: Technical Debt to Address

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

## Part 8: Sustainability Model

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

## Part 9: Blackman AI Integration (Optional Cloud Provider)

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

---

*This roadmap is a living document. Update as priorities shift.*

*Built with love and spite for the VC-industrial complex.*
