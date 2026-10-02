# Stage 03: Implement

Inputs: current approved spec for full runs, or combined brief for light runs.
Output: `runs/<slug>/03-build/output/plan.md` for full runs, the approved checklist definitions
in `runs/<slug>/checklist.json` for both profiles, and code in an isolated worktree/clone.
Items should carry a `check` whenever an automated proof exists; `verify` runs it and no mark is needed.

Inspect code, and obtain the actual stage 3 human acceptance before
implementation. For full runs, write the plan first. Light runs have ONE definition gate, recorded as stage 3 (`sdlc.py decide <run> 3 ...`): the approved `brief.md` + `checklist.json` are the intent, design and plan. An explicit user instruction to execute a reviewed proposal is
implementation authority; never fabricate separate role approvals.
Each concurrent writer needs its own checkout. Use the serialized skill wrapper
in `_system/SDLC.md`; no shared writable skill symlinks or automatic lockfile rewrites.

For bug fixes, reproduce the expected failure first, obtain independent acceptance,
and record protected test inputs with `sdlc.py lock-tests`. Implement and verify in
a loop with stage 04. Update the plan when scope changes and renew stale decisions.
Mark items only with `sdlc.py mark <slug> <item-id> passed|failed --evidence <text>`; never hand-edit receipts.

## Skills

Skillset: `build-skillset` (pin: `.tink/skillsets/build-skillset.json`).
- Once per machine/library, after reviewing the pin (it selects exact upstream code):
  `tink library fetch .tink/skillsets/build-skillset.json`
- At stage open, compile the required disciplines, then start a NEW session so
  `AGENTS.md` is re-read: `tink use build-skillset --snapshot runs/<slug>/03-build`
  The launcher does this for you: `python3 _system/scripts/sdlc.py stage <slug> 3`.
- For a capability gap: `tink-route --receipt runs/<slug>/skills.jsonl "<what you need>"`
  (searches the whole library; prints the skill on stdout. Exit 1 means nothing fits; exit 1 or 2 means
  continue without a skill).

Stage skills (always for this stage; `tink use` compiles the same set):
- `principle-build-the-lever`: Non-trivial work: build the script or tool that does or proves it, so a reviewer can rerun it, instead of doing it by hand.
- `unslop`: Any prose you write (docs, commit messages, replies): cut AI tells and filler before you send it.
When one of these triggers fires, or the stage-open pick (`runs/<slug>/skills/stage-<n>-pick.json`) names a skill, read that skill in full before acting on it. In the handoff note, name each skill that changed a decision and the decision it changed.
