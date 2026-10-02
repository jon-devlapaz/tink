# Stage 02: Design

Inputs: current approved intent for full runs; `runs/<slug>/brief.md` for light runs.
Output: `runs/<slug>/02-design/output/spec.md` for full runs, approach and acceptance
criteria sections in the brief for light runs.

List behavior, interfaces, risks, and proof targets. Scout or route skills only
for actual capability gaps. Required policy skills load deterministically.
Follow the skill lifecycle and serialized CLI wrapper in `_system/SDLC.md`.

Gate: full runs require a current stage 2 human decision before build planning.
Light runs include design in the combined stage 3 definition review. Light runs have ONE definition gate, recorded as stage 3 (`sdlc.py decide <run> 3 ...`): the approved `brief.md` + `checklist.json` are the intent, design and plan.
Changing upstream inputs makes existing approvals stale; preserve feedback and
revise the artifact rather than deleting downstream work.

## Skills

Skillset: `design-skillset` (pin: `.tink/skillsets/design-skillset.json`).
- Once per machine/library, after reviewing the pin (it selects exact upstream code):
  `tink library fetch .tink/skillsets/design-skillset.json`
- At stage open, compile the required disciplines, then start a NEW session so
  `AGENTS.md` is re-read: `tink use design-skillset --snapshot runs/<slug>/02-design`
  The launcher does this for you: `python3 _system/scripts/sdlc.py stage <slug> 2`.
- For a capability gap: `tink-route --receipt runs/<slug>/skills.jsonl "<what you need>"`
  (searches the whole library; prints the skill on stdout. Exit 1 means nothing fits; exit 1 or 2 means
  continue without a skill).

Stage skills (always for this stage; `tink use` compiles the same set):
- `principle-build-the-lever`: Non-trivial work: build the script or tool that does or proves it, so a reviewer can rerun it, instead of doing it by hand.
- `architect`: Before code crosses a function boundary: sketch types, signatures and module structure first, then stay in the loop as the implementation fills in.
- `principle-foundational-thinking`: Before writing logic: choose the core types and data structures, and what concurrent actors share.
When one of these triggers fires, or the stage-open pick (`runs/<slug>/skills/stage-<n>-pick.json`) names a skill, read that skill in full before acting on it. In the handoff note, name each skill that changed a decision and the decision it changed.
