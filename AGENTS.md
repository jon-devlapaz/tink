This project uses Tink to manage Agent Skills under `.agents/skills/`.

## Testing rules

- Never write unit tests after you write code.

- Highly prefer E2E tests as the sole testing mechanism. Use them to verify complex features work. At the end of E2E tests, produce a verifiable and repeatable artifact.

- If you must test a system in isolation, first write down all the ways it could fail, then write the code.

## git-golden

A repository is `git-golden` when all of the following are true:

- It is checked out on `main` with a clean working tree.
- Local `main` is even with `origin/main`.
- Open issues and pull requests are tracked separately; they do not make the checkout unclean.
- The latest `ci` run on `main` succeeded.


<!-- AI-Native SDLC Router -->
## SDLC Workspace
- Read `_system/SDLC.md` for setup, evidence boundaries, and recovery.
- Inspect `python3 _system/scripts/sdlc.py status` before creating a run.
- Read `stages/<stage-name>/CONTEXT.md` before processing a stage.
- Keep factory references in `_shared/` unchanged during feature runs.
- Use separate worktrees or clones for code-writing runs.
- Stage skills: see the Skills section of the current stage's CONTEXT.md.
- Need a specialised skill mid-task? `tink-route --receipt runs/<slug>/skills.jsonl "<what you need>"` prints it on stdout; exit 1 means none fits, so continue without one.
<!-- End AI-Native SDLC Router -->
