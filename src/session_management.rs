/*
 * Copyright © 2025 Harald Sitter <sitter@kde.org>
 *
 * SPDX-License-Identifier: MIT
 */

use anyhow::{anyhow, bail, Result};
use ini::Ini;
use tracing::debug;
use std::path::Path;
use tokio::fs;
use zbus::{self, Connection};

use crate::systemd::SystemdUnit;

// This is our persistent store. It applies unless an ephemeral session is set.
const PERSISTENT_CONFIG_FILE: &str = "/etc/sddm.conf.d/yy-steamos-session.conf";
// The ephemeral session configuration is ordered AFTER the persistent one so it can temporarily override it.
const EPHEMERAL_CONFIG_FILE: &str = "/etc/sddm.conf.d/zz-steamos-autologin.conf";
const CONFIG_SECTION_STEAM: &str = "X-SteamOS";
const CONFIG_SECTION_AUTOLOGIN: &str = "Autologin";
const DEFAULT_DESKTOP_SESSION: &str = "plasmax11";
const DEFAULT_SESSION: &str = "gamescope-wayland";
const CONFIG_KEY_DEFAULT_DESKTOP_SESSION: &str = "DefaultDesktopSession";
const CONFIG_KEY_SESSION: &str = "Session";

async fn find_type_in_dir(
    prefix: impl AsRef<Path>,
    ty: &str
) -> Result<String> {
    let type_without_suffix = ty.trim_end_matches(".desktop");
    let expected_session = format!("{type_without_suffix}.desktop");

    let mut dir = fs::read_dir(prefix.as_ref()).await?;
    while let Some(entry) = dir.next_entry().await? {
        let file_name = entry.file_name();
        let session: &str = &file_name.to_string_lossy();
        if session == &expected_session {
            return Ok(expected_session);
        }
    }

    bail!( "Session type {ty} not found in directory {}",
        prefix.as_ref().display()
    )
}

async fn ensure_session_exists(ty: &str) -> Result<String> {
    // Guard against bad input strings. Notably we don't want relative paths here as they would allow inspecting
    // all root owned files.
    // While we are at it also figure out if the session type has a one-shot variant and prefer that.

    for dir in ["/usr/share/wayland-sessions", "/usr/share/xsessions"] {
        match find_type_in_dir(dir, ty).await {
            Ok(session) => return Ok(session),
            Err(e) => debug!("{e}"), // output and try next
        }
    }

    bail!("Session type {ty} not found in any of the known session directories")
}

async fn ini_load_async(path: &str) -> Result<Ini> {
    let data = tokio::fs::read_to_string(path).await?;
    Ini::load_from_str(data.as_str()).map_err(|e| anyhow!("Failed to load INI from {path}: {e}"))
}

async fn read_type_from_config(
    config_file: &str,
    config_section: &str,
    config_key: &str,
    default_session: &str,
) -> Result<String> {
    let config = match ini_load_async(config_file).await {
        Ok(config) => config,
        _ => return Ok(default_session.to_owned()),
    };
    match config.section(Some(config_section)) {
        Some(section) => {
            let session = section.get(config_key);
            return Ok(session.unwrap_or(default_session).to_owned());
        }
        None => return Ok(default_session.to_owned()),
    }
}

async fn write_type_to_config(
    config_file: &str,
    config_section: &str,
    config_key: &str,
    session_name: &str,
) -> Result<()> {
    let mut config = match ini_load_async(config_file).await {
        Ok(config) => config,
        _ => Ini::new(),
    };
    config
        .with_section(Some(config_section))
        .set(config_key, session_name);
    config.write_to_file(config_file)?;
    Ok(())
}

pub(crate) async fn set_session_to_switch_to(ty: &str) -> Result<()> {
    ensure_session_exists(ty).await?;
    write_type_to_config(
        EPHEMERAL_CONFIG_FILE,
        CONFIG_SECTION_AUTOLOGIN,
        CONFIG_KEY_SESSION,
        &ty,
    )
    .await
}

pub(crate) async fn read_default_desktop_session_type() -> Result<String> {
    read_type_from_config(
        PERSISTENT_CONFIG_FILE,
        CONFIG_SECTION_STEAM,
        CONFIG_KEY_DEFAULT_DESKTOP_SESSION,
        DEFAULT_DESKTOP_SESSION,
    )
    .await
}

pub(crate) async fn write_default_desktop_session_type(ty: &str) -> Result<()> {
    ensure_session_exists(ty).await?;
    write_type_to_config(
        PERSISTENT_CONFIG_FILE,
        CONFIG_SECTION_STEAM,
        CONFIG_KEY_DEFAULT_DESKTOP_SESSION,
        &ty,
    )
    .await
}

pub(crate) async fn read_default_session_type() -> Result<String> {
    read_type_from_config(
        PERSISTENT_CONFIG_FILE,
        CONFIG_SECTION_AUTOLOGIN,
        CONFIG_KEY_SESSION,
        DEFAULT_SESSION,
    )
    .await
}

pub(crate) async fn write_default_session_type(ty: &str) -> Result<()> {
    ensure_session_exists(ty).await?;
    write_type_to_config(
        PERSISTENT_CONFIG_FILE,
        CONFIG_SECTION_AUTOLOGIN,
        CONFIG_KEY_SESSION,
        ty,
    )
    .await
}

pub(crate) async fn restart_session(connection: &Connection) -> Result<()> {
    for service in ["plasma-workspace.target", "gamescope-session.service"] {
        let unit = SystemdUnit::new(connection.clone(), service).await?;
        unit.stop()
            .await
            .map_err(|e| anyhow!("Failed to stop {service}: {e}"))?;
    }

    Ok(())
}

pub(crate) async fn clear_ephemeral_session() -> Result<()> {
    tokio::fs::remove_file(EPHEMERAL_CONFIG_FILE)
        .await
        .map_err(|e| anyhow!("Failed to clear ephemeral session: {e}"))
}
