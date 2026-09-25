# Intent: Zero-Footprint Dynamic Skill Routing

**Originator:** Architectural Review  
**Status:** draft  
**Date:** 2026-09-25  

## 1. Problem Statement
The current Agent Skills specification (agentskills.io) mandates committing or physically placing skills into a project-local `.agents/skills/` directory. While simple, this creates four structural failure modes:

1. **System Prompt Token Tax & Attention Dilution:** Every installed skill injects its frontmatter (`name` + `description`) into the host agent's base system prompt on every turn (800–5,000+ tokens per turn). Empirical research on LLM attention—notably Liu et al. (2023), *"Lost in the Middle: How Language Models Use Long Contexts"*—demonstrates that stuffing expanding catalogs into prompt prefixes significantly degrades reasoning performance and tool retrieval precision.
2. **Repository Pollution & Worktree Divergence:** Project repositories are burdened with `.agents/skills/` file trees in version control, leading to commit noise, merge conflicts, and dirty git working trees. Hermetic build doctrine (Nix, Bazel) dictates that project sources should remain strictly decoupled from developer-local or ephemeral execution tooling.
3. **Turn Latency & Redundant Inference Cycles:** Native progressive disclosure forces a preliminary tool-call turn (`read(SKILL.md)`) before code execution can begin, adding 1–3 seconds of latency and doubling API round-trips.
4. **Synchronization Drift:** Maintaining skills across 10+ active repositories requires repetitive, uncoordinated updates (`tink skill refresh`) rather than referencing a single authoritative global library.

## 2. Research & Theoretical Grounding

This architectural transition is anchored in three established engineering principles from recent AI systems literature:

### A. Hierarchical Tool Retrieval vs. Static Context Stuffing
- **Literature:** *ToolBench / ToolLLaMA* (Qin et al., 2023) and *ToolkenGPT* (Hao et al., 2023) prove that LLM tool-calling accuracy degrades precipitously when tool counts exceed ~16 in context. They establish that a **hierarchical retrieval gate** (category triage $\to$ candidate ranking $\to$ execution) achieves higher precision with near-zero base token overhead compared to flat context enumeration.
- **Voyager Architecture** (Wang et al., 2023): Proves that high-autonomy agents operate best with an offline, evolving skill store that is retrieved just-in-time into working memory rather than permanently serialized into the agent's baseline instructions.

### B. Prompt Prefix Caching Invariants (Anthropic & Gemini)
- **Literature & Provider Specs:** Anthropic (2024) and Google (2024) prompt caching architectures require exact, byte-level prefix invariance to achieve cache hits (delivering up to 90% cost reduction and 85% latency reduction).
- **Critical Invariant:** Any architecture that mutates the system prompt or early conversational turns with dynamically routed skills **completely invalidates prefix caches**. To maintain cache stability, dynamic skills must be delivered at cache-safe context boundaries: either appended to the tail of the interaction (user turn or virtual tool response) or mounted ephemerally on disk.

### C. Ephemeral Workspace Overlays (OverlayFS / Virtual Environments)
- **Literature:** Operating system principles of union filesystems and ephemeral overlays demonstrate that execution environments should present tools dynamically without polluting the underlying storage layer. For agent skills with executable scripts (`scripts/run.py`), an ephemeral mount point (`.tink/.active/` in `.gitignore` or `/tmp/tink/<session>/`) satisfies toolchain execution requirements while keeping git history 100% pure.

## 3. Proposed Outcome
Eliminate project-local `.agents/skills/` directories in favor of a **Zero-Footprint Dynamic Skill Architecture**:

1. **Ambient Pre-Hook Routing:** `tink-route` queries the offline library (`~/.tink-library/skills/`) using a sub-second, three-stage semantic gate (Tri-Noul Gate $\to$ Choice Ranking $\to$ Shortlist Verification).
2. **Cache-Safe Delivery Boundary:**
   - *Prose-Only Skills:* Delivered directly into conversation context at the prompt tail / tool response layer, preserving static system prefix caches.
   - *Multi-File Skills (with `scripts/` or `references/`):* Materialized ephemerally into a `.gitignore`'d directory (`.tink/.active/<skill>/`), providing a valid filesystem path for `bash` or `read` tools without git pollution.
3. **Strict Zero-Git Footprint:** Project git repositories track zero skill markdown files.
4. **Reproducible Project Contracts:** Projects declare and pin required capabilities in `.tink/skills.toml` and `.tink/skills.lock`, ensuring deterministic resolution across team members and CI pipelines without committing physical skill trees.

## 4. Affected Users and Systems
- **Users:** Engineers running AI agents (Pi, Claude Code, Cursor, Codex, headless CI).
- **Modules & Repositories:**
  - `tink` (Rust CLI): Scaffolding (`tink init`), project manifests (`.tink/skills.toml`), and ephemeral mount orchestration.
  - `tink-route` (Python CLI): Offline semantic evaluation, cache-safe activation contract formatting, and payload generation.
  - Harness Integration: Pi extension hook (`before_prompt` / tool middleware) and MCP server adapter for cross-harness portability.

## 5. Constraints & Boundaries
- **Prompt Cache Safety (Strict SLA):** Dynamic skill injection must never mutate the static system prompt prefix.
- **Multi-File Executability:** Skills containing `scripts/` or `references/` must be accessible to standard execution tools via an ephemeral filesystem path or virtual tool interface.
- **Performance SLA:** Pre-hook evaluation must short-circuit in $\le$ 300ms for non-specialist queries and complete full routing in $\le$ 1,200ms.
- **Fail-Safe Fallback:** If `tink-route` times out or external APIs are unreachable, execution must fail-open to standard coding models without blocking user workflows.
- **Explicitly Out of Scope:**
  - Deprecating the physical `.agents/skills/` directory for legacy toolchains; Tink will retain a manual export flag (`tink export-skills`) for environments that strictly require disk-based progressive disclosure.

## 6. Open Questions to Resolve in Stage 02 (Design)
1. **Mount Topology:** Should multi-file skills be materialized into `.tink/.active/` (gitignored in-project) or a global system temp directory (`/tmp/tink/<session_id>/`)?
2. **Delivery Protocol:** Should `tink-route` provide an MCP (Model Context Protocol) server interface to serve Cursor, Claude Code, and Codex uniformly alongside Pi's native extension hooks?
3. **Lifecycle Pruning:** What triggers the teardown of ephemeral mounts—process exit, session close hook, or periodic lru sweep?
