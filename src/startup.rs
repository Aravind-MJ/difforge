use std::io::ErrorKind;
use std::process::{Command, Stdio};

use crate::event::StartupError;

pub fn check_startup() -> Result<(), StartupError> {
    match git_inside_work_tree() {
        Err(err) if err.kind() == ErrorKind::NotFound => return Err(StartupError::GitMissing),
        Err(_) => return Err(StartupError::NotAWorkTree),
        Ok(false) => return Err(StartupError::NotAWorkTree),
        Ok(true) => {}
    }
    if !command_exists("git") {
        return Err(StartupError::GitMissing);
    }
    match Command::new("difft")
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
    {
        Err(err) if err.kind() == ErrorKind::NotFound => Err(StartupError::DifftMissing),
        Err(_) => Err(StartupError::DifftMissing),
        Ok(_) => Ok(()),
    }
}

fn git_inside_work_tree() -> std::io::Result<bool> {
    let out = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(out.status.success() && text.trim() == "true")
}

fn command_exists(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .is_ok()
}
