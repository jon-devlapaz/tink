# Stage 05: Review and release

Inputs: current stage 04 evidence, candidate diff, approved brief or spec/plan,
and `_shared/REVIEW.md`.
Output: `runs/<slug>/05-deploy/output/REVIEW-findings.md` and a PR when requested.

Check logic, security boundaries, and acceptance criteria. Use separate review
passes for risk that warrants them; a model review is not human approval.
Important findings return to stage 03 and require renewed verification.

Gate: independently authenticated code-owner approval and current required CI in
the forge. Local review files cannot approve a release. Consult the deployment
system for the deployed revision, health result, and rollback reference.

Prune ephemeral skills only at run closure after rework is finished, through the
serialized wrapper and a dry-run first. Preserve provenance before cleanup.
See `_system/SDLC.md`; avoid `--all-unpinned` for routine cleanup.
