# DiffForge

A full-screen git TUI whose file preview is a structural diff from difftastic, not git's line diff.

## Language

**DiffForge**:
The TUI this repo is building. Launch it with `difforge` inside a git work tree.
_Avoid_: lazygit clone, diffstatic

**Difftastic**:
Wilfred Hughes's structural diff CLI. The binary name is `difft`. DiffForge spawns it to render the selected path.
_Avoid_: diffstatic, delta, git diff

**Files panel**:
The left pane. It shows either all files or the changed files.
_Avoid_: status panel, sidebar, files list

**All files**:
Every path in the repository. The files panel shows this as a file tree.
_Avoid_: work tree view, full file tree, working tree view

**Changed files**:
Paths with unstaged, staged, or untracked changes.
_Avoid_: git diff only, files list, status

**File tree**:
Directories with nested files in the files panel.
_Avoid_: folder view

**Flat list**:
Changed files as a single-level list, one row per path, no directories.
_Avoid_: single-level list

**File search**:
A `/` prompt that filters the files panel by a case-insensitive path substring.
_Avoid_: find, grep, fuzzy finder

**Refresh**:
Reloading the files panel from git and the current diff from `difft` together.
_Avoid_: watch, sync, reload, poll

**Diff pane**:
The right pane. Difftastic's rendering of the selected path, or of every file under the selected directory.
_Avoid_: preview, pager, hunk view

**Binary**:
A path `difft` classifies as not text.
_Avoid_: unparseable

**Plaintext fallback**:
`difft`'s line-oriented form when it has no grammar or a size/parse limit fired.
_Avoid_: unparseable, unstructured

**Stage**:
Replace the index entry for a whole path with the working-tree file. v1 does not stage hunks or lines.
_Avoid_: git add, partial stage, hunk stage

**Index**:
Git's staging area.
_Avoid_: cache, staging area (in UI copy; "staged" as an adjective is fine)

**Alternate screen**:
The terminal's alternate buffer. DiffForge enters it on start and leaves it on quit so the previous scrollback is intact.
_Avoid_: full screen, alt mode, raw mode (raw mode is a separate setting)

**Commit summary**:
The required subject line of a commit message.
_Avoid_: title, subject (in UI copy)

**Commit description**:
The optional body of a commit message.
_Avoid_: body (in UI copy)

**Nothing-staged confirm**:
The one-row strip that asks before staging every change when `c` is pressed and the index is empty.
_Avoid_: no files staged prompt, stage-all warning

**No-files commit error**:
The one-row strip shown when `c` is pressed and porcelain is empty.
_Avoid_: no files staged, nothing to commit

**Git write error**:
The strip for a failed `git add`, `git reset`, `git rm --cached`, or `git commit`.
_Avoid_: git error, command error, stage error

**Empty-summary refusal**:
The strip on the open commit overlay when the summary is empty or whitespace-only.
_Avoid_: empty subject error, missing title, validation error
