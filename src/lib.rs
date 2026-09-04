//! DiffForge session: keys, mouse, ticks, and child results in; git and `difft` effects out.

mod caret;
mod dump;
mod event;
mod files;
mod session;
mod startup;
mod tui;

pub use event::{
    ChildResult, CommitDraft, CommitField, Effect, EffectKind, Event, FilesMode, FilesRow, Focus,
    Key, MouseHit, RowColor, RowKind, StartupError, Strip,
};
pub use session::{apply, Session};
pub use startup::check_startup;
pub use tui::run;

pub fn startup_message(err: StartupError) -> &'static str {
    match err {
        StartupError::GitMissing => "difforge: git not found on PATH",
        StartupError::DifftMissing => "difforge: difft not found on PATH",
        StartupError::NotAWorkTree => "difforge: not a git work tree",
    }
}
