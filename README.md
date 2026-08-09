<p align="center">
  <img src="assets/logo.png" alt="Tink" width="128" />
</p>

# tink

Skill manager that makes sense to you, and your agent.

Live skills live only under a project’s `.agents/skills/<name>/`. Grouped
skillsets use one canonical nested root at
`.agents/skills/<name>-skillset/<member>/`. There
is no registry and no daemon. Agents that already look for project skills find
them there.

## Install

```console
curl -fsSL https://raw.githubusercontent.com/jon-devlapaz/tink/main/install.sh | sh
```

That installs a release binary into `~/.local/bin/tink` (override with
`TINK_INSTALL_DIR`). Requires `curl` and `tar`. Supported hosts: macOS and
Linux on x86_64/arm64.

Update later with:

```console
tink update
```

### From source

You need a current Rust toolchain. For GitHub skill sources, `git` must be on
`PATH`.

```console
cargo install --git https://github.com/jon-devlapaz/tink.git --locked
```

From a checkout:

```console
cargo install --path . --root ~/.local --force
```

## Tab completion

Tink can complete commands, flags, and live library skill names. Add the line
for your shell to its startup file, then open a new shell:

```sh
# zsh (~/.zshrc)
autoload -Uz compinit
compinit
source <(COMPLETE=zsh tink)

# bash (~/.bashrc)
source <(COMPLETE=bash tink)
```

For Fish, save this as `~/.config/fish/completions/tink.fish`:

```fish
COMPLETE=fish tink | source
```

Then type `tink skill add ` and press Tab. The matches come from the current
Tink library, including skills added after completion was enabled.

## First success

In an empty project directory:

```console
tink init
tink skill add jon-devlapaz/tink-skills --skill skill-scout
tink skill list
tink skill check
```

That installs live skill directories under `.agents/skills/`. Optional:  
`tink init --with-tink-skills` pulls the full
[tink-skills](https://github.com/jon-devlapaz/tink-skills) bundle (scout +
eval loop). On a TTY, `init` may ask about ZEN and that bundle; non-interactive
runs skip them unless you pass flags.

Defaults after `init`:

1. Creates `.agents/skills/`.
2. Ensures `~/.tink` exists.
3. Installs the embedded `manage-tink` skill.

**Agents:** follow [`skills/manage-tink/SKILL.md`](skills/manage-tink/SKILL.md).
The v1 command contract is [`ACCEPTANCE.md`](ACCEPTANCE.md).

## Model

A skill is a directory with `SKILL.md` and the files it needs. Project work uses
only `.agents/skills/`. A GitHub import writes `.tink-source.json`; `skill
refresh` checks that receipt. tink does not run skill code on add, check, or
refresh.

```mermaid
flowchart LR
  agent["Agent harness"]
  tink["tink CLI"]
  live[".agents/skills/"]
  library["~/.tink/skills/"]
  catalog["~/.tink/catalog/"]

  agent -->|"discovers"| live
  tink -->|"add / remove"| live
  tink -->|"copies on add"| library
  tink -->|"records names"| catalog
  tink -->|"add <library-name>"| live
```

Home (`~/.tink` or `$TINK_HOME`) is **not** an agent discovery root. Library holds
skill trees; catalog holds by-project **names** only.

## Use (everyday)

```console
tink skill add ../my-skill
tink skill add skill-name
tink skill add owner/repository --skill skill-name
tink skill add jon-devlapaz/tink-skills --skill skill-eval-loop
tink skillset add common-skillset
tink skillset list
tink skillset list --library
tink skillset refresh common-skillset
tink skillset remove common-skillset
tink inspect https://github.com/mattpocock/skills
tink skill list
tink skill check
tink skill refresh
tink skill refresh skill-name
tink skill remove skill-name
```

- Bare `skill-name` promotes from the library (`tink skill list --library`).
- `--skill` when the source repo publishes several skills.
- Refresh only clean GitHub imports; local edits are refused.
- tink does not overwrite a project skill that differs from what it would
  install.
- tink never inits Git, stages, commits, or pushes.

### Skillsets

Skillset names are explicit and canonical: every name must end in `-skillset`;
Tink never appends or removes that suffix. `skillset add NAME-skillset` reads
the pinned definition at
`$TINK_HOME/catalog/by-skillset/NAME-skillset/meta.json`. The definition contains an
absolute HTTPS Git URL, a full commit SHA, a repository-relative `sourceRoot`,
and explicit member names. Tink copies those member skill trees atomically
under `.agents/skills/NAME-skillset/`, validates the project tree, then mirrors
that exact tree to `$TINK_HOME/skills/NAME-skillset/`. The project is primary;
the home library conforms to it and never overwrites it. Both copies carry
`.tink-skillset.json` as derived receipt evidence. Repeated identical installs
are offline no-ops. `skillset refresh NAME-skillset` updates a clean project to
its pinned catalog definition; local project modifications are refused. Library
drift is repaired only from a valid project tree.
`skillset remove NAME-skillset` removes only the project tree; it preserves the
shared catalog definition and home library copy.
`skillset list` groups each receipt-backed project skillset with its member
skills. `skillset list --library` shows the same grouped view for the home
library.

`tink inspect <GITHUB_URL>` performs a read-only inspection of a public GitHub
repository, folder, or skill URL. It reports directories containing valid
`SKILL.md` files and infers source skillsets from the URL boundary's directory
structure. Inspection never writes the project, catalog, or home library.

## Power

Library, catalog, init flags, and destroy:

```console
tink skill list --library
tink skill add skill-name
tink skill harvest
tink skill list --catalog

tink init --with-zen --with-tink-skills
tink init --no-zen --no-tink-skills --no-manage-tink

tink destroy --yes
tink update
```

**Breaking in 0.3.0:** `skill list --home` → `--catalog`; `skill list --stash` → `--library`. On-disk layout is still `$TINK_HOME/skills/` and `catalog/by-project/`. After a major CLI upgrade, refresh the live project skill: `tink skill remove manage-tink && tink init --no-zen --no-tink-skills`.

If the GitHub tip is already in the library and matches the tip byte-for-byte,
`skill add` may install from the library. If the library differs, tink repairs it
and warns. A bare skill name (not a path and not `owner/repo`) promotes from
the library into the project. `skill remove` deletes the project skill
directory and drops that name from the by-project catalog; it does not delete
library trees. `destroy` also drops this project's catalog entry.

### Uninstall the CLI

```console
rm -f ~/.local/bin/tink
# or, if installed via cargo:
cargo uninstall tink
```

## Layout

```text
.
├── ACCEPTANCE.md
├── ZEN.md
├── assets/
├── skills/           # embedded manage-tink (shipped with the binary)
├── src/
└── tests/
```

## Develop

```console
cargo test
./tink-test init
./tink-test skill list --catalog
```

`./tink-test` builds this checkout and runs `target/debug/tink` (not
`~/.local/bin/tink`). It forces `TINK_HOME` to `~/.tink-test` (override with
`TINK_TEST_HOME`) so dogfood does not touch `~/.tink`.

See [ZEN.md](ZEN.md). Flag-level detail also lives in `tink --help` and
`ACCEPTANCE.md`.

## License

[MIT](LICENSE).
