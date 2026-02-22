#!/usr/bin/env bash
set -euo pipefail

MODEL_INPUT="${1:-qwen2.5-coder-3b}"
QUANT_INPUT="${2:-q4_k_m}"
SMOKE_FLAG="${3:-}"

MODELS_DIR="${TED_MODELS_DIR:-$HOME/.ted/models/local}"

normalize_model() {
  echo "$1" | tr '[:upper:]' '[:lower:]' | tr ':' '-'
}

MODEL_ID="$(normalize_model "$MODEL_INPUT")"
QUANT="$(echo "$QUANT_INPUT" | tr '[:upper:]' '[:lower:]')"

if ! command -v ted >/dev/null 2>&1; then
  echo "Error: ted binary not found in PATH."
  exit 1
fi

resolve_download() {
  case "${MODEL_ID}:${QUANT}" in
    qwen2.5-coder-1.5b:q4_k_m)
      URL="https://huggingface.co/Qwen/Qwen2.5-Coder-1.5B-Instruct-GGUF/resolve/main/qwen2.5-coder-1.5b-instruct-q4_k_m.gguf"
      FILENAME="qwen2.5-coder-1.5b-instruct-q4_k_m.gguf"
      ;;
    qwen2.5-coder-1.5b:q8_0)
      URL="https://huggingface.co/Qwen/Qwen2.5-Coder-1.5B-Instruct-GGUF/resolve/main/qwen2.5-coder-1.5b-instruct-q8_0.gguf"
      FILENAME="qwen2.5-coder-1.5b-instruct-q8_0.gguf"
      ;;
    qwen2.5-coder-3b:q4_k_m)
      URL="https://huggingface.co/Qwen/Qwen2.5-Coder-3B-Instruct-GGUF/resolve/main/qwen2.5-coder-3b-instruct-q4_k_m.gguf"
      FILENAME="qwen2.5-coder-3b-instruct-q4_k_m.gguf"
      ;;
    qwen2.5-coder-3b:q5_k_m)
      URL="https://huggingface.co/Qwen/Qwen2.5-Coder-3B-Instruct-GGUF/resolve/main/qwen2.5-coder-3b-instruct-q5_k_m.gguf"
      FILENAME="qwen2.5-coder-3b-instruct-q5_k_m.gguf"
      ;;
    qwen2.5-coder-3b:q8_0)
      URL="https://huggingface.co/Qwen/Qwen2.5-Coder-3B-Instruct-GGUF/resolve/main/qwen2.5-coder-3b-instruct-q8_0.gguf"
      FILENAME="qwen2.5-coder-3b-instruct-q8_0.gguf"
      ;;
    qwen3-4b:q4_k_m)
      URL="https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF/resolve/main/Qwen3-4B-Instruct-2507-Q4_K_M.gguf"
      FILENAME="qwen3-4b-instruct-2507-q4_k_m.gguf"
      ;;
    qwen3-4b:q5_k_m)
      URL="https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF/resolve/main/Qwen3-4B-Instruct-2507-Q5_K_M.gguf"
      FILENAME="qwen3-4b-instruct-2507-q5_k_m.gguf"
      ;;
    qwen3-4b:q8_0)
      URL="https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF/resolve/main/Qwen3-4B-Instruct-2507-Q8_0.gguf"
      FILENAME="qwen3-4b-instruct-2507-q8_0.gguf"
      ;;
    qwen2.5-coder-7b:q4_k_m)
      URL="https://huggingface.co/Qwen/Qwen2.5-Coder-7B-Instruct-GGUF/resolve/main/qwen2.5-coder-7b-instruct-q4_k_m.gguf"
      FILENAME="qwen2.5-coder-7b-instruct-q4_k_m.gguf"
      ;;
    qwen2.5-coder-7b:q8_0)
      URL="https://huggingface.co/Qwen/Qwen2.5-Coder-7B-Instruct-GGUF/resolve/main/qwen2.5-coder-7b-instruct-q8_0.gguf"
      FILENAME="qwen2.5-coder-7b-instruct-q8_0.gguf"
      ;;
    *)
      echo "Unsupported model+quant combination: ${MODEL_ID} ${QUANT}"
      echo
      echo "Supported examples:"
      echo "  ./scripts/pi-prototype.sh qwen2.5-coder-3b q4_k_m --smoke"
      echo "  ./scripts/pi-prototype.sh qwen2.5-coder-1.5b q4_k_m --smoke"
      echo "  ./scripts/pi-prototype.sh qwen3-4b q4_k_m --smoke"
      echo "  ./scripts/pi-prototype.sh qwen2.5-coder-7b q4_k_m"
      exit 2
      ;;
  esac
}

download_file() {
  local url="$1"
  local target="$2"
  if command -v curl >/dev/null 2>&1; then
    curl -L --progress-bar -o "$target" "$url"
  elif command -v wget >/dev/null 2>&1; then
    wget -O "$target" "$url"
  else
    echo "Error: neither curl nor wget is installed."
    exit 1
  fi
}

resolve_download
mkdir -p "$MODELS_DIR"
TARGET_PATH="${MODELS_DIR}/${FILENAME}"

echo "==> Model setup"
echo "Model: ${MODEL_ID}"
echo "Quant: ${QUANT}"
echo "Path:  ${TARGET_PATH}"
echo

if [[ -f "$TARGET_PATH" ]]; then
  echo "Model already exists, skipping download."
else
  echo "Downloading model..."
  download_file "$URL" "$TARGET_PATH"
fi

echo
echo "Applying Ted settings..."
ted settings set provider local
ted settings set local.model "$MODEL_ID"
ted settings set local.model_path "$TARGET_PATH"

echo
echo "Done. Local provider is configured."
echo "Try: ted ask -p local \"Reply with exactly: TEDDY_PI_READY\""

if [[ "$SMOKE_FLAG" == "--smoke" ]]; then
  echo
  echo "Running smoke test..."
  ted ask -p local "Reply with exactly: TEDDY_PI_READY"
fi
