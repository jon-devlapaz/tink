This project uses Tink to manage Agent Skills under `.agents/skills/`.

## Testing rules

- Never write unit tests after you write code.

- Highly prefer E2E tests as the sole testing mechanism. Use them to verify complex features work. At the end of E2E tests, produce a verifiable and repeatable artifact.

- If you must test a system in isolation, first write down all the ways it could fail, then write the code.


<!-- AI-Native SDLC Router -->
## SDLC Workspace
- Read `_system/SDLC.md` for setup, evidence boundaries, and recovery.
- Inspect `_system/scripts/status.sh` before creating a run.
- Read `stages/<stage-name>/CONTEXT.md` before processing a stage.
- Keep factory references in `_shared/` unchanged during feature runs.
- Use separate worktrees or clones for code-writing runs.
<!-- End AI-Native SDLC Router -->
