# Bootstrap and Runtime Smoke Verification

Verified on 27 July 2026 on the target Windows 11 Pro workstation.

## Pinned inputs

| Component | Verified revision |
| --- | --- |
| Hermes Agent | `3be565fbdee3115ab5b9338551768b8e5e655c56` |
| llama.cpp | `0e4a0362239713ea95a6864a17a8de4b0ad90d62` |
| Laguna GGUF repository | `1a37c0a5fb8c7a18e6106decb6be6327d1b63fa6` |
| Model SHA-256 | `1ac7079101fca5a6df8c5a7523a3c30ea7d1c0e4b1258090e7d6d4039287f6cb` |

## Native runtime

- CUDA Toolkit 13.3.1 compiler: `13.3.73`.
- llama.cpp runtime: build `10154`, commit `0e4a0362`, MSVC
  `19.44.35228.0`, x64.
- CUDA device probe: NVIDIA GeForce RTX 3060, 12,287 MiB reported.
- The compiled `llama-server --help` contains every flag used by the profile
  builder, including automatic VRAM fitting, fit target, CUDA offload, Flash
  Attention, quantised KV cache, prompt cache, API key, metrics, and disabled
  Web UI.

CUDA 13.3 places cuBLAS runtime DLLs in `bin\x64`, separately from `nvcc` in
`bin`. The shared environment bootstrap discovers both directories. A runtime
probe without `bin\x64` reproduced `STATUS_DLL_NOT_FOUND`; the corrected path
then returned version and CUDA device information successfully.

## Model and API smoke

The deterministic Benchmark profile used 32K context, Q8_0 K/V cache,
automatic CUDA offload and a 1 GiB VRAM reserve. llama.cpp loaded the model in
approximately 5.4 seconds and exposed only the loopback listener on port 8011.

The first authenticated chat request:

- discovered only `laguna-xs-2.1-q4km`;
- returned exactly `LAGUNA_OK`;
- completed with a normal `stop` reason.

The native function-call request:

- returned `finish_reason: tool_calls`;
- selected `get_local_time`;
- emitted valid JSON: `{"timezone":"Europe/London"}`.

## Hermes tool smoke

Hermes one-shot mode was restricted to its real terminal tool and asked to
read the local PowerShell version. It completed three authenticated model
calls, executed the tool, and returned:

```text
HERMES_TOOL_OK 5.1.26100.8894
```

The structured usage record marked the run completed, not failed, and named
model `laguna-xs-2.1-q4km`.

An initial 401 test proved that llama.cpp rejects an incorrect API key. The
root cause was Hermes selecting the legacy bare `custom` provider instead of
the named `laguna-local` provider. Both the versioned template and runtime
configuration now select `laguna-local`; Hermes normalises that named custom
provider internally after resolving its `HERMES_LOCAL_API_TOKEN` key reference.

## Supervision

The stack starts in this order:

1. validate configuration and the full model hash;
2. start llama.cpp;
3. require `/health` and `/v1/models`;
4. start `hermes serve`;
5. require `/api/health`.

It stops in reverse order. A persistent supervisor owns both children through
a Windows Job Object configured with `KILL_ON_JOB_CLOSE`, tracks PIDs and
structured health, applies exponential restart backoff, and opens a
restart-loop breaker after five failures in five minutes. Closing llama.cpp's
stdin is the graceful first stop; bounded process-tree termination is the
fallback.
