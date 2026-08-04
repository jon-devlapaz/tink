<p align="center">
  <img src="assets/logo.png" alt="Tink" width="128" />
</p>

# tink

**One skill manager. Any harness. Just works.**

tink copies complete Agent Skills into a project's `.agents/skills/` directory.
No registry, no daemon, no required `~/.agents`. Agents that already look for
project skills find them where they live.

If you are an agent, follow [`skills/manage-tink/SKILL.md`](skills/manage-tink/SKILL.md).

## Install

Needs a current Rust toolchain and `git` on `PATH` for GitHub sources.

```console
cargo install --git https://github.com/jon-devlapaz/tink.git --locked
```

From a checkout:

```console
cargo install --path . --root ~/.local --force
```

### Uninstall

```console
cargo uninstall tink
```

If you installed with `--root ~/.local`:

```console
cargo uninstall --root ~/.local tink
```

## Use

```console
tink init
tink skill add ../my-skill
tink skill list
tink skill list --home
tink skill list --stash
tink skill add --stash my-skill
tink skill check
```

### More

```console
# Init options (non-interactive)
tink init --with-zen --with-tink-skills
tink init --no-zen --no-tink-skills --no-manage-tink

# GitHub source; --skill when the repo has several skills
tink skill add owner/repository --skill skill-name

# Promote a skill from the home stash into this project
tink skill add --stash skill-name

# Refresh clean GitHub imports; local edits are refused
tink skill refresh
tink skill refresh skill-name

# Remove project agent scaffolding (not ~/.tink)
tink destroy --yes
```

Override the home root with `TINK_HOME` when you need an isolated home.
Successful installs stash skill trees under `~/.tink/skills/<name>/` and
record skill **names** under `~/.tink/catalog/by-project/<project>/meta.json`.
Home is not an agent discovery root. List the stash with
`tink skill list --stash`; promote into a project with
`tink skill add --stash <name>`. When a GitHub tip is already stashed
byte-for-byte, add installs the project copy from that stash. Divergent stash
trees are repaired with a warning (project skill overwrites are still refused).

tink does not init Git, stage files, commit, push, or overwrite a skill that
diverged from what it would install.

## Model

A skill is a directory with `SKILL.md` plus whatever it needs. GitHub imports
get a small receipt so refresh can prove the install still matches its source.
tink does not execute skill code during add, check, or refresh.

**Live** skills are only under `.agents/skills/`. **Home** (`~/.tink`) holds a
rebuildable **stash** of skill trees (`skills/<name>/`) and a by-project **name
catalog** (`catalog/by-project/`). Agents never load from home; promote with
`tink skill add --stash <name>`.

## Develop

```console
cargo test
cargo run -q -- init
```

Maintainability notes live in [ZEN.md](ZEN.md).

## License

[MIT](LICENSE).
