# Skillset router

Load after a successful `tink skillset add` when the skillset-root `SKILL.md`
is missing, or when the user asks to create or overwrite a skillset-root router.

Scripts and [router-canonical.md](router-canonical.md) ship in this skill.

## Authority

| User said… | Authorizes… |
| --- | --- |
| `tink skillset add NAME-skillset` succeeded and root `SKILL.md` is missing | **Create** the required root router |
| Overwrite / replace / regenerate / update the router | **Overwrite** for the named skillset |

A root `SKILL.md` on a receipt-backed skillset is the agent router. Tink's
receipt digest ignores it so authoring does not dirty the install. Leave
`.tink-skillset.json` and member skills as Tink left them. Refresh preserves an
existing router.

## Procedure

### 1. Inventory

From this skill's directory:

```bash
node "<manage-tink>/scripts/list-members.mjs" "<NAME-skillset-or-abs-dir>" \
  [--project <project-root>] [--all-trees] [--stdout summary]
```

Read the `inventoryFile` from the summary (under
`~/.tink/cache/manage-tink-skillset-router/`). Re-open a member file only when
an inventory field looks wrong.

**Done when:** Summary and inventory file are in hand for every authorized
target tree.

### 2. Draft

Load [router-canonical.md](router-canonical.md). Draft from inventory only:
descriptions, openers, handoffs, `byRole`, `clusters`, and `receiptDiff`.

**Done when:** The draft matches the canonical contract and every Ask cell
traces to inventory evidence.

### 3. Write and verify

Write only `<skillset-dir>/SKILL.md` for each authorized target.

```bash
node "<manage-tink>/scripts/verify-router.mjs" "<skillset-dir>"
```

Clear every `failures` entry. Fix or justify each `warnings` entry in the
report. Confirm receipts and member files are unchanged.

**Done when:** Verify reports `ok: true`, and the report names paths, mode
(create or overwrite), inventory file, receiptDiff, and any empty
descriptions.

## Scope

Stay inside router authoring: leave install, refresh, remove, lock, and sync
to the main manage-tink procedure. Keep inventory JSON under
`~/.tink/cache/manage-tink-skillset-router/`. Keep skillset nesting and
receipts intact.
