# Plan: Self-Contained Ted Binary

## Goal
Make Ted a single downloadable binary that requires no external dependencies (no Ollama, no separate model servers) while maintaining full functionality for local LLM inference and semantic search.

---

## Current Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                         Ted (Current)                        │
├─────────────────────────────────────────────────────────────┤
│  Embeddings: Ollama (nomic-embed-text)                      │
│  - Requires Ollama running on localhost:11434               │
│  - Auto-pulls model if missing                              │
├─────────────────────────────────────────────────────────────┤
│  Local LLM: Ollama Provider                                 │
│  - External dependency: ollama server                       │
│  - User must install and run Ollama separately              │
├─────────────────────────────────────────────────────────────┤
│  Cloud: Anthropic, OpenRouter, Blackman                     │
└─────────────────────────────────────────────────────────────┘
```

## Target Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Ted (Self-Contained)                      │
├─────────────────────────────────────────────────────────────┤
│  Embeddings: fastembed-rs (bundled)                         │
│  - all-MiniLM-L6-v2 (~25MB) - default, fast                │
│  - nomic-embed-text-v1.5 (~130MB) - optional, better        │
│  - Model downloaded on first use to ~/.ted/models/          │
├─────────────────────────────────────────────────────────────┤
│  Local LLM: llama-cpp-2 (bundled)                           │
│  - CPU inference (AVX2/AVX512 auto-detect)                  │
│  - Metal acceleration on macOS (auto-detect)                │
│  - CUDA acceleration (optional feature flag)                │
│  - User downloads GGUF models to ~/.ted/models/             │
├─────────────────────────────────────────────────────────────┤
│  Cloud: Anthropic, OpenRouter, Blackman (unchanged)         │
├─────────────────────────────────────────────────────────────┤
│  Legacy: Ollama Provider (kept for backwards compat)        │
│  - Still works if user has Ollama installed                 │
└─────────────────────────────────────────────────────────────┘
```

---

## Phase 1: Bundled Embeddings (fastembed-rs)

### 1.1 Add fastembed-rs Dependency

**File**: `Cargo.toml`

```toml
[dependencies]
fastembed = "4"  # Latest stable version

[features]
default = ["bundled-embeddings"]
bundled-embeddings = ["fastembed"]
```

### 1.2 Create Bundled Embedding Provider

**New File**: `src/embeddings/bundled.rs`

```rust
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::path::PathBuf;

pub struct BundledEmbeddings {
    model: TextEmbedding,
    model_name: String,
}

impl BundledEmbeddings {
    pub fn new(model_name: &str, cache_dir: PathBuf) -> Result<Self> {
        // Models: AllMiniLML6V2, NomicEmbedTextV15, etc.
        let model_type = match model_name {
            "all-minilm-l6-v2" => EmbeddingModel::AllMiniLML6V2,
            "nomic-embed-text-v1.5" => EmbeddingModel::NomicEmbedTextV15,
            _ => EmbeddingModel::AllMiniLML6V2, // default
        };

        let model = TextEmbedding::try_new(InitOptions {
            model_name: model_type,
            cache_dir: Some(cache_dir),
            show_download_progress: true,
            ..Default::default()
        })?;

        Ok(Self { model, model_name: model_name.to_string() })
    }

    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.model.embed(vec![text], None)?;
        Ok(embeddings.into_iter().next().unwrap())
    }

    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.model.embed(texts.to_vec(), None)
    }

    pub fn dimension(&self) -> usize {
        match self.model_name.as_str() {
            "all-minilm-l6-v2" => 384,
            "nomic-embed-text-v1.5" => 768,
            _ => 384,
        }
    }
}
```

### 1.3 Update EmbeddingGenerator to Support Both Backends

**File**: `src/embeddings/mod.rs`

```rust
pub enum EmbeddingBackend {
    Bundled(BundledEmbeddings),
    Ollama(OllamaEmbeddings),  // Keep for backwards compatibility
}

pub struct EmbeddingGenerator {
    backend: EmbeddingBackend,
}

impl EmbeddingGenerator {
    /// Create with bundled fastembed (default, no external deps)
    pub fn bundled(model: &str, cache_dir: PathBuf) -> Result<Self> {
        Ok(Self {
            backend: EmbeddingBackend::Bundled(BundledEmbeddings::new(model, cache_dir)?),
        })
    }

    /// Create with Ollama backend (requires running Ollama)
    pub fn ollama(config: EmbeddingConfig) -> Self {
        Self {
            backend: EmbeddingBackend::Ollama(OllamaEmbeddings::new(config)),
        }
    }

    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        match &self.backend {
            EmbeddingBackend::Bundled(b) => b.embed(text),
            EmbeddingBackend::Ollama(o) => o.embed(text).await,
        }
    }
}
```

### 1.4 Update Settings for Embedding Backend Choice

**File**: `src/settings.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingSettings {
    /// Backend: "bundled" (default) or "ollama"
    pub backend: String,
    /// Model name (backend-specific)
    pub model: String,
}

impl Default for EmbeddingSettings {
    fn default() -> Self {
        Self {
            backend: "bundled".to_string(),
            model: "all-minilm-l6-v2".to_string(),
        }
    }
}
```

---

## Phase 2: Bundled LLM Inference (llama-cpp-2)

### 2.1 Add llama-cpp-2 Dependency

**File**: `Cargo.toml`

```toml
[dependencies]
llama-cpp-2 = { version = "0.1", optional = true }

[features]
default = ["bundled-embeddings", "local-llm"]
local-llm = ["llama-cpp-2"]
local-llm-cuda = ["llama-cpp-2/cuda"]  # Optional CUDA support
local-llm-metal = ["llama-cpp-2/metal"]  # Auto-enabled on macOS
```

### 2.2 Create LlamaCpp Provider

**New File**: `src/llm/providers/llama_cpp.rs`

```rust
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use std::path::PathBuf;

pub struct LlamaCppProvider {
    backend: LlamaBackend,
    model: Option<LlamaModel>,
    model_path: Option<PathBuf>,
    context_size: u32,
    gpu_layers: i32,
}

impl LlamaCppProvider {
    pub fn new(models_dir: PathBuf) -> Result<Self> {
        let backend = LlamaBackend::init()?;

        Ok(Self {
            backend,
            model: None,
            model_path: None,
            context_size: 8192,
            gpu_layers: -1,  // Auto-detect (all on GPU if available)
        })
    }

    pub fn load_model(&mut self, model_path: PathBuf) -> Result<()> {
        let params = LlamaModelParams::default()
            .with_n_gpu_layers(self.gpu_layers);

        let model = LlamaModel::load_from_file(&self.backend, &model_path, &params)?;

        self.model = Some(model);
        self.model_path = Some(model_path);
        Ok(())
    }

    fn available_models(&self, models_dir: &Path) -> Vec<ModelInfo> {
        // Scan ~/.ted/models/ for .gguf files
        std::fs::read_dir(models_dir)
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension() == Some("gguf".as_ref()))
                    .map(|e| ModelInfo {
                        name: e.file_name().to_string_lossy().to_string(),
                        context_length: 8192,  // Could parse from filename
                        supports_tools: true,
                        supports_vision: false,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[async_trait]
impl LlmProvider for LlamaCppProvider {
    fn name(&self) -> &str {
        "llamacpp"
    }

    fn available_models(&self) -> Vec<ModelInfo> {
        self.available_models(&self.models_dir)
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let model = self.model.as_ref()
            .ok_or_else(|| anyhow!("No model loaded"))?;

        // Convert messages to prompt format
        let prompt = self.format_messages(&request.messages)?;

        // Create context and generate
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(self.context_size);

        let mut ctx = model.new_context(&self.backend, ctx_params)?;

        // Tokenize and generate...
        // (Implementation details)
    }

    async fn complete_stream(&self, request: CompletionRequest)
        -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>> {
        // Streaming implementation using async channels
    }
}
```

### 2.3 Model Management Commands

**New File**: `src/chat/commands/model.rs`

Add commands for model management:

```rust
/// /model list - List available local models
/// /model download <url> - Download a GGUF model
/// /model load <name> - Load a specific model
/// /model info - Show current model info

pub async fn handle_model_command(args: &str, session: &mut ChatSession) -> Result<String> {
    let parts: Vec<&str> = args.split_whitespace().collect();

    match parts.get(0).map(|s| *s) {
        Some("list") => list_models(session).await,
        Some("download") => download_model(parts.get(1), session).await,
        Some("load") => load_model(parts.get(1), session).await,
        Some("info") => model_info(session).await,
        _ => Ok("Usage: /model [list|download|load|info]".to_string()),
    }
}

async fn download_model(url: Option<&&str>, session: &ChatSession) -> Result<String> {
    let url = url.ok_or_else(|| anyhow!("Usage: /model download <url>"))?;

    // Download with progress bar to ~/.ted/models/
    let models_dir = dirs::home_dir()
        .ok_or_else(|| anyhow!("Cannot find home directory"))?
        .join(".ted")
        .join("models");

    std::fs::create_dir_all(&models_dir)?;

    // Use reqwest with progress callback
    // ...

    Ok(format!("Downloaded model to {:?}", models_dir))
}
```

### 2.4 Update Provider Factory

**File**: `src/llm/factory.rs`

```rust
impl ProviderFactory {
    pub async fn create(
        provider_name: &str,
        settings: &Settings,
        perform_health_check: bool,
    ) -> Result<Arc<dyn LlmProvider>> {
        match provider_name {
            "anthropic" => Self::create_anthropic(settings).await,
            "ollama" => Self::create_ollama(settings, perform_health_check).await,
            "openrouter" => Self::create_openrouter(settings).await,
            "blackman" => Self::create_blackman(settings).await,
            "local" | "llamacpp" => Self::create_llamacpp(settings).await,  // NEW
            _ => Err(anyhow!("Unknown provider: {}", provider_name)),
        }
    }

    async fn create_llamacpp(settings: &Settings) -> Result<Arc<dyn LlmProvider>> {
        let models_dir = dirs::home_dir()
            .ok_or_else(|| anyhow!("Cannot find home directory"))?
            .join(".ted")
            .join("models");

        let mut provider = LlamaCppProvider::new(models_dir)?;

        // Load default model if configured
        if let Some(model_path) = &settings.local.default_model {
            provider.load_model(PathBuf::from(model_path))?;
        }

        Ok(Arc::new(provider))
    }
}
```

---

## Phase 3: Semantic Search Integration

### 3.1 Add Vector Index to Indexer

**New File**: `src/indexer/vector.rs`

```rust
use std::collections::HashMap;

/// In-memory vector index with HNSW-like approximate nearest neighbor search
pub struct VectorIndex {
    vectors: HashMap<Uuid, Vec<f32>>,
    dimension: usize,
}

impl VectorIndex {
    pub fn new(dimension: usize) -> Self {
        Self {
            vectors: HashMap::new(),
            dimension,
        }
    }

    pub fn insert(&mut self, id: Uuid, vector: Vec<f32>) {
        assert_eq!(vector.len(), self.dimension);
        self.vectors.insert(id, vector);
    }

    pub fn search(&self, query: &[f32], k: usize) -> Vec<(Uuid, f32)> {
        // Brute force for now, can optimize with HNSW later
        let mut scores: Vec<_> = self.vectors.iter()
            .map(|(id, vec)| (*id, cosine_similarity(query, vec)))
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        scores.truncate(k);
        scores
    }

    pub fn remove(&mut self, id: &Uuid) {
        self.vectors.remove(id);
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 { 0.0 } else { dot / (norm_a * norm_b) }
}
```

### 3.2 Integrate Embeddings into Indexer

**File**: `src/indexer/mod.rs` (modifications)

```rust
pub struct Indexer {
    // ... existing fields ...

    /// Vector index for semantic search
    vector_index: VectorIndex,

    /// Embedding generator
    embeddings: EmbeddingGenerator,
}

impl Indexer {
    pub async fn new(root: PathBuf, config: IndexerConfig) -> Result<Self> {
        // Initialize bundled embeddings by default
        let cache_dir = dirs::home_dir()
            .ok_or_else(|| anyhow!("Cannot find home directory"))?
            .join(".ted")
            .join("models");

        let embeddings = EmbeddingGenerator::bundled(
            &config.embedding_model,
            cache_dir,
        )?;

        let dimension = embeddings.dimension();
        let vector_index = VectorIndex::new(dimension);

        // ... rest of initialization ...
    }

    /// Index a chunk with its embedding
    pub async fn index_chunk(&mut self, chunk: &CodeChunk) -> Result<()> {
        // Generate embedding for chunk content
        let embedding = self.embeddings.embed(&chunk.content).await?;

        // Store in vector index
        self.vector_index.insert(chunk.id, embedding);

        // ... existing chunk indexing logic ...
        Ok(())
    }

    /// Semantic search for relevant chunks
    pub async fn semantic_search(&self, query: &str, k: usize) -> Result<Vec<(Uuid, f32)>> {
        let query_embedding = self.embeddings.embed(query).await?;
        Ok(self.vector_index.search(&query_embedding, k))
    }

    /// Hybrid search: combine semantic + keyword + memory scoring
    pub async fn hybrid_search(
        &self,
        query: &str,
        k: usize,
    ) -> Result<Vec<SearchResult>> {
        // 1. Semantic search
        let semantic_results = self.semantic_search(query, k * 2).await?;

        // 2. Keyword search (existing grep-like functionality)
        let keyword_results = self.keyword_search(query, k * 2)?;

        // 3. Combine with RRF (Reciprocal Rank Fusion)
        let mut scores: HashMap<Uuid, f32> = HashMap::new();

        for (rank, (id, _score)) in semantic_results.iter().enumerate() {
            *scores.entry(*id).or_default() += 1.0 / (60.0 + rank as f32);
        }

        for (rank, (id, _score)) in keyword_results.iter().enumerate() {
            *scores.entry(*id).or_default() += 1.0 / (60.0 + rank as f32);
        }

        // 4. Boost by memory retention score
        for (id, score) in scores.iter_mut() {
            if let Some(chunk) = self.get_chunk(id) {
                if let Some(memory) = self.get_chunk_memory(id) {
                    // Boost recently/frequently accessed chunks
                    let retention = self.scorer.chunk_retention_score(memory);
                    *score *= 1.0 + retention * 0.5;
                }
            }
        }

        // 5. Sort and return top k
        let mut results: Vec<_> = scores.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        results.truncate(k);

        Ok(results.into_iter()
            .filter_map(|(id, score)| {
                self.get_chunk(&id).map(|chunk| SearchResult { chunk, score })
            })
            .collect())
    }
}
```

### 3.3 Automatic Context Injection

**File**: `src/context/manager.rs` (modifications)

```rust
impl ContextManager {
    /// Build context for a query using hybrid search
    pub async fn build_context(
        &mut self,
        query: &str,
        max_tokens: usize,
    ) -> Result<String> {
        // Semantic search for relevant chunks
        let results = self.indexer.hybrid_search(query, 20).await?;

        let mut context = String::new();
        let mut token_count = 0;

        for result in results {
            let chunk_tokens = self.count_tokens(&result.chunk.content);
            if token_count + chunk_tokens > max_tokens {
                break;
            }

            context.push_str(&format!(
                "// File: {:?}:{}-{}\n{}\n\n",
                result.chunk.source.file,
                result.chunk.source.start_line,
                result.chunk.source.end_line,
                result.chunk.content,
            ));

            token_count += chunk_tokens;

            // Record access for memory scoring
            self.indexer.record_chunk_access(&result.chunk.id).await?;
        }

        Ok(context)
    }
}
```

---

## Phase 4: Model Registry & Distribution

### 4.1 Registry Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Ted Binary                                                  │
│  - Embedded fallback model list (works fully offline)       │
│  - Fetches registry on `/model list` or `/model download`   │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  Registry: https://ted.dev/models.json                       │
│  (Hosted on GitHub Pages - free, reliable)                   │
│                                                              │
│  {                                                           │
│    "version": 2,                                             │
│    "updated": "2025-01-15T00:00:00Z",                       │
│    "models": [                                               │
│      {                                                       │
│        "id": "qwen2.5-coder-7b-q4",                         │
│        "name": "Qwen 2.5 Coder 7B",                         │
│        "type": "chat",                                       │
│        "size_bytes": 4718592000,                            │
│        "quantization": "Q4_K_M",                            │
│        "context_length": 32768,                             │
│        "url": "https://huggingface.co/Qwen/...",            │
│        "sha256": "abc123...",                               │
│        "recommended": true,                                  │
│        "tags": ["code", "instruct"]                         │
│      }                                                       │
│    ]                                                         │
│  }                                                           │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  HuggingFace (actual model file hosting)                     │
│  - GGUF files downloaded directly from HF                   │
│  - Free, reliable, zero bandwidth cost for us               │
│  - Ted verifies SHA256 after download                       │
└─────────────────────────────────────────────────────────────┘
```

**Benefits**:
- Zero hosting costs for large model files
- Update recommended models without releasing new Ted versions
- Embedded fallback ensures fully offline scenarios work
- SHA256 checksums for integrity verification
- Can add new models, mark deprecated ones, add warnings

### 4.2 Registry Schema

**New File**: `registry/models.json` (in ted repo, served via GitHub Pages)

```json
{
  "version": 2,
  "updated": "2025-01-15T00:00:00Z",
  "embedding_models": [
    {
      "id": "all-minilm-l6-v2",
      "name": "MiniLM L6 v2",
      "provider": "fastembed",
      "dimension": 384,
      "size_bytes": 26214400,
      "description": "Fast, lightweight embeddings - recommended for most users",
      "recommended": true
    },
    {
      "id": "nomic-embed-text-v1.5",
      "name": "Nomic Embed Text v1.5",
      "provider": "fastembed",
      "dimension": 768,
      "size_bytes": 136314880,
      "description": "Higher quality embeddings, better for large codebases"
    }
  ],
  "chat_models": [
    {
      "id": "qwen2.5-coder-7b-q4",
      "name": "Qwen 2.5 Coder 7B (Q4_K_M)",
      "size_bytes": 4718592000,
      "context_length": 32768,
      "url": "https://huggingface.co/Qwen/Qwen2.5-Coder-7B-Instruct-GGUF/resolve/main/qwen2.5-coder-7b-instruct-q4_k_m.gguf",
      "sha256": "...",
      "description": "Excellent code model, best balance of quality and speed",
      "recommended": true,
      "tags": ["code", "instruct", "tool-use"]
    },
    {
      "id": "deepseek-coder-v2-lite-q4",
      "name": "DeepSeek Coder V2 Lite (Q4_K_M)",
      "size_bytes": 9437184000,
      "context_length": 163840,
      "url": "https://huggingface.co/...",
      "sha256": "...",
      "description": "Larger context window, good for big files",
      "tags": ["code", "instruct", "long-context"]
    },
    {
      "id": "llama-3.2-3b-q4",
      "name": "Llama 3.2 3B (Q4_K_M)",
      "size_bytes": 2097152000,
      "context_length": 8192,
      "url": "https://huggingface.co/...",
      "sha256": "...",
      "description": "Small and fast, good for quick tasks on limited hardware",
      "tags": ["general", "instruct", "lightweight"]
    }
  ]
}
```

### 4.3 Registry Client Implementation

**New File**: `src/models/registry.rs`

```rust
use serde::{Deserialize, Serialize};

const REGISTRY_URL: &str = "https://ted.dev/models.json";
const REGISTRY_CACHE_DURATION: Duration = Duration::from_secs(3600); // 1 hour

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRegistry {
    pub version: u32,
    pub updated: String,
    pub embedding_models: Vec<EmbeddingModelEntry>,
    pub chat_models: Vec<ChatModelEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatModelEntry {
    pub id: String,
    pub name: String,
    pub size_bytes: u64,
    pub context_length: u32,
    pub url: String,
    pub sha256: String,
    pub description: String,
    #[serde(default)]
    pub recommended: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub deprecated: bool,
    pub deprecated_message: Option<String>,
}

impl ModelRegistry {
    /// Fetch registry from remote, with local cache
    pub async fn fetch() -> Result<Self> {
        // Check cache first
        let cache_path = Self::cache_path()?;
        if let Ok(cached) = Self::load_cached(&cache_path) {
            if cached.is_fresh() {
                return Ok(cached.registry);
            }
        }

        // Fetch from remote
        match Self::fetch_remote().await {
            Ok(registry) => {
                // Cache for next time
                let _ = registry.save_cache(&cache_path);
                Ok(registry)
            }
            Err(e) => {
                // Fall back to embedded registry
                tracing::warn!("Failed to fetch registry: {}, using embedded fallback", e);
                Ok(Self::embedded())
            }
        }
    }

    /// Embedded fallback registry (compiled into binary)
    pub fn embedded() -> Self {
        serde_json::from_str(include_str!("../registry/fallback.json"))
            .expect("embedded registry should be valid")
    }

    /// Get recommended models
    pub fn recommended_chat_models(&self) -> Vec<&ChatModelEntry> {
        self.chat_models.iter()
            .filter(|m| m.recommended && !m.deprecated)
            .collect()
    }
}
```

### 4.4 Model Download with Progress & Verification

```rust
pub async fn download_model(entry: &ChatModelEntry, models_dir: &Path) -> Result<PathBuf> {
    let filename = entry.url.rsplit('/').next()
        .ok_or_else(|| anyhow!("Invalid URL"))?;
    let dest_path = models_dir.join(filename);

    // Check if already downloaded and valid
    if dest_path.exists() {
        if verify_sha256(&dest_path, &entry.sha256)? {
            return Ok(dest_path);
        }
        tracing::warn!("Existing file failed checksum, re-downloading");
    }

    // Download with progress
    let client = reqwest::Client::new();
    let response = client.get(&entry.url).send().await?;
    let total_size = response.content_length()
        .ok_or_else(|| anyhow!("Unknown content length"))?;

    let pb = ProgressBar::new(total_size);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")?);

    let mut file = File::create(&dest_path)?;
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        pb.set_position(downloaded);
    }

    pb.finish_with_message("Downloaded");

    // Verify checksum
    if !verify_sha256(&dest_path, &entry.sha256)? {
        std::fs::remove_file(&dest_path)?;
        return Err(anyhow!("Checksum verification failed"));
    }

    Ok(dest_path)
}
```

### 4.5 Embedded Fallback Registry

The binary includes a fallback registry so Ted works fully offline:

**New File**: `src/models/fallback.json` (compiled into binary via `include_str!`)

```json
{
  "version": 1,
  "updated": "2025-01-01T00:00:00Z",
  "embedding_models": [
    {
      "id": "all-minilm-l6-v2",
      "name": "MiniLM L6 v2",
      "provider": "fastembed",
      "dimension": 384,
      "size_bytes": 26214400,
      "description": "Fast, lightweight embeddings",
      "recommended": true
    }
  ],
  "chat_models": [
    {
      "id": "qwen2.5-coder-7b-q4",
      "name": "Qwen 2.5 Coder 7B (Q4_K_M)",
      "size_bytes": 4718592000,
      "context_length": 32768,
      "url": "https://huggingface.co/Qwen/Qwen2.5-Coder-7B-Instruct-GGUF/resolve/main/qwen2.5-coder-7b-instruct-q4_k_m.gguf",
      "sha256": "...",
      "description": "Excellent code model",
      "recommended": true,
      "tags": ["code", "instruct"]
    }
  ]
}
```

### 4.6 Registry Hosting

**Option A: GitHub Pages (recommended)**
1. Create `docs/models.json` in ted repo
2. Enable GitHub Pages in repo settings
3. Registry available at `https://yourorg.github.io/ted/models.json`

**Option B: Raw GitHub**
```
https://raw.githubusercontent.com/yourorg/ted/main/registry/models.json
```

### 4.7 First-Run Experience

```rust
pub async fn first_run_setup(settings: &mut Settings) -> Result<()> {
    println!("Welcome to Ted!\n");

    // 1. Embedding model (downloads automatically via fastembed on first use)
    println!("Semantic search will be enabled automatically (25MB download on first use).");

    // 2. Optional: Local LLM
    println!("\nWould you like to download a local LLM for offline use?");
    println!("  1. Skip for now (use Anthropic/OpenRouter)");
    println!("  2. Qwen 2.5 Coder 7B (4.5GB) - Best for code [recommended]");
    println!("  3. Llama 3.2 3B (2GB) - Smaller, faster");

    // Download if selected...
}
```

---

## Phase 5: Build & Distribution

### 5.1 Feature Flags Summary

```toml
[features]
default = ["bundled-embeddings", "local-llm"]

# Core functionality
bundled-embeddings = ["fastembed"]  # Always include
local-llm = ["llama-cpp-2"]         # Include by default

# Hardware acceleration (optional)
cuda = ["llama-cpp-2/cuda"]         # For NVIDIA GPUs
metal = []                           # Auto-enabled on macOS

# Minimal build (cloud-only)
cloud-only = []                      # Exclude local LLM
```

### 5.2 Binary Size Estimates

| Build Configuration | Estimated Size |
|---------------------|----------------|
| Cloud-only (no local LLM) | ~15MB |
| Default (embeddings + llama.cpp CPU) | ~30MB |
| With CUDA support | ~50MB |

Note: Model files are downloaded separately and stored in `~/.ted/models/`

### 5.3 Cross-Compilation Matrix

| Target | Embeddings | LLM | Notes |
|--------|------------|-----|-------|
| x86_64-apple-darwin | ✅ | ✅ + Metal | Auto GPU |
| aarch64-apple-darwin | ✅ | ✅ + Metal | M1/M2/M3 |
| x86_64-unknown-linux-gnu | ✅ | ✅ | CPU/CUDA |
| x86_64-pc-windows-msvc | ✅ | ✅ | CPU/CUDA |

---

## Implementation Order

### Sprint 1: Bundled Embeddings
1. Add `fastembed` dependency
2. Create `src/embeddings/bundled.rs`
3. Update `EmbeddingGenerator` with backend enum
4. Update settings for embedding backend choice
5. Update indexer to use bundled embeddings
6. Test: verify embeddings work without Ollama

### Sprint 2: Vector Index
1. Create `src/indexer/vector.rs`
2. Integrate vector index into Indexer
3. Implement `semantic_search()` method
4. Implement `hybrid_search()` with RRF
5. Test: verify semantic search returns relevant results

### Sprint 3: Local LLM Provider
1. Add `llama-cpp-2` dependency with feature flag
2. Create `src/llm/providers/llama_cpp.rs`
3. Implement `LlmProvider` trait for LlamaCpp
4. Add `/model` commands for model management
5. Update provider factory
6. Test: verify local inference works

### Sprint 4: Model Registry & Download
1. Create `src/models/registry.rs` with registry client
2. Set up registry JSON (GitHub Pages or raw GitHub)
3. Implement model download with progress & SHA256 verification
4. Add `/model list`, `/model download`, `/model load` commands
5. Create embedded fallback registry (`src/models/fallback.json`)
6. Test: verify registry fetch, download, and offline fallback

### Sprint 5: Polish & Distribution
1. Implement first-run setup wizard
2. Add model download progress UI in TUI
3. Update CI for cross-platform builds
4. Write user documentation
5. Test: end-to-end offline experience

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Large binary size | Feature flags for optional components |
| Slow embedding on first run | Show progress, cache aggressively |
| Model compatibility issues | Curated model list, version pinning |
| Memory usage with large models | Configurable context size, mmap |
| Cross-platform build failures | CI matrix testing all targets |

---

## Success Criteria

1. **Zero external dependencies**: `ted` binary runs without Ollama or any other server
2. **Fast startup**: < 2s to interactive prompt (embedding model lazy-loaded)
3. **Semantic search works**: Relevant code found for natural language queries
4. **Offline capable**: Full functionality without internet (after model download)
5. **Reasonable size**: Default binary < 50MB, models downloaded separately
