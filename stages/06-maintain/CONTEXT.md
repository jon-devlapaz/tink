# Maintenance intake (optional)

This is a producer of new work, not a mandatory final stage for each feature.
Inputs: telemetry and an explicitly configured, tested monitoring policy.
No monitoring job or bands policy is provisioned by this scaffold.

Before enabling automation, define suitable metric-specific thresholds, stable
baselines, incident deduplication keys, cooldowns, and active-run limits. Do not
assume universal 2-sigma/3-sigma thresholds are appropriate.

Output: a new draft run from `new-run.sh`, with observed evidence and affected
systems in its brief or intent. Repeated alerts update the existing incident
instead of spawning duplicate work.
Gate: service owner triages, dismisses, schedules, or approves the draft through
the normal definition gate. Telemetry never fabricates approval or releases code.
