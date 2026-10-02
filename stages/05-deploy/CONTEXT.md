# Stage 05: Review and release

Inputs: current stage 04 evidence, candidate diff, approved brief or spec/plan,
and `_shared/REVIEW.md`.
Output: `runs/<slug>/05-deploy/output/REVIEW-findings.md` and a PR when requested.
Open (or confirm) the PR from the verified, committed candidate; the agent may open it, only a human merges it.
If code changes after the PR is open, run verify again before the review continues.

Check logic, security boundaries, and acceptance criteria. Use separate review
passes for risk that warrants them; a model review is not human approval.
Important findings return to stage 03 and require renewed verification.

Gate: independently authenticated code-owner approval and current required CI in
the forge. Local review files cannot approve a release. Consult the deployment
system for the deployed revision, health result, and rollback reference.

Skill mounts land in the git-ignored `.active` directory inside `.tink` (created on first mount, not shipped); nothing to clean up at run closure.
See `_system/SDLC.md`.

## Skills

Skillset: `deployment-skillset` (pin: `.tink/skillsets/deployment-skillset.json`).
- Once per machine/library, after reviewing the pin (it selects exact upstream code):
  `tink library fetch .tink/skillsets/deployment-skillset.json`
- At stage open, compile the required disciplines, then start a NEW session so
  `AGENTS.md` is re-read: `tink use deployment-skillset --snapshot runs/<slug>/05-deploy`
  The launcher does this for you: `python3 _system/scripts/sdlc.py stage <slug> 5`.
- For a capability gap: `tink-route --receipt runs/<slug>/skills.jsonl "<what you need>"`
  (searches the whole library; prints the skill on stdout. Exit 1 means nothing fits; exit 1 or 2 means
  continue without a skill).

Stage skills (always for this stage; `tink use` compiles the same set):
- `principle-prove-it-works`: Before declaring done: run the real artifact and show its output; a green build or "it compiles" is not proof.
When one of these triggers fires, or the stage-open pick (`runs/<slug>/skills/stage-<n>-pick.json`) names a skill, read that skill in full before acting on it. In the handoff note, name each skill that changed a decision and the decision it changed.
