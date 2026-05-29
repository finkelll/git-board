# git-board

Live terminal dashboard for GitHub Actions runs.

`git-board` uses the GitHub CLI for data, so it reuses your existing `gh auth`
session and repository access.

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
git-board --columns status,title,workflow,branch,event,id,elapsed,age
git-board --branch main --workflow verify --status in_progress
git-board --no-cursor-auto-hide
git-board --cursor-hide-after 10s
```

Keys:

- `r`: refresh immediately
- `h`: toggle cursor auto-hide
- `c`: show focused config panel
- `k`: show key commands
- `Up` / `Down`: move selection
- `Enter`: open the selected run in the browser via `gh run view <id> --web`
- `q` / `Esc`: close panel / quit

In the config panel, `Up` / `Down` changes the focused setting, `Left` /
`Right`, `-`, and `+` edit values, `Left` / `Right` moves the cursor inside
the columns field, `Enter` accepts the draft settings, and `q` / `Esc` cancels
without applying changes.

## Configuration

Config is loaded from `~/.config/git-board/config.toml`. CLI flags override
config values.

```toml
repo = "owner/name"
interval = "15s"
limit = 20
columns = ["status", "title", "workflow", "branch", "event", "id", "elapsed", "age"]

[cursor]
auto_hide = true
hide_after = "5s"

[filters]
branch = ""
workflow = ""
status = ""
event = ""
```
