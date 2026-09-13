//! Desktop-owned Tunnel credentials. Never project the Runtime Key into UI state or logs.
use crate::error::{DesktopError, DesktopResult};
use crate::models::{OpenAiTunnelConfigSnapshot, TunnelConfigSource};
use serde::Deserialize;
use std::io::Read;
use std::path::Path;
use std::process::Command;

pub(crate) const TUNNEL_KEY_FILE_NAME: &str = "openai-tunnel-key.dpapi";
pub(crate) const LEGACY_TUNNEL_CONFIG_FILE_NAME: &str = "tunnel-config.json";
const TUNNEL_KEY_MAGIC: &[u8] = b"WCDT1\0";
const MAX_SECRET_BYTES: u64 = 16 * 1024;
const MAX_API_KEY_BYTES: usize = 8192;

#[derive(Clone)]
struct Credentials {
    tunnel_id: String,
    api_key: String,
}

#[derive(Clone, Default)]
pub(crate) struct TunnelConfig {
    saved: Option<Credentials>,
    invalid: bool,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum TunnelConfigRequest {
    Save {
        #[serde(rename = "tunnelId")]
        tunnel_id: String,
        #[serde(rename = "apiKey")]
        api_key: Option<String>,
    },
    UseEnvironment,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyCredentials {
    tunnel_id: String,
    api_key: String,
}

impl TunnelConfig {
    pub fn load(saved_tunnel_id: Option<String>, key_path: &Path, legacy_path: &Path) -> Self {
        match Self::read(saved_tunnel_id, key_path, legacy_path) {
            Ok(saved) => Self {
                saved,
                invalid: false,
            },
            Err(_) => Self {
                saved: None,
                invalid: true,
            },
        }
    }

    fn read(
        saved_tunnel_id: Option<String>,
        key_path: &Path,
        legacy_path: &Path,
    ) -> DesktopResult<Option<Credentials>> {
        let Some(tunnel_id) = saved_tunnel_id else {
            if path_exists(key_path)? || path_exists(legacy_path)? {
                return Err(config_error());
            }
            return Ok(None);
        };
        let api_key = match read_saved_key(key_path)? {
            Some(value) => {
                remove_regular_file_if_present(legacy_path)?;
                value
            }
            None => {
                migrate_legacy_key(&tunnel_id, key_path, legacy_path)?.ok_or_else(config_error)?
            }
        };
        validate(&tunnel_id, &api_key)?;
        Ok(Some(Credentials { tunnel_id, api_key }))
    }

    pub fn update(
        &mut self,
        key_path: &Path,
        request: TunnelConfigRequest,
    ) -> DesktopResult<Option<String>> {
        match request {
            TunnelConfigRequest::UseEnvironment => {
                remove_regular_file_if_present(key_path)?;
                remove_regular_file_if_present(
                    &key_path.with_file_name(LEGACY_TUNNEL_CONFIG_FILE_NAME),
                )?;
                self.saved = None;
                self.invalid = false;
                Ok(None)
            }
            TunnelConfigRequest::Save { tunnel_id, api_key } => {
                let tunnel_id = tunnel_id.trim().to_string();
                let api_key = api_key
                    .filter(|value| !value.trim().is_empty())
                    .map(|value| value.trim().to_string())
                    .or_else(|| self.saved.as_ref().map(|value| value.api_key.clone()))
                    .ok_or_else(config_error)?;
                validate(&tunnel_id, &api_key)?;
                persist_saved_key(key_path, &api_key)?;
                remove_regular_file_if_present(
                    &key_path.with_file_name(LEGACY_TUNNEL_CONFIG_FILE_NAME),
                )?;
                self.saved = Some(Credentials {
                    tunnel_id: tunnel_id.clone(),
                    api_key,
                });
                self.invalid = false;
                Ok(Some(tunnel_id))
            }
        }
    }

    pub fn snapshot(&self) -> OpenAiTunnelConfigSnapshot {
        if self.invalid {
            return OpenAiTunnelConfigSnapshot {
                source: TunnelConfigSource::Invalid,
                ..Default::default()
            };
        }
        if let Some(value) = &self.saved {
            return OpenAiTunnelConfigSnapshot {
                tunnel_id_present: true,
                api_key_present: true,
                source: TunnelConfigSource::File,
                saved_tunnel_id: Some(value.tunnel_id.clone()),
                effective_tunnel_id: Some(value.tunnel_id.clone()),
            };
        }
        environment_snapshot()
    }

    pub fn apply_to_command(&self, command: &mut Command) -> DesktopResult<()> {
        if self.invalid {
            return Err(config_error());
        }
        if let Some(value) = &self.saved {
            command
                .env("CONTROL_PLANE_TUNNEL_ID", &value.tunnel_id)
                .env("CONTROL_PLANE_API_KEY", &value.api_key);
        }
        Ok(())
    }
}

fn read_saved_key(path: &Path) -> DesktopResult<Option<String>> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(config_error()),
    };
    if !metadata.is_file() || metadata.len() > MAX_SECRET_BYTES {
        return Err(config_error());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| config_error())?
        .take(MAX_SECRET_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| config_error())?;
    if bytes.len() as u64 > MAX_SECRET_BYTES
        || !bytes.starts_with(TUNNEL_KEY_MAGIC)
        || bytes.len() == TUNNEL_KEY_MAGIC.len()
    {
        return Err(config_error());
    }
    let plain = crate::secret_protection::unprotect_current_user(&bytes[TUNNEL_KEY_MAGIC.len()..])
        .map_err(|_| config_error())?;
    let api_key = String::from_utf8(plain).map_err(|_| config_error())?;
    validate_api_key(&api_key)?;
    Ok(Some(api_key))
}

fn persist_saved_key(path: &Path, api_key: &str) -> DesktopResult<()> {
    let protected = crate::secret_protection::protect_current_user(api_key.as_bytes())
        .map_err(|_| config_save_error())?;
    let mut encoded = Vec::with_capacity(TUNNEL_KEY_MAGIC.len() + protected.len());
    encoded.extend_from_slice(TUNNEL_KEY_MAGIC);
    encoded.extend_from_slice(&protected);
    if encoded.len() as u64 > MAX_SECRET_BYTES {
        return Err(config_error());
    }
    crate::state::write_atomic_file(path, &encoded).map_err(|_| config_save_error())
}

fn migrate_legacy_key(
    tunnel_id: &str,
    key_path: &Path,
    legacy_path: &Path,
) -> DesktopResult<Option<String>> {
    let metadata = match std::fs::symlink_metadata(legacy_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(config_error()),
    };
    if !metadata.is_file() || metadata.len() > MAX_SECRET_BYTES {
        return Err(config_error());
    }
    let bytes = std::fs::read(legacy_path).map_err(|_| config_error())?;
    let saved: Option<LegacyCredentials> =
        serde_json::from_slice(&bytes).map_err(|_| config_error())?;
    let Some(saved) = saved else {
        return Ok(None);
    };
    validate(&saved.tunnel_id, &saved.api_key)?;
    if saved.tunnel_id != tunnel_id {
        return Err(config_error());
    }
    persist_saved_key(key_path, &saved.api_key)?;
    remove_regular_file_if_present(legacy_path)?;
    Ok(Some(saved.api_key))
}

fn remove_regular_file_if_present(path: &Path) -> DesktopResult<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => {
            std::fs::remove_file(path).map_err(|_| config_save_error())
        }
        Ok(_) => Err(config_error()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(config_save_error()),
    }
}

fn path_exists(path: &Path) -> DesktopResult<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(config_error()),
    }
}

pub(crate) fn environment_snapshot() -> OpenAiTunnelConfigSnapshot {
    OpenAiTunnelConfigSnapshot {
        tunnel_id_present: std::env::var_os("CONTROL_PLANE_TUNNEL_ID")
            .is_some_and(|value| !value.is_empty()),
        api_key_present: std::env::var_os("CONTROL_PLANE_API_KEY")
            .is_some_and(|value| !value.is_empty()),
        source: TunnelConfigSource::Environment,
        saved_tunnel_id: None,
        effective_tunnel_id: std::env::var("CONTROL_PLANE_TUNNEL_ID").ok().filter(|id| {
            !id.is_empty()
                && id.len() <= 256
                && id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        }),
    }
}

fn validate(tunnel_id: &str, api_key: &str) -> DesktopResult<()> {
    if tunnel_id.is_empty()
        || tunnel_id.len() > 256
        || !tunnel_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-')
    {
        return Err(config_error());
    }
    validate_api_key(api_key)
}

fn validate_api_key(api_key: &str) -> DesktopResult<()> {
    if api_key.is_empty()
        || api_key.len() > MAX_API_KEY_BYTES
        || !api_key.bytes().all(|value| value.is_ascii_graphic())
    {
        return Err(config_error());
    }
    Ok(())
}

fn config_save_error() -> DesktopError {
    DesktopError::new(
        "tunnel_config_save_failed",
        "Could not save the Tunnel configuration",
        "Check access to the Desktop application data directory and retry.",
    )
}

fn config_error() -> DesktopError {
    DesktopError::new(
        "tunnel_config_invalid",
        "Tunnel configuration is missing or invalid",
        "Enter a valid Tunnel ID and Runtime Key, then save again. Invalid saved configuration never falls back to environment credentials.",
    )
}

#[cfg(test)]
mod tests;
