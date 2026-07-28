# Model tuning

## Selected model

| Field | Value |
|---|---|
| Repository | `poolside/Laguna-XS-2.1-GGUF` |
| Revision | `1a37c0a5fb8c7a18e6106decb6be6327d1b63fa6` |
| File | `Laguna-XS-2.1-Q4_K_M.gguf` |
| Size | 20,274,300,032 bytes |
| SHA-256 | `1ac7079101fca5a6df8c5a7523a3c30ea7d1c0e4b1258090e7d6d4039287f6cb` |
| Quantisation | Q4_K_M |
| Model maximum | 262,144 tokens |
| Runtime | CUDA llama.cpp build 10154 |
| llama.cpp commit | `0e4a0362239713ea95a6864a17a8de4b0ad90d62` |

Laguna support from llama.cpp PR 25165 was already merged upstream on
2026-07-22, so this build uses the pinned merged implementation rather than
an obsolete feature branch.

## Promotion policy

Configurations were selected in this order:

1. correct text and native tool-call output;
2. no page-file thrashing;
3. no CUDA OOM;
4. preserve Q4_K_M model quality;
5. sustained generation at or above 15 tok/s;
6. prompt throughput;
7. lower resource cost.

Short-context speed was not allowed to select an unstable long-context
profile. Speculative decoding remains disabled because no trustworthy,
tokenizer-compatible Laguna draft model was available to pass the required
quality and tool-call equivalence gates.

## Named profiles

| Profile | Context | KV | Batch / micro-batch | VRAM reserve | Notes |
|---|---:|---|---:|---:|---|
| Daily | 65,536 | Q8_0 / Q8_0 | 1024 / 256 | 1,536 MiB | Default quality-first profile |
| Research | 65,536 | Q8_0 / Q8_0 | 1024 / 256 | 1,536 MiB | Same stable base, prompt cache on |
| Deep Research | 81,920 | Q8_0 / Q8_0 | 768 / 192 | 2,048 MiB | Measured extended context |
| Coding | 49,152 | Q8_0 / Q8_0 | 1024 / 256 | 1,536 MiB | Faster tool-heavy loops |
| Maximum Context | 131,072 | Q8_0 / Q8_0 | 512 / 128 | 2,048 MiB | Experimental, not auto-selected |
| Benchmark | 32,768 | Q8_0 / Q8_0 | 1024 / 256 | 1,024 MiB | Seed 3407, cache off |
| Safe Recovery | 8,192 | F16 / F16 | 256 / 64 | 2,048 MiB | CPU-only diagnostic fallback |

Daily, Research, Deep Research and Coding use Flash Attention, automatic GPU
layer fitting, 8 generation threads and 14 batch threads. Daily/Research use
prompt caching.

## Measured performance

| Scenario | Prompt tok/s | Decode tok/s | Peak VRAM | Peak RAM |
|---|---:|---:|---:|---:|
| Short chat 2K | — | 53.297 mean | recorded in raw results | recorded in raw results |
| Sustained 1,000-token decode | — | 54.57 mean/minimum | recorded in raw results | recorded in raw results |
| 16K | 332.672 | 40.052 | 10,743 MiB | 17.99 GiB |
| 32K | 318.644 | 41.114 | 10,797 MiB | 18.11 GiB |
| 64K | 284.582 | 33.062 | 10,750 MiB | 18.10 GiB |
| 80K | 269.377 | 33.347 | 10,743 MiB | 18.13 GiB |

The prompt-cache probe improved identical-request wall time from
24,674.19 ms to 85.88 ms, a measured 287.31x ratio. The 64K gate peaked at
7 hard-page reads/s and 2% paging-file usage with no percentage-point
increase, supporting the no-active-thrashing decision. All 18 native
benchmark cases and 41 saved result rows completed without a model-server or
validation failure.

## Tuning decisions

- Six generation threads measured 54.596 tok/s, but eight threads were kept as
  the P-core-focused operational setting because the difference was within
  run variance.
- Explicit CPU-MoE placement did not produce a meaningful benefit; automatic
  CUDA fitting with zero explicitly placed CPU-MoE layers is simpler and
  faster.
- Batch 2048 / micro-batch 512 won a short prompt sweep, but 1024 / 256 was
  retained because that exact setting passed the 64K and 80K capacity gates.
- F16 KV measured 51.354 tok/s in a short sweep, but was not capacity-tested
  at 64K/80K. Q8_0 preserves more headroom with substantially better quality
  than a Q4 fallback.
- A 1,024 MiB reserve was slightly faster in a short sweep; Daily keeps
  1,536 MiB and Deep Research 2,048 MiB to absorb desktop/browser load.
- Maximum Context stays experimental because the required endurance gate
  ended at 80K.

## Reproduce

Run the complete deterministic harness:

```powershell
& 'D:\Hermes-Local\Benchmark-Hermes-Local.ps1' -NonInteractive
```

The harness records exact binary/source revisions, commands, seeds, counters,
GPU samples, validation and errors in
`D:\Hermes-Local\benchmarks\results\latest.json`. The reader-facing report is
`D:\Hermes-Local\benchmarks\reports\LATEST.md`.

Rerun after any model, llama.cpp, CUDA, GPU driver, KV type, context, thread,
batch, reserve or speculative-decoding change.
