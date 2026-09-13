use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(std::path::PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "codexlink-tunnel-settings-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.join("secrets")).unwrap();
        Self(path)
    }

    fn key_path(&self) -> std::path::PathBuf {
        self.0.join("secrets").join(TUNNEL_KEY_FILE_NAME)
    }

    fn legacy_path(&self) -> std::path::PathBuf {
        self.0.join("secrets").join(LEGACY_TUNNEL_CONFIG_FILE_NAME)
    }

    fn load(&self, tunnel_id: Option<&str>) -> TunnelConfig {
        TunnelConfig::load(
            tunnel_id.map(str::to_owned),
            &self.key_path(),
            &self.legacy_path(),
        )
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn save(id: &str, key: Option<&str>) -> TunnelConfigRequest {
    TunnelConfigRequest::Save {
        tunnel_id: id.into(),
        api_key: key.map(str::to_owned),
    }
}

#[test]
fn validation_is_bounded_and_strict() {
    assert!(validate("tunnel_valid", "key-123").is_ok());
    assert!(validate("invalid/id", "key-123").is_err());
    assert!(validate("tunnel_valid", "bad\nkey").is_err());
    assert!(validate("tunnel_valid", &"x".repeat(MAX_API_KEY_BYTES + 1)).is_err());
}

#[cfg(windows)]
#[test]
fn rc5_dpapi_pair_roundtrips_and_never_projects_key() {
    let fixture = Fixture::new();
    let mut config = TunnelConfig::default();
    let id = config
        .update(
            &fixture.key_path(),
            save("tunnel_saved", Some("test-only-api-key")),
        )
        .unwrap();
    assert_eq!(id.as_deref(), Some("tunnel_saved"));

    let encoded = std::fs::read(fixture.key_path()).unwrap();
    assert!(encoded.starts_with(TUNNEL_KEY_MAGIC));
    assert!(!encoded
        .windows(b"test-only-api-key".len())
        .any(|window| window == b"test-only-api-key"));

    let mut config = fixture.load(Some("tunnel_saved"));
    assert_eq!(config.snapshot().source, TunnelConfigSource::File);
    let projected = serde_json::to_string(&config.snapshot()).unwrap();
    assert!(!projected.contains("test-only-api-key"));

    let mut command = Command::new("unused");
    config.apply_to_command(&mut command).unwrap();
    let env: std::collections::HashMap<_, _> = command.get_envs().collect();
    assert_eq!(
        env[std::ffi::OsStr::new("CONTROL_PLANE_TUNNEL_ID")].unwrap(),
        "tunnel_saved"
    );
    assert_eq!(
        env[std::ffi::OsStr::new("CONTROL_PLANE_API_KEY")].unwrap(),
        "test-only-api-key"
    );

    let changed = config
        .update(&fixture.key_path(), save("tunnel_changed", None))
        .unwrap();
    assert_eq!(changed.as_deref(), Some("tunnel_changed"));
    assert_eq!(
        fixture.load(Some("tunnel_changed")).saved.unwrap().api_key,
        "test-only-api-key"
    );

    assert_eq!(
        config
            .update(&fixture.key_path(), TunnelConfigRequest::UseEnvironment)
            .unwrap(),
        None
    );
    assert!(!fixture.key_path().exists());
    assert_eq!(
        fixture.load(None).snapshot().source,
        TunnelConfigSource::Environment
    );
}

#[cfg(windows)]
#[test]
fn legacy_plaintext_is_converted_once_to_rc5_dpapi_storage() {
    let fixture = Fixture::new();
    std::fs::write(
        fixture.legacy_path(),
        br#"{"tunnel_id":"tunnel_legacy","api_key":"legacy-test-key"}"#,
    )
    .unwrap();

    let config = fixture.load(Some("tunnel_legacy"));
    assert_eq!(config.snapshot().source, TunnelConfigSource::File);
    assert!(!fixture.legacy_path().exists());
    let encoded = std::fs::read(fixture.key_path()).unwrap();
    assert!(encoded.starts_with(TUNNEL_KEY_MAGIC));
    assert!(!encoded
        .windows(b"legacy-test-key".len())
        .any(|window| window == b"legacy-test-key"));
}

#[cfg(windows)]
#[test]
fn invalid_saved_secret_fails_closed_and_can_be_repaired() {
    let fixture = Fixture::new();
    std::fs::write(fixture.key_path(), b"not-a-dpapi-secret").unwrap();
    let mut config = fixture.load(Some("tunnel_saved"));
    assert_eq!(config.snapshot().source, TunnelConfigSource::Invalid);
    assert!(!config.snapshot().is_configured());
    let error = config
        .apply_to_command(&mut Command::new("unused"))
        .unwrap_err();
    assert!(!error.to_string().contains("not-a-dpapi-secret"));

    config
        .update(&fixture.key_path(), save("tunnel_fixed", Some("test-key")))
        .unwrap();
    assert!(fixture
        .load(Some("tunnel_fixed"))
        .snapshot()
        .is_configured());

    std::fs::write(
        fixture.key_path(),
        vec![b' '; MAX_SECRET_BYTES as usize + 1],
    )
    .unwrap();
    assert_eq!(
        fixture.load(Some("tunnel_fixed")).snapshot().source,
        TunnelConfigSource::Invalid
    );
}

#[cfg(windows)]
#[tokio::test]
async fn saved_settings_survive_restart_without_secret_projection() {
    let fixture = Fixture::new();
    let app = crate::state::AppState::new(fixture.0.clone(), fixture.0.join("resources")).unwrap();
    let snapshot = app
        .update_tunnel_config(save("tunnel_persisted", Some("test-only-secret")))
        .await
        .unwrap();
    assert_eq!(
        snapshot.openai_tunnel_config.source,
        TunnelConfigSource::File
    );
    assert_eq!(
        snapshot.openai_tunnel_config.saved_tunnel_id.as_deref(),
        Some("tunnel_persisted")
    );

    let public_state = serde_json::to_string(&app.get_state()).unwrap();
    let stored_state = std::fs::read_to_string(fixture.0.join("desktop-state.json")).unwrap();
    let secret_file = std::fs::read(fixture.key_path()).unwrap();
    assert!(!public_state.contains("test-only-secret"));
    assert!(!stored_state.contains("test-only-secret"));
    assert!(!secret_file
        .windows(b"test-only-secret".len())
        .any(|window| window == b"test-only-secret"));
    assert!(stored_state.contains("openai_tunnel_id"));

    let restarted =
        crate::state::AppState::new(fixture.0.clone(), fixture.0.join("resources")).unwrap();
    assert_eq!(
        restarted.get_state().openai_tunnel_config.source,
        TunnelConfigSource::File
    );
    restarted
        .update_tunnel_config(TunnelConfigRequest::UseEnvironment)
        .await
        .unwrap();
    assert_eq!(
        restarted.get_state().openai_tunnel_config.source,
        TunnelConfigSource::Environment
    );
    assert!(!fixture.key_path().exists());
    assert!(
        !std::fs::read_to_string(fixture.0.join("desktop-state.json"))
            .unwrap()
            .contains("openai_tunnel_id")
    );
}
