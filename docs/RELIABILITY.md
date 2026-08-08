# Reliability Notes

## Context
- Intake date: 2026-08-07
- Production profile: single-binary CLI with outbound calls to external process/tool chains.
- Scope: harden integration calls at command boundaries and record operating policy.
- Constraint: keep behavior-compatible for current flows; no schema or protocol redesign in this phase.

## Integration-Point Audit
| Dependency | Location | Timeout | Retry | Circuit Breaker | Bulkhead / isolation | Current Status |
|---|---|---|---|---|---|---|
| `curl` (GitHub API + artifact download) | `src/update.rs::curl_to_file`, `src/update.rs::update_binary` | `--connect-timeout 5`; metadata `--max-time 30`; asset `--max-time 300` | `--retry 2`, `--retry-delay 1` | `retry budget in-process` (bounded attempts) | Update call is isolated to explicit user command path | hardened |
| `git` (remote head/clone/worktree) | `src/git.rs::run_git` | `http.lowSpeedLimit=1024`, `http.lowSpeedTime=30` (mid-transfer stall; no separate connect wall-clock) | not built into helper; command-level retries via existing user retry | bounded by low-speed transport config (single-call invocation) | all git operations isolated to explicit command path (`add/refresh`) | hardened |
| `tar` (release archive extraction) | `src/update.rs::extract_tink_binary` | n/a (local filesystem) | n/a | n/a | CLI-local, no network | not applicable |
| `env::current_exe/current_dir` process boundary | `src/main.rs`, `src/update.rs` | n/a | n/a | n/a | clap parse first so `--help`/`--version` work without cwd; remaining commands fail closed on missing cwd | hardened |

## Query & Resource Findings
- No outbound SQL or paginated list endpoints in this codebase.
- Network work is bounded by explicit process-level commands with timeout/retry caps.
- Archive and Git operations remain single-process and do not maintain pooled connections.

## Health Checks & Metrics
- No dedicated long-lived service metrics are present in this CLI.
- Immediate operational checks should remain exit code + stderr assertions (integration tests already cover major error/success paths).
- If deployed in packaging pipelines, add external telemetry by wrapping process exit codes in caller scripts.

## Deploy vs Release
- **Release (`tink update`)** is explicit, user-initiated and non-automatic.
- No automatic background rollout exists in this repository.
- Runtime update behavior now degrades deterministically on integration timeout/failure instead of hanging.

## Failure Strategy
- Treat inbound command failures as hard failures with explicit messages.
- Keep retries small and bounded so failures remain actionable and do not silently mask dependency problems.
