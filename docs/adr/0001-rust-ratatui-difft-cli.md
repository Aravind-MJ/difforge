# 0001. Rust, ratatui, git CLI, difft as a subprocess

## Status

Accepted

## Context

DiffForge is a TUI that should feel like a tiny lazygit, with better diffs from difftastic. The user asked the agent to pick the stack.

Lazygit itself is Go. Difftastic is Rust and is a CLI (`difft`) with no stable library API. The TUI must own the terminal's alternate screen.

## Decision

- Language: Rust
- TUI: ratatui on crossterm, including `EnterAlternateScreen`
- Git: subprocess calls to `git` (`status`, `add`, `restore --staged`, `commit`). No libgit2
- Diffs: spawn `difft` from PATH. Do not link difftastic as a crate

## Why

Ratatui plus crossterm is how you take the alternate screen and draw panels in Rust without writing a terminal toolkit. Spawning `difft` matches how git itself uses difftastic (`diff.external`) and avoids depending on an unstable library surface. Shelling out to `git` keeps v1 to the porcelain we already know from the files/stage/commit loop, instead of reimplementing index writes.

Go plus bubbletea would copy lazygit's ecosystem more closely, and would also work. It was dropped because the hard part here is hosting difftastic's output, not cloning lazygit's internals.

## Consequences

`difft` must be installed for diffs to render. A missing binary is a startup or pane error, not a compile-time problem. Git must be on PATH too.
