# Free and local feature matrix

[← Documentation home](README.md) · [Project home](../README.md)

This matrix is based on the pinned official Hermes source plus the local
integration at commit `ee683263aaa7f3bca33f785630926350fa119c38`. “Installed”
means dependencies and UI/tool code are present; runtime availability can
still depend on the active surface and a health check.

## Core surfaces and local state

| Feature | Classification | Notes |
|---|---|---|
| Hermes CLI | Enabled by default | Managed Python runtime; selected local llama.cpp provider |
| Hermes TUI | Enabled by default | Standalone and real ConPTY/xterm.js launcher surface |
| Hermes Desktop / Chat | Enabled by default | Packaged official Electron/React application |
| Hermes Web Dashboard | Enabled by default | Unified backend at the configured loopback port |
| Local API/model server | Enabled by default | Authenticated llama.cpp at the configured loopback port |
| Sessions and SQLite state | Enabled by default | Persistent under `data\hermes`; FTS/session search available |
| Local memory | Enabled by default / security-sensitive | Built-in provider; writes require approval |
| Skills | Enabled by default / security-sensitive | List/view/manage available; writes require approval |
| Todos | Enabled by default | Local structured planning |
| Cron | Enabled by default / security-sensitive | Built-in local scheduler; user controls creation and execution |
| Projects/context files | Enabled by default | Desktop project surface and local context files |
| Batch processing | Installed but disabled | Invoke explicitly for suitable work |
| Trajectory capture | Installed but disabled | Local/free, opt-in because traces can contain private content |
| Local diagnostics | Enabled by default | Redacted archive export |
| Local plugin management | Installed but disabled | No plugin enabled automatically |
| ACP/IDE support | Installed but disabled | Free/local; start only when an IDE integration is configured |

## Toolsets

| Tool/toolset family | Classification | Notes |
|---|---|---|
| File read/search | Enabled by default | Local, bounded, canonicalised paths |
| File write/patch | Enabled by default / security-sensitive | Explicit approval and installation protection |
| Terminal/process | Enabled by default / security-sensitive | Local backend, safe cwd, timeout/cancellation/output bounds |
| Code execution | Enabled by default / security-sensitive | Local managed Python; no cloud sandbox |
| Browser automation | Enabled by default / security-sensitive | Local Chromium/Playwright; SSRF and navigation tests pass |
| Web extraction | Enabled by default | Local extraction from user-selected pages |
| Web search | Installed but disabled | No stable zero-key API backend was promoted; browser-driven search remains available |
| Delegate task | Enabled by default | One child maximum, one level, no auto-approval |
| Cronjob / todo | Enabled by default / security-sensitive | Local structured interfaces |
| Memory / session search | Enabled by default | Write approval for memory |
| Skills list/view/manage | Enabled by default / security-sensitive | Native Windows inline-shell guard; write approval |
| Clarify | Enabled by default | Local UI interaction |
| Safe/read-only toolsets | Enabled by default | Available as narrowed selections |
| Debugging toolset | Enabled by default | Local diagnostic/code tools |
| Desktop project/open-preview/read-terminal | Enabled by default on Desktop | Hidden on incompatible surfaces |
| Kanban/orchestrator | Installed but disabled | Not needed with one delegated child |
| Computer use | Installed but disabled / security-sensitive | Requires the optional local driver and deliberate enablement |
| Vision analysis | Model-dependent | Enable only when the selected GGUF/runtime supports vision inputs |
| Image generation | Requires user credentials | External provider-backed in this configuration |
| Video analysis/generation | Requires user credentials | External provider-backed; disabled |
| X search | Requires user credentials | xAI/X credentials; disabled |
| Home Assistant | Requires user credentials | Requires an existing instance/token; disabled |
| Spotify | Requires user credentials | Disabled |
| Arbitrary MCP servers | Installed but disabled / security-sensitive | Add only reviewed servers; no inherited arbitrary MCP for delegates |

## Voice

| Feature | Classification | Notes |
|---|---|---|
| Local speech-to-text | Installed but disabled | faster-whisper-capable dependency set; enable only with sufficient accelerator memory |
| Local Piper/Kitten/NeuTTS | Installed but disabled | Optional local engines; no voice model is auto-downloaded |
| Edge TTS | Installed but disabled | Free but network-backed; clearly not offline |
| Premium/cloud TTS and STT | Requires user credentials / paid service | ElevenLabs, OpenAI, xAI, Mistral, Gemini, DeepInfra and similar are disabled |

## Model and provider backends

| Provider family | Classification | Notes |
|---|---|---|
| `local-llama` custom OpenAI-compatible provider | Enabled by default | Dynamically selects the registered GGUF; no paid inference required |
| Other local OpenAI-compatible endpoints | Installed but disabled | User may add a reviewed local server |
| Ollama local / LM Studio style endpoints | Installed but disabled | Not needed for the selected llama.cpp runtime |
| OpenRouter | Requires user credentials / typically paid | Disabled |
| Nous Portal | Requires user credentials / service account | Disabled |
| OpenAI / OpenAI Codex | Requires user credentials / paid service | Disabled |
| Anthropic / Claude Code | Requires user credentials / paid service | Disabled |
| Google AI / Vertex | Requires user credentials / paid service | Disabled |
| AWS Bedrock | Requires user credentials / paid service | Disabled |
| Azure Foundry/OpenAI | Requires user credentials / paid service | Disabled |
| xAI OAuth/API | Requires user credentials / paid service | Disabled |
| Z.AI/GLM | Requires user credentials / paid service | Disabled |
| Kimi/Moonshot | Requires user credentials / paid service | Disabled |
| DeepSeek and other API registries | Requires user credentials / paid service | Disabled |
| Ollama Cloud | Requires user credentials / paid service | Disabled |

## Messaging, automation and external memory

| Integration | Classification | Notes |
|---|---|---|
| Discord, Telegram and other bot gateways | Requires user credentials / security-sensitive | Disabled |
| Slack, Teams, Feishu/Lark and similar messaging | Requires user credentials / security-sensitive | Disabled |
| Remote terminals/cloud sandboxes | Requires paid service / security-sensitive | Disabled |
| External memory providers (Mem0, Honcho, OpenViking, etc.) | Requires user credentials or extra service | Disabled; built-in local memory is active |
| Public remote gateway | Not applicable / security-sensitive | Not implemented; loopback only |
| Docker deployment | Not applicable | Explicitly excluded from this Windows-native product |
| WSL execution | Unsupported | Explicitly rejected as a hidden prerequisite; skill preprocessing rejects the WSL Bash launcher |

## Optional local model features

| Feature | Classification | Notes |
|---|---|---|
| Prompt cache/prefix reuse | Enabled by default | 287.31x two-pass wall-clock ratio measured |
| Flash Attention | Profile-controlled | Enabled in the relevant starter profiles and editable per user |
| Q8_0 KV cache | Enabled by default | Capacity-tested at 64K and 80K |
| Speculative decoding / DFlash | Installed but disabled | No compatible trustworthy draft model passed promotion criteria |
| Maximum 128K context | Installed but disabled by default | Experimental and never auto-selected |
| Additional model downloads | User-controlled | Register an existing GGUF or add a manifest/source; no unselected download |
