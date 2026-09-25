# Stage 03: Implement

Inputs: current approved spec for full runs, or combined brief for light runs.
Output: `runs/<slug>/03-build/output/plan.md` for full runs, implementation checklist
in `runs/<slug>/brief.md` for light runs, and code in an isolated worktree/clone.

Inspect code, write the plan, and obtain the actual stage 3 human acceptance before
implementation. An explicit user instruction to execute a reviewed proposal is
implementation authority; never fabricate separate role approvals.
Each concurrent writer needs its own checkout. Use the serialized skill wrapper
in `_system/SDLC.md`; no shared writable skill symlinks or automatic lockfile rewrites.

For bug fixes, reproduce the expected failure first, obtain independent acceptance,
and record protected test inputs with `sdlc.py lock-tests`. Implement and verify in
a loop with stage 04. Update the plan when scope changes and renew stale decisions.
