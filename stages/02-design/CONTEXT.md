# Stage 02: Design

Inputs: current approved intent for full runs; `runs/<slug>/brief.md` for light runs.
Output: `runs/<slug>/02-design/output/spec.md` for full runs, approach and acceptance
criteria sections in the brief for light runs.

Describe behavior, interfaces, risks, and proof targets. Scout or route skills only
for actual capability gaps. Required policy skills load deterministically.
Follow the skill lifecycle and serialized CLI wrapper in `_system/SDLC.md`.

Gate: full runs require a current stage 2 human decision before build planning.
Light runs include design in the combined stage 3 definition review.
Changing upstream inputs makes existing approvals stale; preserve feedback and
revise the artifact rather than deleting downstream work.
