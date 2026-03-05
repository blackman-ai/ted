# Permissions Policy V2

## Summary
Ted supports optional static permission policies from:
- user scope: `~/.ted/permissions.toml`
- project scope: `<project>/.ted/permissions.toml`

Project rules are evaluated after user rules.  
The last matching rule wins.

Permission decisions are appended to:
- `~/.ted/audit/permissions.jsonl`

## File schema

```toml
include = ["packs/shared-policy.toml"] # optional, relative or absolute

[[rules]]
effect = "allow" # allow | ask | deny
tools = ["shell"]            # optional glob patterns
commands = ["cargo *"]       # optional glob patterns
paths = ["src/**"]           # optional glob patterns
destructive = false          # optional boolean matcher
reason = "Safe local build commands"

[[lock_rules]]
effect = "deny"
tools = ["shell"]
commands = ["git push --force*"]
reason = "Org guardrail (non-overridable)"
```

Rule field semantics:
- `effect`: required behavior when matched.
- `tools`: matches tool name (`shell`, `file_edit`, `file_*`).
- `commands`: matches command text for shell-like actions.
- `paths`: matches affected paths from tool requests.
- `destructive`: if set, only matches when action has same destructive flag.
- `reason`: optional human-readable explanation.
- `include`: optional list of policy pack files to include (relative paths resolve from the current file directory).
- `lock_rules`: enforced rules evaluated after normal rules; when matched they override regular allow/ask outcomes.

If a match dimension is omitted or empty, it does not constrain matching.

## Examples

Allow routine Cargo commands:

```toml
[[rules]]
effect = "allow"
tools = ["shell"]
commands = ["cargo *"]
reason = "Routine Rust build/test operations"
```

Deny dangerous shell command classes:

```toml
[[rules]]
effect = "deny"
tools = ["shell"]
commands = ["rm -rf *", "git push --force*"]
reason = "High-risk destructive operations"
```

Force prompt for edits under migrations:

```toml
[[rules]]
effect = "ask"
tools = ["file_edit", "file_write"]
paths = ["migrations/**", "db/**"]
reason = "Require explicit confirmation on schema-impacting files"
```

Deny edits to secrets:

```toml
[[rules]]
effect = "deny"
tools = ["file_edit", "file_write"]
paths = ["secrets/**", "**/*.pem", "**/.env*"]
reason = "Protected secret material"
```

## Evaluation behavior
1. Collect rules from user file then project file.
2. Evaluate each rule in order.
3. Keep the last rule that matches all provided dimensions.
4. Evaluate `lock_rules` and apply the last matching lock rule (if any).
5. Apply final effect:
   - `allow`: skip interactive permission prompt
   - `ask`: force interactive prompt
   - `deny`: reject request immediately

## Compatibility
- If no policy files exist, Ted uses legacy permission behavior.
- `--trust` still bypasses prompts and policy checks.

## CLI helpers
- `ted permissions show` - display active file paths and merged policy sources.
- `ted permissions init [--scope user|project]` - write a starter template.
- `ted permissions check --tool <name> --action "<text>" [--path <p>] [--destructive]` - dry-run a decision.
- `ted permissions log --limit 20` - show recent audited permission decisions.
- `ted compliance --since 2026-03-01` - summarize policy denies/prompts/trust usage from audit events.
