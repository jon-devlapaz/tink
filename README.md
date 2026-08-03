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

## Use

```console
tink init
tink skill add ../my-skill
tink skill list
tink skill check
```

### More

```console
# Init options (non-interactive)
tink init --with-zen --with-twotink
tink init --no-zen --no-twotink --no-manage-tink

# GitHub source; --skill when the repo has several skills
tink skill add owner/repository --skill skill-name

# Refresh clean GitHub imports; local edits are refused
tink skill refresh
tink skill refresh skill-name
```

`tink add`, `tink check`, and `tink refresh` are aliases of the matching
`tink skill …` commands. Override the home root with `TINK_HOME` when you need
an isolated home. Successful installs also record skill **names** under
`~/.tink/skills/by-project/<project>/meta.json` for offline inventory — not a
second discovery root and not a skill-tree mirror.

tink does not init Git, stage files, commit, push, or overwrite a skill that
diverged from what it would install.

## Model

A skill is a directory with `SKILL.md` plus whatever it needs. GitHub imports
get a small receipt so refresh can prove the install still matches its source.
tink does not execute skill code during add, check, or refresh.

## Develop

```console
cargo test
cargo run -q -- init
```

Maintainability notes live in [ZEN.md](ZEN.md).

## License

[MIT](LICENSE).
