# Universal AI Provider Catalog CLI

Connect to 184 AI providers with 6,245 models using a single, unified interface.

## 🚀 Quick Start

```bash
# Clone and build
git clone <repo-url>
cd providers
cargo build --release

# List all 184 providers
./target/release/providers list

# List runtime providers (25 with API implementations)
./target/release/providers configured

# Set up a provider
./target/release/providers setup groq
```

## ⚡ Performance

This CLI uses **rkyv + memmap2** for ultra-fast provider data loading:

- **38 microseconds** load time (essentially instant)
- **200x faster** than Node.js
- **297x faster** than Python
- **Zero-copy** deserialization with memory mapping

## 📊 Provider Database

- **184 providers** available
- **6,245 models** supported
- **Data file**: `data/providers.rkyv` (655 KB)
- **Load method**: Zero-copy memory mapping (38 μs)
- **Sources**: LiteLLM + Models.dev merged into a local snapshot

The `184 / 6,245` count is the local binary catalog snapshot. Live LiteLLM and Models.dev counts move independently, so current official coverage should be refreshed before publishing new launch numbers.

### Provider Categories

- **Chat**: 150+ providers supporting conversational AI
- **Embedding**: 45+ providers for text embeddings
- **Image**: 30+ providers for image generation
- **Audio**: 15+ providers for speech/audio processing

### Runtime Providers (25 with API implementations)

Web-based (Cookie/OAuth):
- ChatGPT Web (Free)
- Google Gemini Web (Free)
- Claude Web (Free)
- OpenAI Codex (ChatGPT Plus/Pro)
- Google Antigravity (Free)

API Key Providers:
- Google Gemini (3 auth methods)
- Qwen and Alibaba DashScope metadata
- Groq
- Cerebras
- Mistral and Codestral metadata
- OpenRouter
- OpenCode Zen (public free models, paid models with API key)
- Cohere
- NVIDIA NIM
- SambaNova
- HuggingFace Inference
- GitHub Models
- Fireworks AI
- DeepSeek
- Together AI
- Cloudflare Workers AI
- Anthropic Claude
- Perplexity AI
- Replicate

Catalog-only freemium metadata is also tracked for OVHcloud AI Endpoints, ZAI, Scaleway, Alibaba DashScope, Gemini CLI, and other providers that should not be presented as runtime-ready until verified.

## 🏗️ Architecture

```
providers/
├── src/
│   ├── providers/
│   │   ├── rkyv_loader.rs    # Ultra-fast provider loading (38 μs)
│   │   ├── anthropic.rs      # Anthropic API
│   │   ├── google_gemini.rs  # Google Gemini
│   │   └── ...               # Other providers
│   ├── auth/                 # OAuth & cookie extraction (9 browsers)
│   └── main.rs               # CLI entry point
├── data/
│   └── providers.rkyv        # Binary provider data (655 KB, 184 providers)
└── .hexed/                   # Archived docs & benchmarks
```

## 📚 Commands

### List Providers

```bash
# List all 184 providers from database (default)
providers list

# List runtime providers with API implementations (25 providers)
providers configured

# Filter providers
providers list-all --capability chat
providers list-all --source litellm
providers list-all --with-models
```

### Setup & Configuration

```bash
# Set up a specific provider
providers setup groq

# Set up all providers interactively
providers setup-all
```

### Send Messages

```bash
# Send to specific provider
providers send groq "Explain quantum computing"

# Test all configured providers
providers hello

# Test with auto-configuration
providers test
```

### View Models

```bash
# Show all models
providers models

# Show working providers only
providers working-models

# Show with detailed limits
providers working-models --detailed

# JSON output
providers models --json
```

## 🔧 Development

```bash
# Format code
cargo fmt

# Check for issues
cargo clippy -- -D warnings

# Build release
cargo build --release

# Run tests
cargo test
```

## 📈 Benchmark Results

See `.hexed/BENCHMARK_RESULTS.md` for detailed performance comparison across Rust, Node.js, and Python.

**Winner**: Rust rkyv+memmap at 38 μs
- 62,521x faster than traditional JSON parsing
- 200x faster than Node.js
- 297x faster than Python

## 🌐 Browser Support

Cookie extraction supports all major browsers:
- Chrome
- Firefox
- Edge
- Brave
- Opera / Opera GX
- Chromium
- Vivaldi
- Safari (macOS only)

## 🎯 Current Status

- ✅ Provider data loaded via rkyv+memmap (38 μs)
- ✅ 184 providers documented in the local snapshot (6,245 models)
- ✅ 25 providers with API implementations
- ✅ Structured provider identity and freemium metadata
- ✅ OpenCode Zen public free models exposed as a runtime provider
- ✅ OAuth & cookie extraction (9 browsers)
- ✅ Multi-browser support
- 🚧 Additional provider integrations in progress

## 📝 Documentation

- **README.md** - This file (project overview)
- **DX.md** - Development standards and guidelines
- **.hexed/** - Archived documentation, research, and benchmarks
  - `BENCHMARK_RESULTS.md` - Complete benchmark results
  - `MERGED_PROVIDERS_SUMMARY.md` - All 184 providers list
  - `LITELLM_INTEGRATION_PLAN.md` - Implementation strategy

## 📝 License

[Your License Here]
