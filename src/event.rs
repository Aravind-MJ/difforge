//! Events in and effects out of the Session.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char { c: char, ctrl: bool },
    Enter { ctrl: bool },
    Esc,
    Tab,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseHit {
    Files { index: usize },
    Search { col: u16 },
    Summary { col: u16 },
    Description { col: u16, row: u16 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChildResult {
    pub id: u64,
    pub exit: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Key(Key),
    Mouse(MouseHit),
    Tick,
    Resize { cols: u16, rows: u16 },
    Child(ChildResult),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EffectKind {
    Quit,
    GitPorcelain,
    GitLsFiles,
    GitDiff {
        path: String,
        cached: bool,
        width: u16,
    },
    Difft {
        path: String,
        width: u16,
    },
    GitAdd {
        path: String,
    },
    GitReset {
        path: String,
    },
    GitRmCached {
        path: String,
    },
    GitAddAll,
    GitCommit {
        summary: String,
        description: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Effect {
    pub id: u64,
    pub kind: EffectKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Files,
    Diff,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilesMode {
    ChangedFlat,
    ChangedTree,
    AllTree,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    File,
    Directory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowColor {
    Default,
    Staged,
    Both,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilesRow {
    pub path: String,
    pub display: String,
    pub xy: String,
    pub kind: RowKind,
    pub color: RowColor,
    pub depth: usize,
    pub expanded: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitField {
    Summary,
    Description,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CommitDraft {
    pub summary: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Strip {
    NothingStagedConfirm,
    NoFilesCommitError,
    EmptySummaryRefusal,
    GitWriteError { text: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupError {
    GitMissing,
    DifftMissing,
    NotAWorkTree,
}
