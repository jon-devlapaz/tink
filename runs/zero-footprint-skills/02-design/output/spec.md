# Spec: Zero-Footprint Dynamic Skill Routing

**Derived from:** `01-plan/output/intent.md`  
**Status:** draft  
**Lead Approver:** Architectural Review  

---

## 1. Requirements & Functional Behavior

### 1.1 Project Initialization & Scaffolding (`tink init`)
1. **Zero Git Pollution:** `tink init` shall no longer create `.agents/skills/`.
2. **Project Scaffolding:** `tink init` creates:
   - `.tink/`: Project metadata directory.
   - `.tink/.gitignore`: Ignores `.tink/.active/`, `.tink/cache/`, and `.tink/ephemeral.*`.
   - `.tink/skills.toml`: Declarative capability manifest pinning required skills and revisions.
   - `.tink/skills.lock`: Immutable cryptographic digests for exact reproducibility across machines.
   - `AGENTS.md`: Standard instructions directing agents to use pre-routed or ephemeral capabilities.
3. **Legacy Mode:** `tink init --legacy-agents-skills` or `tink export` retains the ability to materialize static `.agents/skills/` trees for toolchains requiring physical directory presence.

### 1.2 Ephemeral Workspace Overlay (`.tink/.active/`)
1. **Mount Criteria:** Any skill requiring physical disk access—either containing executable scripts (`scripts/`) or multi-file references (`references/`)—is mounted ephemerally into `.tink/.active/<skill_name>/`.
2. **Side-Effect Boundary:**
   - `tink-route` remains a pure, read-only semantic evaluator. It never performs filesystem mutations directly.
   - Mounting is strictly executed via `tink mount <skill>` (Rust CLI) or harness mounting middleware.
3. **Atomic Concurrency:** Mounts use atomic staging to prevent collisions in multi-pane or parallel agent sessions:
   - Symlink/junction created at `.tink/.active/.tmp-<skill>-<uuid>`
   - Atomically renamed (`rename(2)`) to `.tink/.active/<skill>`
4. **Cross-Platform Hierarchy:**
   - **POSIX (macOS / Linux):** Read-only symbolic link (`symlink(2)`).
   - **Windows:** Directory Junction (`mklink /J`), falling back to read-only copy if permissions fail.
5. **Security & Jailbreak Prevention:** Canonical source path must strictly resolve inside `~/.tink-library/skills/`. Any path traversing outside the library root returns an immediate security error.

### 1.3 Prompt Caching Invariance & Ephemeral History
1. **Cache Preservation Invariant:** System prompt prefixes must remain byte-for-byte static across all turns. Dynamic skills are **never** injected into the system prompt.
2. **History Ephemeralization:** To prevent Turn 1's injected skill from permanently polluting Turn 2's prefix cache:
   - The harness hook delivers the skill via a virtual ephemeral message block or strips the injected tail when committing turns to persistent session history.
   - Turn 2 begins with a clean conversational baseline, allowing new skills to be routed without compounding cache invalidation.

### 1.4 CI Hydration & Cold-Start Reproducibility (`tink sync --frozen`)
1. **Cold CI Environments:** In CI runners where `~/.tink-library/` is empty:
   - Running `tink sync --frozen` validates `.tink/skills.toml` against `.tink/skills.lock`.
   - Downloads/hydrates exact pinned revisions directly into `~/.tink-library/skills/`.
   - Atomically mounts declared project dependencies into `.tink/.active/`.

---

## 2. Architecture & Data Contracts

### 2.1 Unified Routing JSON Contract (`tink-route --json`)

```json
{
  "status": "routed",
  "task": "Compile an epistemic matrix audit for memory",
  "specialist_noul": 0.89,
  "winner": "epistemic-matrix",
  "probability": 1.0,
  "confidence": 0.99,
  "elapsed_ms": 289,
  "action": {
    "type": "mount_and_inject",
    "delivery": "prompt_tail",
    "mount_command": "tink mount epistemic-matrix"
  },
  "mount": {
    "required": true,
    "target_dir": ".tink/.active/epistemic-matrix",
    "entrypoint": ".tink/.active/epistemic-matrix/SKILL.md",
    "has_scripts": false,
    "has_references": true,
    "references": ["references/evidence.md"],
    "scripts": []
  },
  "payload": {
    "placement": "user_tail",
    "ephemeral": true,
    "content": "# Epistemic Matrix\n\n## Core Workflows..."
  }
}
```

### 2.2 Project Capability Manifest (`.tink/skills.toml`)

```toml
[project]
name = "tink"
version = "1.0.35"

[dependencies]
source = "https://github.com/jon-devlapaz/tink.git"

[skills]
epistemic-matrix = { version = "1.0.0", digest = "sha256:7f4a..." }
karpathy-guidelines = { version = "1.0.0", digest = "sha256:3e2b..." }
```

### 2.3 Filesystem Topology
```
<project_root>/
├── .gitignore
├── .tink/
│   ├── .gitignore          # Contains: .active/ , cache/ , ephemeral.*
│   ├── skills.toml         # Version-pinned capability manifest
│   ├── skills.lock         # Exact immutable cryptographic digests
│   └── .active/            # Ephemeral mount root (100% git-ignored)
│       └── <skill_name>/   # Atomically mounted from ~/.tink-library/skills/
│           ├── SKILL.md
│           ├── scripts/
│           └── references/
├── AGENTS.md
└── src/                    # 100% pure application code
```

---

## 3. Harness Integration & Delivery Mechanisms

### 3.1 Pi Ambient Pre-Prompt Extension (`pi-tink-hook`)
- Hooks into `before_prompt`.
- Runs `tink-route --json "<prompt>"`.
- If `status == "routed"`:
  - If `action.mount_command` is specified, calls `tink mount <skill>` via local process.
  - Appends `payload.content` to the immediate user message context.
  - Flags the injection as ephemeral so session compaction/history does not permanently retain the injected prefix for subsequent turns.
- If `status == "no_skill_needed"`, passes the prompt through with 0 overhead.

### 3.2 Cross-Harness Protocol: Model Context Protocol (`tink-mcp`)
For Claude Code, Cursor, Codex, and Cline:
- Exposes `tink-route` as an MCP server with two tools:
  1. `tink_mount_skill(name)`: Atomically mounts an approved skill into `.tink/.active/<name>`.
  2. `tink_route(query)`: Pure semantic query returning routing recommendations and mount requirements.

---

## 4. Flagged Policy & Implementation Invariants

1. **Strict Pure Evaluator Boundary:** `tink-route` must never execute filesystem writes or symlinks; all filesystem mutations are delegated to `tink mount`.
2. **Atomic Mount Staging:** All symlinks/junctions must stage to `.tmp-<uuid>` and rename to target to avoid concurrency panics in multi-agent environments (Herdr, Pi workflows).
3. **Prefix Cache Invariant:** No dynamic skill content may be injected into the static system prompt.
4. **CI Reproducibility:** CI builds must rely strictly on `tink sync --frozen` reading `.tink/skills.lock` without depending on developer-local `~/.tink-library/` state.
