#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${1:-http://127.0.0.1:8847}"
CHAT_URL="${BASE_URL%/}/v1/chat/completions"
MODELS_URL="${BASE_URL%/}/v1/models"

if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required" >&2
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required" >&2
  exit 1
fi

models_response="$(curl -sS "$MODELS_URL" 2>/dev/null || true)"
if [[ -z "$models_response" ]]; then
  echo "Could not reach local server at $BASE_URL" >&2
  echo "Start Teddy local provider first, then rerun this script." >&2
  exit 1
fi

MODEL="${MODEL:-$(jq -r '.data[0].id // empty' <<<"$models_response")}"
if [[ -z "$MODEL" ]]; then
  echo "Could not detect a model from $MODELS_URL" >&2
  exit 1
fi

read -r -d '' TOOLS_JSON <<'JSON' || true
[
  {
    "type": "function",
    "function": {
      "name": "file_read",
      "description": "Read file content",
      "parameters": {
        "type": "object",
        "properties": {
          "path": { "type": "string" }
        },
        "required": ["path"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "file_write",
      "description": "Create or overwrite a file",
      "parameters": {
        "type": "object",
        "properties": {
          "path": { "type": "string" },
          "content": { "type": "string" }
        },
        "required": ["path", "content"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "file_edit",
      "description": "Replace text in a file",
      "parameters": {
        "type": "object",
        "properties": {
          "path": { "type": "string" },
          "old_string": { "type": "string" },
          "new_string": { "type": "string" }
        },
        "required": ["path", "old_string", "new_string"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "file_delete",
      "description": "Delete a file",
      "parameters": {
        "type": "object",
        "properties": {
          "path": { "type": "string" }
        },
        "required": ["path"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "shell",
      "description": "Run shell command",
      "parameters": {
        "type": "object",
        "properties": {
          "command": { "type": "string" }
        },
        "required": ["command"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "glob",
      "description": "Find files by glob pattern",
      "parameters": {
        "type": "object",
        "properties": {
          "pattern": { "type": "string" },
          "path": { "type": "string" }
        },
        "required": ["pattern"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "grep",
      "description": "Search text by pattern",
      "parameters": {
        "type": "object",
        "properties": {
          "pattern": { "type": "string" },
          "path": { "type": "string" }
        },
        "required": ["pattern"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "plan_update",
      "description": "Update planning checklist",
      "parameters": {
        "type": "object",
        "properties": {
          "title": { "type": "string" },
          "content": { "type": "string" }
        },
        "required": ["content"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "propose_file_changes",
      "description": "Propose grouped file operations",
      "parameters": {
        "type": "object",
        "properties": {
          "description": { "type": "string" },
          "operations": {
            "type": "array",
            "items": {
              "type": "object",
              "properties": {
                "type": { "type": "string" },
                "path": { "type": "string" },
                "content": { "type": "string" },
                "old_string": { "type": "string" },
                "new_string": { "type": "string" }
              },
              "required": ["type", "path"]
            }
          }
        },
        "required": ["operations"]
      }
    }
  }
]
JSON

chat_completion() {
  local payload="$1"
  curl -sS "$CHAT_URL" \
    -H "Content-Type: application/json" \
    -d "$payload"
}

print_case() {
  local label="$1"
  local response="$2"
  local finish_reason tool_name tool_args text
  finish_reason="$(jq -r '.choices[0].finish_reason // "none"' <<<"$response")"
  tool_name="$(jq -r '.choices[0].message.tool_calls[0].function.name // "none"' <<<"$response")"
  tool_args="$(jq -r '.choices[0].message.tool_calls[0].function.arguments // ""' <<<"$response")"
  text="$(jq -r '.choices[0].message.content // ""' <<<"$response")"
  echo "[$label]"
  echo "  finish_reason: $finish_reason"
  echo "  first_tool:    $tool_name"
  if [[ -n "$tool_args" ]]; then
    echo "  tool_args:     $tool_args"
  fi
  if [[ -n "$text" ]]; then
    echo "  text:          $text"
  fi
  echo
}

echo "Using model: $MODEL"
echo "Endpoint: $CHAT_URL"
echo

payload="$(jq -n \
  --arg model "$MODEL" \
  '{
    model: $model,
    stream: false,
    messages: [{ role: "user", content: "hi" }]
  }'
)"
response="$(chat_completion "$payload")"
print_case "greeting_no_tools" "$response"

payload="$(jq -n \
  --arg model "$MODEL" \
  --argjson tools "$TOOLS_JSON" \
  '{
    model: $model,
    stream: false,
    messages: [{ role: "user", content: "hi" }],
    tools: $tools,
    tool_choice: "auto"
  }'
)"
response="$(chat_completion "$payload")"
print_case "greeting_with_tools_auto" "$response"

payload="$(jq -n \
  --arg model "$MODEL" \
  --argjson tools "$TOOLS_JSON" \
  '{
    model: $model,
    stream: false,
    messages: [{ role: "user", content: "create a tiny html file" }],
    tools: $tools,
    tool_choice: "auto"
  }'
)"
response="$(chat_completion "$payload")"
print_case "build_with_tools_auto" "$response"

FORCED_TOOLS=(
  file_read
  file_write
  file_edit
  file_delete
  shell
  glob
  grep
  plan_update
  propose_file_changes
)

for tool in "${FORCED_TOOLS[@]}"; do
  payload="$(jq -n \
    --arg model "$MODEL" \
    --arg tool "$tool" \
    --argjson tools "$TOOLS_JSON" \
    '{
      model: $model,
      stream: false,
      messages: [{ role: "user", content: "Return a valid call for this required tool." }],
      tools: $tools,
      tool_choice: { type: "function", function: { name: $tool } }
    }'
  )"
  response="$(chat_completion "$payload")"
  print_case "forced_$tool" "$response"
done
