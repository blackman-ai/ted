# Raspberry Pi Prototype (Pi 5 8GB)

This is the fastest path to a working Teddy/Ted local-model prototype on a Raspberry Pi 5 (8GB).

## End-User Flow (No Terminal)

For Teddy users, use:

1. `Settings` -> `Hardware`
2. Click `Setup Local AI`
3. Wait for download + auto-configuration to complete

No model names or shell commands needed.

## Model Set To Test First

Run these in order:

1. `qwen2.5-coder-3b` + `q4_k_m` (default baseline)
2. `qwen2.5-coder-1.5b` + `q4_k_m` (fast fallback)
3. `qwen3-4b` + `q4_k_m` (agentic alternative)
4. `qwen2.5-coder-7b` + `q4_k_m` (quality stretch target)

## Expected Tradeoffs

| Model | Quant | Approx file size | Why |
|---|---|---:|---|
| qwen2.5-coder-1.5b | q4_k_m | 1.0 GB | Fastest + safest on thermals |
| qwen2.5-coder-3b | q4_k_m | 2.1 GB | Best baseline quality/speed on Pi 5 8GB |
| qwen3-4b | q4_k_m | 2.5 GB | Better tool-use/agent behavior in some flows |
| qwen2.5-coder-7b | q4_k_m | 4.4 GB | Higher quality, slower, tighter memory headroom |

## One-Command Setup

Use the setup helper script in this repo:

```bash
./scripts/pi-prototype.sh qwen2.5-coder-3b q4_k_m --smoke
```

Fast fallback:

```bash
./scripts/pi-prototype.sh qwen2.5-coder-1.5b q4_k_m --smoke
```

## Manual Setup (if preferred)

```bash
ted system --format json
mkdir -p ~/.ted/models/local

# Example: qwen2.5-coder-3b q4_k_m
curl -L --progress-bar \
  -o ~/.ted/models/local/qwen2.5-coder-3b-instruct-q4_k_m.gguf \
  "https://huggingface.co/Qwen/Qwen2.5-Coder-3B-Instruct-GGUF/resolve/main/qwen2.5-coder-3b-instruct-q4_k_m.gguf"

ted settings set provider local
ted settings set local.model qwen2.5-coder-3b
ted settings set local.model_path ~/.ted/models/local/qwen2.5-coder-3b-instruct-q4_k_m.gguf
```

Smoke test:

```bash
ted ask -p local "Reply with exactly: TEDDY_PI_READY"
```

## First Prototype Pass Criteria

For each model above, run 10 app-building prompts and track:

1. valid tool-call JSON rate
2. successful end-to-end app run rate
3. median response latency
4. thermal throttling events

Pick the default model by highest success rate first, then latency.
