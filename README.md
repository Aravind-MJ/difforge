# DiffForge

A full-screen git TUI. The right pane is a structural dump from [difftastic](https://github.com/Wilfred/difftastic), not `git diff`.

You still stage and commit whole files. The point is a better look at what changed.

## Requirements

- a git work tree
- `git` on PATH
- [`difft`](https://github.com/Wilfred/difftastic) on PATH

`difforge` refuses to start without those. It shells out to both. It does not link libgit2 or difftastic.

## Install

From this repo:

```bash
./install.sh
```

Or:

```bash
cargo install --git https://github.com/Aravind-MJ/difforge.git --locked
```

Then run `difforge` inside a repository.

## Keys

| Key | Action |
| --- | --- |
| `j` / `k` | Move in the files panel, or scroll the dump when the diff pane is focused |
| `h` / `l` | Fold or expand a directory |
| `f` | Toggle all files vs changed files |
| `` ` `` or `t` | Tree vs flat on the changed-files list |
| `/` | Filter the files panel |
| `tab` | Swap focus between the files panel and the diff pane |
| `space` | Stage, unstage, or `git rm --cached` the selected path |
| `c` | Commit overlay. Empty index asks before staging everything |
| `r` | Reload porcelain and the current dump |
| `q` | Quit |

Mouse clicks select files and place the caret in search and commit fields.

The files panel is 28 columns. Leftover width goes to the dump. `difft` runs side-by-side at that width.

Dumps are cached by path, side (work tree / index / untracked), pane width, and a git object-id hash of the two sides. A 10s poll that finds the same bytes keeps the last dump. Loading only shows when the hash misses.

## Develop

```bash
cargo test
cargo run
```

Session tests fake git and `difft`. They do not open a TTY.

## License

MIT
