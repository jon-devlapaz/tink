# Canonical skillset router

Output contract for a skillset-root router `SKILL.md`. Progressive
disclosure: metadata decides activation; the body loads on trigger; members
load only after **handoff**.

## Design rules

1. **Router, not encyclopedia.** Classify and hand off. Leave member
   principles, recipes, severity, and formats in the member skills.
2. **Description is Level 1.** Say what the suite covers, when to use this
   router, and the negative trigger (already-named member). Keep it short.
3. **Body is Level 2 and thin.** Soft line budget by active (non-deprecated)
   members: ≤10 → ~90 lines; ≤20 → ~120; larger → about `40 + 3×active + 40`
   with clusters. Prefer tables. Cluster before dropping members.
4. **Members are Level 3.** Relative links only: `member/SKILL.md`.
5. **Lightest owner wins.** One primary member per ask. Load siblings only on
   an explicit handoff, or when a coordinator needs its workers.
6. **Named member wins.** A user-named member loads alone.
7. **Coordinators are workflow-specific.** Zero, one, or many. Route the
   matching workflow there. Plain multi-domain implementation still loads only
   the owners needed unless a coordinator owns that workflow end-to-end.
8. **Prefer this tree.** When a sibling standalone skill shares a name, use
   the copy under this skillset.
9. **Evidence from disk.** Every Ask cell must paraphrase that member's
   inventory `description`, opener, or handoffs.
10. **Deprecated members leave Classify.** Boundaries only, with inventory
    `replacement` when known.
11. **Receipts stay with Tink.** Mention `.tink-skillset.json` only as an
    ownership boundary.
12. **Cluster at 12+ active members.** Use inventory `clusters` /
    `clusterHint` as a starting cut; rename when member wording supports a
    clearer lifecycle grouping.

## Role vocabulary (from inventory)

| Role | Meaning in the router |
| --- | --- |
| `coordinator` | Workflow driver for a multi-step or multi-agent flow |
| `worker` | Helper a coordinator calls; still list it when users invoke it directly |
| `leaf` | Ordinary single-purpose member |
| `deprecated` | Boundaries only |

For workers, write Ask cells that say they protect caller context.

## Templates

### Small suite (flat table)

Use when active members are under 12 and there is at most one clear holistic
coordinator.

```markdown
---
name: <skillset-dirname>
description: >
  Router for the <short suite label> skillset. Use when the user asks for
  <skillset-dirname>, which member skill to use, or work that spans
  <two-to-five domain words from real members>. Do not use when a single named
  member skill is already the clear owner.
---

# <Human title from skillset name>

Route. Prefer members under this skillset tree over any sibling standalone
skill with the same name.

## 1. Classify the request

Pick the lightest owner that covers the ask:

| Ask | Load |
| --- | --- |
| <holistic or workflow ask for each coordinator> | [<coord>/SKILL.md](<coord>/SKILL.md) |
| <paraphrase of member A description triggers> | [<a>/SKILL.md](<a>/SKILL.md) |

If the user names a member, load that member only.

If several domains are in play and a coordinator owns that workflow, load it.
Otherwise load only the owners needed for the change.

## 2. Hand off

1. Read the chosen member `SKILL.md` in full.
2. Follow that skill's procedure, references, and reporting format.
3. Load sibling members only when the chosen skill names a handoff, or when a
   coordinator requires its workers.

## 3. Boundaries

- Leave `.tink-skillset.json` untouched. It is ownership and digest evidence.
- Skillset install, refresh, and remove stay with `manage-tink`.
- Deprecated: `<dir>` — <replacement guidance from inventory>.
```

### Large suite (clustered + multiple coordinators)

Use when active members are 12+, or when inventory `byRole.coordinator` has more
than one entry.

```markdown
---
name: <skillset-dirname>
description: >
  Router for the <short suite label> skillset. Use when the user asks for
  <skillset-dirname>, which member skill to use, or work that spans
  <suite themes>. Do not use when a single named member skill is already the
  clear owner.
---

# <Human title>

Route. Prefer members under this skillset tree.

## 1. Classify the request

If the user names a member, load that member only.

### Workflow coordinators

| Ask | Load |
| --- | --- |
| <workflow A> | [<coord-a>/SKILL.md](<coord-a>/SKILL.md) |
| <workflow B> | [<coord-b>/SKILL.md](<coord-b>/SKILL.md) |

### <Cluster name from inventory, edited for clarity>

| Ask | Load |
| --- | --- |
| <ask> | [<member>/SKILL.md](<member>/SKILL.md) |

### <Next cluster>

| Ask | Load |
| --- | --- |
| <ask> | [<member>/SKILL.md](<member>/SKILL.md) |

Workers that protect caller context stay in their cluster. Keep them out of
Workflow coordinators unless inventory marks them `coordinator`.

## 2. Hand off

1. Read the chosen member `SKILL.md` in full.
2. Follow that skill's procedure, references, and reporting format.
3. Load siblings only when that skill's inventory `handoffs` (or its own text)
   requires them.

## 3. Boundaries

- Leave `.tink-skillset.json` untouched.
- Skillset install, refresh, and remove stay with `manage-tink`.
- Deprecated: `<dir>` — use `<replacement>` instead.
```

## Quality bar

A router fails when any of these hold:

- An active member with a `SKILL.md` is missing from Classify
- A deprecated member is absent from Boundaries
- An Ask cell lacks inventory evidence
- Member procedure text was pasted into the router
- The description lacks the already-named-member negative trigger
- The file teaches domain knowledge instead of routing
- Multiple inventory coordinators exist and Classify has no coordinator rows

**Done when:** `scripts/verify-router.mjs` reports `ok: true`. Treat `failures`
as blocking and `warnings` as fix-or-justify.
