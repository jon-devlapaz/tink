# Stage 01: Define intent

Inputs: originator's request and `_shared/intent-template.md` for full runs.
Read the run's `run.json` to select its profile.

For light runs, write `runs/<slug>/brief.md` with problem, acceptance criteria,
approach, risks, and verification. Define the implementation checklist as
item definitions in `runs/<slug>/checklist.json` (id, description, verify, and a `check` whenever an automated proof exists). Combine stages 01–03
into one human-reviewed definition. Light runs have ONE definition gate, recorded as stage 3 (`sdlc.py decide <run> 3 ...`): the approved `brief.md` + `checklist.json` are the intent, design and plan.
For full runs, write `runs/<slug>/01-plan/output/intent.md`.
Use `seed-me` only for consequential unresolved decisions.
Output: `runs/<slug>/brief.md` and `runs/<slug>/checklist.json` (light runs), or
`runs/<slug>/01-plan/output/intent.md` (full runs).

Gate: actual human acceptance recorded with `sdlc.py decide`; text status tags
are not approval evidence. Follow `python3 _system/scripts/sdlc.py status <slug>`.
See `_system/SDLC.md` for rejection, stale inputs, and authority boundaries.

## Skills

Skillset: `planning-skillset` (pin: `.tink/skillsets/planning-skillset.json`).
- Once per machine/library, after reviewing the pin (it selects exact upstream code):
  `tink library fetch .tink/skillsets/planning-skillset.json`
- At stage open, compile the required disciplines, then start a NEW session so
  `AGENTS.md` is re-read: `tink use planning-skillset --snapshot runs/<slug>/01-plan`
  The launcher does this for you: `python3 _system/scripts/sdlc.py stage <slug> 1`.
- For a capability gap: `tink-route --receipt runs/<slug>/skills.jsonl "<what you need>"`
  (searches the whole library; prints the skill on stdout. Exit 1 means nothing fits; exit 1 or 2 means
  continue without a skill).

Stage skills (always for this stage; `tink use` compiles the same set):
- `principle-build-the-lever`: Non-trivial work: build the script or tool that does or proves it, so a reviewer can rerun it, instead of doing it by hand.
When one of these triggers fires, or the stage-open pick (`runs/<slug>/skills/stage-<n>-pick.json`) names a skill, read that skill in full before acting on it. In the handoff note, name each skill that changed a decision and the decision it changed.
