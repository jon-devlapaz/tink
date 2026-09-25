# Stage 01: Define intent

Inputs: originator's request and `_shared/intent-template.md` for full runs.
Read the run's `run.json` to select its profile.

For light runs, write `runs/<slug>/brief.md` with problem, acceptance criteria,
approach, implementation checklist, risks, and verification. Combine stages 01–03
into one human-reviewed definition. Use stage 3 when recording that decision.
For full runs, write `runs/<slug>/01-plan/output/intent.md`.
Use `interrogate` only for consequential unresolved decisions.

Gate: actual human acceptance recorded with `sdlc.py decide`; text status tags
are not approval evidence. Follow `_system/scripts/status.sh <slug>`.
See `_system/SDLC.md` for rejection, stale inputs, and authority boundaries.
