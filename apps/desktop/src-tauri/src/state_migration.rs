use crate::models::StoredDesktopConfig;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

const PRODUCT_DATA_DIR_NAME: &str = "dev.codexlink.desktop";
const LEGACY_DATA_DIR_NAME: &str = "dev.webcodex.desktop";
const MIGRATION_MARKER: &str = ".codexlink-state-migration-v1";
const MIGRATION_IN_PROGRESS: &str = ".codexlink-state-migration-v1.in-progress";
const CONFIG_FILE_LIMIT: u64 = 2 * 1024 * 1024;
const DATABASE_FILE_LIMIT: u64 = 1024 * 1024 * 1024;
const MAX_CONNECTION_DIRS: usize = 128;
const MAX_PROJECT_RECORDS: usize = 256;

pub(crate) fn migrate_legacy_state_if_needed(target: &Path) -> io::Result<()> {
    let Some(legacy) = legacy_data_dir(target) else {
        return Ok(());
    };
    if path_exists(target)? {
        require_real_directory(target, "CodexLink state directory")?;
    }
    let marker = target.join(MIGRATION_MARKER);
    if existing_regular_file(&marker)? {
        return Ok(());
    }
    let in_progress = target.join(MIGRATION_IN_PROGRESS);
    let resuming = existing_regular_file(&in_progress)?;
    if !path_exists(&legacy)? {
        return Ok(());
    }
    require_real_directory(&legacy, "legacy CodexLink migration source")?;

    if !resuming && target_has_authoritative_state(target)? {
        ensure_real_directory(target)?;
        write_bytes_if_missing(&marker, b"codexlink-state-migration-v1\n", None)?;
        return Ok(());
    }

    ensure_real_directory(target)?;
    if !resuming {
        write_bytes_if_missing(
            &in_progress,
            b"codexlink-state-migration-v1.in-progress\n",
            None,
        )?;
    }

    migrate_desktop_state(&legacy, target)?;
    copy_known_relative_file(
        &legacy,
        target,
        "secrets/tunnel-config.json",
        CONFIG_FILE_LIMIT,
    )?;
    migrate_runtime(&legacy, target)?;
    migrate_connections(&legacy, target)?;
    migrate_license_payload(&legacy, target)?;

    write_bytes_if_missing(&marker, b"codexlink-state-migration-v1\n", None)?;
    remove_regular_file_if_present(&in_progress)?;
    Ok(())
}

fn target_has_authoritative_state(target: &Path) -> io::Result<bool> {
    if !path_exists(target)? {
        return Ok(false);
    }
    require_real_directory(target, "CodexLink state directory")?;
    let mut has_state = existing_regular_file(&target.join("desktop-state.json"))?;
    for name in [
        "license",
        "secrets",
        "runtime",
        "connections",
        "local-connections",
    ] {
        has_state |= existing_real_directory(&target.join(name))?;
    }
    Ok(has_state)
}

fn legacy_data_dir(target: &Path) -> Option<PathBuf> {
    if target.file_name()?.to_str()? != PRODUCT_DATA_DIR_NAME {
        return None;
    }
    Some(target.parent()?.join(LEGACY_DATA_DIR_NAME))
}

fn migrate_desktop_state(legacy: &Path, target: &Path) -> io::Result<()> {
    let source = legacy.join("desktop-state.json");
    let destination = target.join("desktop-state.json");
    if existing_regular_file(&destination)? || !path_exists(&source)? {
        return Ok(());
    }
    let metadata = require_regular_file(&source, CONFIG_FILE_LIMIT)?;
    let mut text = String::new();
    File::open(&source)?.read_to_string(&mut text)?;
    let mut config: StoredDesktopConfig = serde_json::from_str(&text).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("legacy desktop-state.json is invalid: {error}"),
        )
    })?;
    rebase_stored_runtime_paths(&mut config, legacy, target);
    migrate_legacy_tunnel_id(legacy, &mut config)?;
    let encoded = serde_json::to_vec_pretty(&config).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("could not encode migrated desktop state: {error}"),
        )
    })?;
    write_bytes_if_missing(&destination, &encoded, Some(&metadata))?;
    Ok(())
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyTunnelCredentials {
    tunnel_id: String,
    api_key: String,
}

fn migrate_legacy_tunnel_id(legacy: &Path, config: &mut StoredDesktopConfig) -> io::Result<()> {
    if config.openai_tunnel_id.is_some() {
        return Ok(());
    }
    let path = legacy.join("secrets/tunnel-config.json");
    if !path_exists(&path)? {
        return Ok(());
    }
    require_regular_file(&path, CONFIG_FILE_LIMIT)?;
    let bytes = fs::read(&path)?;
    let saved: Option<LegacyTunnelCredentials> =
        serde_json::from_slice(&bytes).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("legacy Tunnel configuration is invalid: {error}"),
            )
        })?;
    let Some(saved) = saved else {
        return Ok(());
    };
    let valid_id = !saved.tunnel_id.is_empty()
        && saved.tunnel_id.len() <= 256
        && saved
            .tunnel_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-');
    let valid_key = !saved.api_key.is_empty()
        && saved.api_key.len() <= 8192
        && saved.api_key.bytes().all(|value| value.is_ascii_graphic());
    if !valid_id || !valid_key {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "legacy Tunnel configuration is invalid",
        ));
    }
    config.openai_tunnel_id = Some(saved.tunnel_id);
    Ok(())
}

fn rebase_stored_runtime_paths(config: &mut StoredDesktopConfig, legacy: &Path, target: &Path) {
    let Some(runtime) = config.runtime.as_mut() else {
        return;
    };
    rebase_optional_owned_path(&mut runtime.server_env_file, legacy, target);
    rebase_optional_owned_path(&mut runtime.runner_config, legacy, target);
    rebase_optional_owned_path(&mut runtime.user_token_file, legacy, target);
}

fn rebase_optional_owned_path(path: &mut Option<PathBuf>, legacy: &Path, target: &Path) {
    let Some(current) = path.as_ref() else {
        return;
    };
    if let Some(rebased) = rebase_owned_path(current, legacy, target) {
        *path = Some(rebased);
    }
}

fn rebase_owned_path(path: &Path, legacy: &Path, target: &Path) -> Option<PathBuf> {
    let relative = path.strip_prefix(legacy).ok()?;
    let mut components = relative.components();
    let Component::Normal(first) = components.next()? else {
        return None;
    };
    let mut rebased = target.to_path_buf();
    if first == "local-connections" || first == "connections" {
        rebased.push("connections");
    } else {
        rebased.push(first);
    }
    for component in components {
        let Component::Normal(name) = component else {
            return None;
        };
        rebased.push(name);
    }
    Some(rebased)
}

fn migrate_runtime(legacy: &Path, target: &Path) -> io::Result<()> {
    for relative in [
        "runtime/local/webcodex.env",
        "runtime/local/data/webcodex.db",
        "runtime/local/data/webcodex.db-wal",
        "runtime/local/data/webcodex.db-shm",
        "runtime/local/data/sessions.json",
    ] {
        let limit = if relative.contains("webcodex.db") {
            DATABASE_FILE_LIMIT
        } else {
            CONFIG_FILE_LIMIT
        };
        copy_known_relative_file(legacy, target, relative, limit)?;
    }
    Ok(())
}

fn migrate_license_payload(legacy: &Path, target: &Path) -> io::Result<()> {
    for relative in ["license/license.json", "license/device_identity.dpapi"] {
        copy_known_relative_file(legacy, target, relative, CONFIG_FILE_LIMIT)?;
    }
    Ok(())
}

fn migrate_connections(legacy: &Path, target: &Path) -> io::Result<()> {
    let historical = legacy.join("connections");
    let upstream = legacy.join("local-connections");
    let historical_exists = path_exists(&historical)?;
    let upstream_exists = path_exists(&upstream)?;
    if historical_exists && upstream_exists {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "legacy state contains both connections and local-connections; refusing to merge them",
        ));
    }
    let source = if historical_exists {
        historical
    } else if upstream_exists {
        upstream
    } else {
        return Ok(());
    };
    require_real_directory(&source, "legacy connections directory")?;
    let destination = target.join("connections");
    ensure_real_directory(&destination)?;
    let mut directory_budget = MAX_CONNECTION_DIRS;
    migrate_connection_tree(
        &source,
        &destination,
        0,
        &mut directory_budget,
        legacy,
        target,
    )
}

fn migrate_connection_tree(
    source: &Path,
    destination: &Path,
    depth: usize,
    directory_budget: &mut usize,
    legacy: &Path,
    target: &Path,
) -> io::Result<()> {
    if depth > 3 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "legacy connection layout exceeds the bounded migration depth",
        ));
    }
    if connection_leaf(source)? {
        ensure_real_directory(destination)?;
        copy_known_file(
            &source.join("server.toml"),
            &destination.join("server.toml"),
            CONFIG_FILE_LIMIT,
        )?;
        copy_known_file(
            &source.join("webcodex-user-token"),
            &destination.join("webcodex-user-token"),
            CONFIG_FILE_LIMIT,
        )?;
        migrate_runner_config(source, destination, legacy, target)?;
        migrate_project_registry(source, destination)?;
        return Ok(());
    }
    let mut entries = fs::read_dir(source)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let metadata = fs::symlink_metadata(entry.path())?;
        if !metadata.is_dir() {
            continue;
        }
        let name = entry.file_name();
        if !valid_dynamic_component(&name) {
            continue;
        }
        if *directory_budget == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "legacy connection layout exceeds the bounded directory count",
            ));
        }
        *directory_budget -= 1;
        migrate_connection_tree(
            &entry.path(),
            &destination.join(name),
            depth + 1,
            directory_budget,
            legacy,
            target,
        )?;
    }
    Ok(())
}

fn connection_leaf(path: &Path) -> io::Result<bool> {
    for name in [
        "server.toml",
        "runner.toml",
        "agent.toml",
        "webcodex-user-token",
    ] {
        if path_exists(&path.join(name))? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn migrate_runner_config(
    source_dir: &Path,
    destination_dir: &Path,
    legacy: &Path,
    target: &Path,
) -> io::Result<()> {
    let runner = source_dir.join("runner.toml");
    let agent = source_dir.join("agent.toml");
    let runner_exists = path_exists(&runner)?;
    let agent_exists = path_exists(&agent)?;
    if runner_exists && agent_exists {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "legacy connection contains both runner.toml and agent.toml",
        ));
    }
    let Some((source, name)) = runner_exists
        .then_some((runner, "runner.toml"))
        .or_else(|| agent_exists.then_some((agent, "agent.toml")))
    else {
        return Ok(());
    };
    if existing_regular_file(&destination_dir.join("runner.toml"))?
        || existing_regular_file(&destination_dir.join("agent.toml"))?
    {
        return Ok(());
    }
    let metadata = require_regular_file(&source, CONFIG_FILE_LIMIT)?;
    let mut text = String::new();
    File::open(&source)?.read_to_string(&mut text)?;
    let mut value: toml::Value = toml::from_str(&text).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("legacy Runner config is invalid TOML: {error}"),
        )
    })?;
    if let Some(project_registry) = value
        .get_mut("project_registry_dir")
        .and_then(|value| value.as_str())
        .map(PathBuf::from)
        .and_then(|path| rebase_owned_path(&path, legacy, target))
    {
        value["project_registry_dir"] =
            toml::Value::String(project_registry.to_string_lossy().to_string());
    }
    let encoded = toml::to_string_pretty(&value).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("could not encode migrated Runner config: {error}"),
        )
    })?;
    write_bytes_if_missing(
        &destination_dir.join(name),
        encoded.as_bytes(),
        Some(&metadata),
    )?;
    Ok(())
}

fn migrate_project_registry(source_dir: &Path, destination_dir: &Path) -> io::Result<()> {
    let current = source_dir.join("project-registry");
    let legacy = source_dir.join("projects.d");
    let current_exists = path_exists(&current)?;
    let legacy_exists = path_exists(&legacy)?;
    if current_exists && legacy_exists {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "legacy connection contains both project-registry and projects.d",
        ));
    }
    let Some((source, name)) = current_exists
        .then_some((current, "project-registry"))
        .or_else(|| legacy_exists.then_some((legacy, "projects.d")))
    else {
        return Ok(());
    };
    if existing_real_directory(&destination_dir.join("project-registry"))?
        || existing_real_directory(&destination_dir.join("projects.d"))?
    {
        return Ok(());
    }
    require_real_directory(&source, "legacy project registry")?;
    let destination = destination_dir.join(name);
    ensure_real_directory(&destination)?;
    let mut entries = fs::read_dir(source)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    if entries.len() > MAX_PROJECT_RECORDS {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "legacy project registry exceeds the bounded migration record count",
        ));
    }
    for entry in entries {
        let name = entry.file_name();
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_file()
            || !name.to_string_lossy().ends_with(".toml")
            || !valid_dynamic_component(&name)
        {
            continue;
        }
        copy_known_file(&path, &destination.join(name), CONFIG_FILE_LIMIT)?;
    }
    Ok(())
}

fn copy_known_relative_file(
    source_root: &Path,
    destination_root: &Path,
    relative: &str,
    limit: u64,
) -> io::Result<()> {
    let relative = Path::new(relative);
    if relative.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "migration allowlist path must be relative",
        ));
    }
    let mut current = source_root.to_path_buf();
    if let Some(parent) = relative.parent() {
        for component in parent.components() {
            let Component::Normal(name) = component else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "migration allowlist path contains an unsupported component",
                ));
            };
            current.push(name);
            if !path_exists(&current)? {
                return Ok(());
            }
            require_real_directory(&current, "legacy migration subtree")?;
        }
    }
    copy_known_file(
        &source_root.join(relative),
        &destination_root.join(relative),
        limit,
    )
}

fn copy_known_file(source: &Path, destination: &Path, limit: u64) -> io::Result<()> {
    if existing_regular_file(destination)? || !path_exists(source)? {
        return Ok(());
    }
    let source_metadata = require_regular_file(source, limit)?;
    let parent = destination.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "migration destination has no parent",
        )
    })?;
    ensure_real_directory(parent)?;
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "migration destination has an invalid name",
            )
        })?;
    let temporary = parent.join(format!(".{file_name}.codexlink-migrate.tmp"));
    if path_exists(&temporary)? {
        let metadata = fs::symlink_metadata(&temporary)?;
        if !metadata.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "migration temporary path is not a regular file",
            ));
        }
        fs::remove_file(&temporary)?;
    }

    let result = (|| -> io::Result<()> {
        let input = File::open(source)?;
        let opened_metadata = input.metadata()?;
        if !opened_metadata.is_file() || opened_metadata.len() > limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "migration source changed while opening: {}",
                    source.display()
                ),
            ));
        }
        let mut limited_input = input.take(limit.saturating_add(1));
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let copied = io::copy(&mut limited_input, &mut output)?;
        if copied > limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "migration source exceeded its size limit while copying: {}",
                    source.display()
                ),
            ));
        }
        output.sync_all()?;
        fs::set_permissions(&temporary, source_metadata.permissions())?;
        if existing_regular_file(destination)? {
            fs::remove_file(&temporary)?;
            return Ok(());
        }
        fs::rename(&temporary, destination)?;
        Ok(())
    })();
    if result.is_err() && is_regular_file(&temporary).unwrap_or(false) {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn write_bytes_if_missing(
    destination: &Path,
    bytes: &[u8],
    source_metadata: Option<&fs::Metadata>,
) -> io::Result<()> {
    if existing_regular_file(destination)? {
        return Ok(());
    }
    let parent = destination.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "migration destination has no parent",
        )
    })?;
    ensure_real_directory(parent)?;
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "migration destination has an invalid name",
            )
        })?;
    let temporary = parent.join(format!(".{file_name}.codexlink-migrate.tmp"));
    if path_exists(&temporary)? {
        let metadata = fs::symlink_metadata(&temporary)?;
        if !metadata.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "migration temporary path is not a regular file",
            ));
        }
        fs::remove_file(&temporary)?;
    }
    let result = (|| -> io::Result<()> {
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        output.write_all(bytes)?;
        output.sync_all()?;
        if let Some(metadata) = source_metadata {
            fs::set_permissions(&temporary, metadata.permissions())?;
        }
        if existing_regular_file(destination)? {
            fs::remove_file(&temporary)?;
            return Ok(());
        }
        fs::rename(&temporary, destination)?;
        Ok(())
    })();
    if result.is_err() && is_regular_file(&temporary).unwrap_or(false) {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn existing_regular_file(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(true),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("migration path is not a regular file: {}", path.display()),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn existing_real_directory(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(true),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("migration path is not a real directory: {}", path.display()),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn remove_regular_file_if_present(path: &Path) -> io::Result<()> {
    if existing_regular_file(path)? {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn require_regular_file(path: &Path, limit: u64) -> io::Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("migration source is not a regular file: {}", path.display()),
        ));
    }
    if metadata.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "migration source exceeds its size limit: {}",
                path.display()
            ),
        ));
    }
    Ok(metadata)
}

fn ensure_real_directory(path: &Path) -> io::Result<()> {
    if path_exists(path)? {
        return require_real_directory(path, "migration destination");
    }
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("migration destination has no parent: {}", path.display()),
        )
    })?;
    if parent != path {
        ensure_real_directory(parent)?;
    }
    match fs::create_dir(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    require_real_directory(path, "migration destination")
}

fn require_real_directory(path: &Path, label: &str) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{label} is not a real directory: {}", path.display()),
        ))
    }
}

fn is_regular_file(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn path_exists(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn valid_dynamic_component(name: &std::ffi::OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    !name.is_empty()
        && !name.starts_with('.')
        && name.len() <= 160
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        ProjectSelection, RegularConnectionPreference, StoredRuntime, TunnelProxyConfig,
    };

    fn roots() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().expect("temp dir");
        let legacy = temp.path().join(LEGACY_DATA_DIR_NAME);
        let target = temp.path().join(PRODUCT_DATA_DIR_NAME);
        fs::create_dir_all(&legacy).expect("legacy dir");
        (temp, legacy, target)
    }

    fn write(path: &Path, content: impl AsRef<[u8]>) {
        fs::create_dir_all(path.parent().expect("parent")).expect("parent dir");
        fs::write(path, content).expect("fixture write");
    }

    fn toml_path(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "\\\\")
    }

    #[cfg(windows)]
    fn create_directory_symlink(source: &Path, link: &Path) -> io::Result<()> {
        std::os::windows::fs::symlink_dir(source, link)
    }

    #[cfg(unix)]
    fn create_directory_symlink(source: &Path, link: &Path) -> io::Result<()> {
        std::os::unix::fs::symlink(source, link)
    }

    #[cfg(not(any(windows, unix)))]
    fn create_directory_symlink(_source: &Path, _link: &Path) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "directory symlink fixtures are unsupported on this platform",
        ))
    }

    #[cfg(windows)]
    fn remove_directory_symlink(link: &Path) -> io::Result<()> {
        fs::remove_dir(link)
    }

    #[cfg(unix)]
    fn remove_directory_symlink(link: &Path) -> io::Result<()> {
        fs::remove_file(link)
    }

    #[cfg(not(any(windows, unix)))]
    fn remove_directory_symlink(_link: &Path) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "directory symlink fixtures are unsupported on this platform",
        ))
    }

    fn directory_symlink_fixture(source: &Path, link: &Path) -> bool {
        match create_directory_symlink(source, link) {
            Ok(()) => true,
            Err(error)
                if error.kind() == io::ErrorKind::PermissionDenied
                    || error.kind() == io::ErrorKind::Unsupported
                    || error.raw_os_error() == Some(1314) =>
            {
                false
            }
            Err(error) => panic!("could not create directory symlink fixture: {error}"),
        }
    }

    fn fixture_state(legacy: &Path) -> StoredDesktopConfig {
        let connection = legacy
            .join("local-connections")
            .join("a")
            .join("https_example.test")
            .join("alice");
        StoredDesktopConfig {
            topology: None,
            runtime_anchor: None,
            project: Some(ProjectSelection {
                path: r"D:\GitHub\customer-project".to_string(),
                allowed_root: r"D:\GitHub\customer-project".to_string(),
                is_git_repository: true,
                runtime_project_id: Some("project-1".to_string()),
            }),
            projects: vec![ProjectSelection {
                path: r"D:\GitHub\customer-project".to_string(),
                allowed_root: r"D:\GitHub\customer-project".to_string(),
                is_git_repository: true,
                runtime_project_id: Some("project-1".to_string()),
            }],
            runtime: Some(StoredRuntime {
                server_url: "https://example.test".to_string(),
                server_env_file: Some(legacy.join("runtime/local/webcodex.env")),
                runner_config: Some(connection.join("runner.toml")),
                user_token_file: Some(connection.join("webcodex-user-token")),
                runner_client_id: Some("desktop-test".to_string()),
                project_id: Some("project-1".to_string()),
                runtime_project_id: Some("agent:desktop-test:project-1".to_string()),
            }),
            runtime_autostart: Some(true),
            preferred_connection: Some(RegularConnectionPreference::NoChatGpt),
            openai_tunnel_id: None,
            tunnel_proxy: TunnelProxyConfig::default(),
        }
    }

    fn populate_legacy(legacy: &Path) -> StoredDesktopConfig {
        let state = fixture_state(legacy);
        write(
            &legacy.join("desktop-state.json"),
            serde_json::to_vec_pretty(&state).expect("state json"),
        );
        write(
            &legacy.join("secrets/tunnel-config.json"),
            br#"{"tunnel_id":"tunnel_0123456789abcdef0123456789abcdef","api_key":"test-only-runtime-key"}"#,
        );
        write(
            &legacy.join("runtime/local/webcodex.env"),
            b"WEBCODEX_PORT=18080\n",
        );
        write(
            &legacy.join("runtime/local/data/webcodex.db"),
            b"sqlite-fixture",
        );
        write(&legacy.join("runtime/local/data/sessions.json"), b"[]\n");
        write(&legacy.join("license/license.json"), b"legacy-license\n");

        let connection = state
            .runtime
            .as_ref()
            .unwrap()
            .runner_config
            .as_ref()
            .unwrap()
            .parent()
            .unwrap();
        let registry = connection.join("project-registry");
        write(
            &connection.join("server.toml"),
            b"server_url = \"https://example.test\"\n",
        );
        write(&connection.join("webcodex-user-token"), b"token-fixture\n");
        let runner = format!(
            "server_url = \"https://example.test\"\ntoken = \"runner-fixture\"\nclient_id = \"desktop-test\"\nowner = \"alice\"\ntransport = \"websocket\"\npoll_interval_ms = 1000\nproject_registry_dir = \"{}\"\n\n[policy]\nallow_raw_shell = false\nallow_cwd_anywhere = false\nallowed_roots = [\"D:\\\\GitHub\\\\customer-project\"]\nmax_timeout_secs = 3600\nmax_output_bytes = 262144\n",
            toml_path(&registry)
        );
        write(&connection.join("runner.toml"), runner);
        write(
            &registry.join("project-1.toml"),
            b"path = \"D:\\\\GitHub\\\\customer-project\"\n",
        );
        write(&connection.join("ignored.bin"), b"do-not-copy");
        state
    }

    #[test]
    fn clean_profile_without_legacy_state_stays_clean() {
        let temp = tempfile::tempdir().expect("temp dir");
        let target = temp.path().join(PRODUCT_DATA_DIR_NAME);
        migrate_legacy_state_if_needed(&target).expect("migration");
        assert!(!target.exists());
    }

    #[test]
    fn owned_path_rebase_rejects_parent_traversal_components() {
        let (_temp, legacy, target) = roots();
        let normal = legacy.join("runtime").join("local").join("webcodex.env");
        assert_eq!(
            rebase_owned_path(&normal, &legacy, &target),
            Some(target.join("runtime").join("local").join("webcodex.env"))
        );

        let traversal = legacy
            .join("runtime")
            .join("..")
            .join("outside")
            .join("token");
        assert!(rebase_owned_path(&traversal, &legacy, &target).is_none());
    }

    #[test]
    fn migration_copies_only_bounded_state_and_rebases_owned_paths() {
        let (_temp, legacy, target) = roots();
        let source_state = populate_legacy(&legacy);
        migrate_legacy_state_if_needed(&target).expect("migration");

        let migrated: StoredDesktopConfig = serde_json::from_slice(
            &fs::read(target.join("desktop-state.json")).expect("migrated state"),
        )
        .expect("valid migrated state");
        assert_eq!(migrated.project, source_state.project);
        assert_eq!(
            migrated.openai_tunnel_id.as_deref(),
            Some("tunnel_0123456789abcdef0123456789abcdef")
        );
        let migrated_state_text =
            fs::read_to_string(target.join("desktop-state.json")).expect("migrated state text");
        assert!(!migrated_state_text.contains("test-only-runtime-key"));
        let runtime = migrated.runtime.expect("runtime");
        assert_eq!(
            runtime.server_env_file,
            Some(target.join("runtime/local/webcodex.env"))
        );
        assert!(runtime
            .runner_config
            .as_deref()
            .expect("runner config")
            .starts_with(target.join("connections")));
        assert!(runtime
            .user_token_file
            .as_deref()
            .expect("user token")
            .starts_with(target.join("connections")));
        assert!(!target.join("local-connections").exists());
        assert!(target.join("secrets/tunnel-config.json").is_file());
        assert!(target.join("runtime/local/data/webcodex.db").is_file());
        assert!(target.join("runtime/local/data/sessions.json").is_file());
        assert!(target.join("license/license.json").is_file());
        let migrated_connection = target
            .join("connections")
            .join("a")
            .join("https_example.test")
            .join("alice");
        assert!(!migrated_connection.join("ignored.bin").exists());
        assert!(target.join(MIGRATION_MARKER).is_file());

        let runner_path = runtime.runner_config.expect("runner path");
        let runner: toml::Value =
            toml::from_str(&fs::read_to_string(&runner_path).expect("runner config text"))
                .expect("runner toml");
        let expected_registry = migrated_connection.join("project-registry");
        assert_eq!(
            runner["project_registry_dir"].as_str(),
            Some(expected_registry.to_string_lossy().as_ref())
        );
        assert_eq!(
            runner["policy"]["allowed_roots"][0].as_str(),
            Some(r"D:\GitHub\customer-project")
        );
        assert!(legacy.join("desktop-state.json").is_file());
        assert!(legacy.join("local-connections").is_dir());
    }

    #[test]
    fn migration_marker_prevents_deleted_target_state_from_resurrecting() {
        let (_temp, legacy, target) = roots();
        populate_legacy(&legacy);
        migrate_legacy_state_if_needed(&target).expect("first migration");
        let migrated_secret = target.join("secrets/tunnel-config.json");
        fs::remove_file(&migrated_secret).expect("delete migrated target fixture");
        write(
            &legacy.join("secrets/tunnel-config.json"),
            br#"{"schema_version":2}"#,
        );

        migrate_legacy_state_if_needed(&target).expect("repeat migration");
        assert!(!migrated_secret.exists());
        assert!(legacy.join("secrets/tunnel-config.json").is_file());
    }

    #[test]
    fn existing_target_files_win_without_overwrite() {
        let (_temp, legacy, target) = roots();
        populate_legacy(&legacy);
        let mut target_state = StoredDesktopConfig::default();
        target_state.project = Some(ProjectSelection {
            path: r"E:\Existing\project".to_string(),
            allowed_root: r"E:\Existing\project".to_string(),
            is_git_repository: false,
            runtime_project_id: None,
        });
        write(
            &target.join("desktop-state.json"),
            serde_json::to_vec_pretty(&target_state).expect("target state json"),
        );

        migrate_legacy_state_if_needed(&target).expect("migration");
        let observed: StoredDesktopConfig = serde_json::from_slice(
            &fs::read(target.join("desktop-state.json")).expect("target state"),
        )
        .expect("valid target state");
        assert_eq!(observed.project, target_state.project);
        assert!(target.join(MIGRATION_MARKER).is_file());
        assert!(!target.join("secrets").exists());
        assert!(!target.join("runtime").exists());
        assert!(!target.join("connections").exists());
        assert!(!target.join("license").exists());
    }

    #[test]
    fn interrupted_migration_resumes_without_overwriting_existing_target_files() {
        let (_temp, legacy, target) = roots();
        populate_legacy(&legacy);
        let mut target_state = StoredDesktopConfig::default();
        target_state.project = Some(ProjectSelection {
            path: r"E:\Existing\project".to_string(),
            allowed_root: r"E:\Existing\project".to_string(),
            is_git_repository: false,
            runtime_project_id: None,
        });
        write(
            &target.join("desktop-state.json"),
            serde_json::to_vec_pretty(&target_state).expect("target state json"),
        );
        write(
            &target.join(MIGRATION_IN_PROGRESS),
            b"codexlink-state-migration-v1.in-progress\n",
        );

        migrate_legacy_state_if_needed(&target).expect("resume migration");

        let observed: StoredDesktopConfig = serde_json::from_slice(
            &fs::read(target.join("desktop-state.json")).expect("target state"),
        )
        .expect("valid target state");
        assert_eq!(observed.project, target_state.project);
        assert!(target.join("secrets/tunnel-config.json").is_file());
        assert!(target.join("runtime/local/data/webcodex.db").is_file());
        assert!(target.join(MIGRATION_MARKER).is_file());
        assert!(!target.join(MIGRATION_IN_PROGRESS).exists());
    }

    #[test]
    fn symlinked_legacy_runtime_is_rejected_when_platform_allows_symlinks() {
        let (_temp, legacy, target) = roots();
        let outside = legacy.parent().unwrap().join("outside-runtime");
        fs::create_dir_all(outside.join("local")).expect("outside runtime");
        write(&outside.join("local/webcodex.env"), b"WEBCODEX_PORT=9999\n");
        if !directory_symlink_fixture(&outside, &legacy.join("runtime")) {
            return;
        }

        let error = migrate_legacy_state_if_needed(&target).expect_err("symlink must be rejected");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(!target.join(MIGRATION_MARKER).exists());
        assert_eq!(
            fs::read_to_string(outside.join("local/webcodex.env")).unwrap(),
            "WEBCODEX_PORT=9999\n"
        );
        remove_directory_symlink(&legacy.join("runtime")).expect("remove legacy runtime symlink");
    }

    #[test]
    fn symlinked_target_subtree_is_rejected_when_platform_allows_symlinks() {
        let (_temp, legacy, target) = roots();
        populate_legacy(&legacy);
        fs::create_dir_all(&target).expect("target dir");
        let outside = target.parent().unwrap().join("outside-target-runtime");
        fs::create_dir_all(&outside).expect("outside target runtime");
        if !directory_symlink_fixture(&outside, &target.join("runtime")) {
            return;
        }

        let error = migrate_legacy_state_if_needed(&target).expect_err("symlink must be rejected");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(!target.join(MIGRATION_MARKER).exists());
        assert!(fs::read_dir(&outside).unwrap().next().is_none());
        remove_directory_symlink(&target.join("runtime")).expect("remove target runtime symlink");
    }

    #[test]
    fn symlinked_target_root_is_rejected_even_with_marker_when_platform_allows_symlinks() {
        let temp = tempfile::tempdir().expect("temp dir");
        let target = temp.path().join(PRODUCT_DATA_DIR_NAME);
        let outside = temp.path().join("outside-target-root");
        fs::create_dir_all(&outside).expect("outside target root");
        write(
            &outside.join(MIGRATION_MARKER),
            b"codexlink-state-migration-v1\n",
        );
        if !directory_symlink_fixture(&outside, &target) {
            return;
        }

        let error = migrate_legacy_state_if_needed(&target)
            .expect_err("symlinked state root must be rejected before marker handling");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(
            fs::read_to_string(outside.join(MIGRATION_MARKER)).unwrap(),
            "codexlink-state-migration-v1\n"
        );
        remove_directory_symlink(&target).expect("remove target root symlink");
    }

    #[test]
    fn unrelated_target_namespace_is_not_migrated() {
        let (temp, legacy, _target) = roots();
        populate_legacy(&legacy);
        let unrelated = temp.path().join("some.other.product");
        migrate_legacy_state_if_needed(&unrelated).expect("migration");
        assert!(!unrelated.exists());
    }

    #[test]
    fn license_migration_uses_rc5_allowlist() {
        let (_temp, legacy, target) = roots();
        write(&legacy.join("license/license.json"), "one");
        write(&legacy.join("license/device_identity.dpapi"), "two");
        write(&legacy.join("license/device.json"), "ignored");

        migrate_legacy_state_if_needed(&target).expect("migration");

        assert_eq!(
            fs::read_to_string(target.join("license/license.json")).unwrap(),
            "one"
        );
        assert_eq!(
            fs::read_to_string(target.join("license/device_identity.dpapi")).unwrap(),
            "two"
        );
        assert!(!target.join("license/device.json").exists());
    }
}
