# git-board

Live terminal dashboard for GitHub Actions runs.

`git-board` uses the GitHub CLI for data, so it reuses your existing `gh auth`
session and repository access. If GitHub CLI authentication is missing or
invalid, `git-board` shows an authentication prompt and can run
`gh auth login -h github.com` from the current terminal.

## Screenshots

Example dashboard using `ProdVision/clinic-os`:

![git-board dashboard](docs/screenshots/dashboard.png)

Focused config panel:

![git-board config panel](docs/screenshots/config.png)

## Requirements

- Rust/Cargo
- GitHub CLI (`gh`)
- `gh auth login` with access to the repository workflows

## Usage

```sh
git-board --repo owner/name
```

When `--repo` is omitted, `git-board` runs `gh repo view --json nameWithOwner`
from the current directory and monitors that repository.

Useful options:

```sh
git-board --repo owner/name --interval 10s --limit 30
git-board --layout all
git-board --columns status,title,workflow,branch,event,pr,id,elapsed,age
git-board --pr-columns status,title,author,branch,base,number,age,updated
git-board --branch main --workflow verify --status in_progress
git-board --no-cursor-auto-hide
git-board --cursor-hide-after 10s
git-board --independent
```

By default, `git-board` shares a temporary cache between clients opened on the
same repository. One live client owns refreshes and writes the cache; other
clients read it. If the owner exits or stops heartbeating, another client can
take over refreshes. The title shows the number of live clients and marks the
current mode: `🐓 [N clients]` for the refresh owner, `🐥 [N clients]` for a
shared non-owner, and `🐺` for independent mode. The cache is
marked stale in the title when it is older than 1.2x the configured refresh
interval. The cache is removed when the last client for that repository exits.
Use `--independent` to run a process with its own refresh loop instead of the
shared cache.

Run layouts:

- `in-progress`: default startup layout. Shows in-progress runs, unresolved
  failures, and the latest successful run for each workflow that has no newer
  run.
- `all`: shows all runs.

An unresolved failure is a failed run whose same workflow and branch has no newer
started run and no newer successful run.

The default run sort is newest first, with successful runs placed after active
and failed runs in the `in-progress` layout. Press `s` to cycle between that
sort and plain newest-first ordering.

Keys:

- `r`: refresh immediately
- `l`: cycle run layout
- `s`: cycle run sort
- `⇥`: switch between GitHub Actions and open pull request screens
- `h`: toggle cursor auto-hide
- `c`: show focused config panel
- `k`: show key commands
- `␣`: quick look at the selected row
- `↑` / `↓`: move selection
- `↵`: open the selected run or pull request in the browser
- `q` / `ESC`: close panel / quit

In the quick look panel, `↑` / `↓` scrolls overflowing row data, and
`␣`, `q`, or `ESC` closes it. Failed runs include the failed job or step
summary when GitHub returns that detail.

In the config panel, `↑` / `↓` changes the focused setting, `←` /
`→`, `-`, and `+` edit values, `←` / `→` moves the cursor inside
the columns field, `↵` accepts the draft settings, and `q` / `ESC` cancels
without applying changes.

If GitHub authentication is not valid, an authentication panel asks whether to
run `gh auth login`. Choose with `←` / `→` and confirm with `↵`, or press
`Y` / `N`. `q` or `ESC` closes the prompt for the current client session.

Refresh and cache errors are shown on a separate error line above the footer and
remain visible until a successful refresh clears them or a newer error replaces
them.

## Configuration

Config is loaded from `~/.config/git-board/config.toml`. CLI flags override
config values.

```toml
repo = "owner/name"
interval = "15s"
limit = 20
layout = "in-progress"
columns = ["status", "title", "workflow", "branch", "event", "pr", "id", "elapsed", "age"]
pr_columns = ["status", "title", "author", "branch", "base", "number", "age", "updated"]

[cursor]
auto_hide = true
hide_after = "5s"

[filters]
branch = ""
workflow = ""
status = ""
event = ""
```
