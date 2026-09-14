use std::path::PathBuf;
use std::process::{Command, Stdio};
#[cfg(unix)]
use std::{io, os::unix::net::UnixStream};

use anyhow::{Context, Result};
use tracing::warn;

#[cfg(unix)]
const DIOXUS_ACTIVATION_SOCKET_NAME: &str = "keepbook-dioxus.activate.sock";

fn spawn_detached(program: &str, args: &[&str]) -> Result<()> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("Failed to launch {program}"))?;
    Ok(())
}

#[cfg(unix)]
fn dioxus_activation_socket_path() -> PathBuf {
    if let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        return PathBuf::from(runtime_dir).join(DIOXUS_ACTIVATION_SOCKET_NAME);
    }

    std::env::temp_dir().join(DIOXUS_ACTIVATION_SOCKET_NAME)
}

#[cfg(unix)]
fn activate_running_dioxus_app() -> bool {
    match UnixStream::connect(dioxus_activation_socket_path()) {
        Ok(_) => true,
        Err(error) => {
            if error.kind() != io::ErrorKind::NotFound
                && error.kind() != io::ErrorKind::ConnectionRefused
            {
                warn!(error = %error, "failed to contact running Dioxus app");
            }
            false
        }
    }
}

#[cfg(not(unix))]
fn activate_running_dioxus_app() -> bool {
    false
}

pub(crate) fn open_dioxus_app() -> Result<()> {
    if activate_running_dioxus_app() {
        return Ok(());
    }

    if let Ok(command) = std::env::var("KEEPBOOK_DIOXUS_APP_CMD") {
        if !command.trim().is_empty() {
            return spawn_detached("sh", &["-lc", &command]);
        }
    }

    let candidates: [(&str, &[&str]); 3] = [
        ("keepbook-dioxus", &[]),
        ("gtk-launch", &["org.colonelpanic.keepbook.dioxus"]),
        ("gtk-launch", &["keepbook-dioxus"]),
    ];
    let mut errors = Vec::new();

    for (program, args) in candidates {
        match spawn_detached(program, args) {
            Ok(()) => return Ok(()),
            Err(err) => errors.push(format!("{program}: {err}")),
        }
    }

    anyhow::bail!(
        "Unable to open the Dioxus app automatically ({})",
        errors.join("; ")
    )
}
