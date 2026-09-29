use crate::error::{DesktopError, DesktopResult};
use crate::models::{LicenseSnapshot, LicenseStatus};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use ring::rand::SecureRandom as _;
use ring::signature::{UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

const LICENSE_VERSION: u32 = 1;
const LICENSE_PRODUCT: &str = "webcodex-deployment";
const LICENSE_FILE_NAME: &str = "license.json";
const DEVICE_IDENTITY_FILE_NAME: &str = "device_identity.dpapi";
pub(crate) const WINDOWS_DEVICE_ACTIVATION_DIR_NAME: &str = "CodexLinkSecretKey";
const WINDOWS_PREVIOUS_DEVICE_ACTIVATION_DIR_NAME: &str = "CodexLink.DeviceActivation.v1";
const DEVICE_IDENTITY_MAGIC: &[u8; 6] = b"WCDL1\0";
const DEVICE_SECRET_BYTES: usize = 32;
const DEVICE_ID_PREFIX: &str = "wcd-";
const ACTIVATION_URL: &str = "https://deploy.syzdq.xyz/api/activate";
const LICENSE_PUBLIC_KEY_B64URL: &str = "rqXd_KnEWjUubLQ_2qW-FQ20HcInvwyx4lpGFhjfGpk";
const LICENSE_FILE_MAX_BYTES: u64 = 16 * 1024;
const DEVICE_IDENTITY_MAX_BYTES: u64 = 16 * 1024;
const ACTIVATION_RESPONSE_MAX_BYTES: usize = 64 * 1024;
const ACTIVATION_TIMEOUT: Duration = Duration::from_secs(15);
const LICENSE_STATUS_URL: &str = "https://deploy.syzdq.xyz/api/license/status";
// The status reply is signed with the license key, so it carries its own
// purpose and key set and can never be mistaken for a license.
const LICENSE_STATUS_PURPOSE: &str = "license_status";
const LICENSE_STATUS_VERSION: u32 = 1;
const LICENSE_STATUS_TIMEOUT: Duration = Duration::from_secs(8);
const LICENSE_STATUS_NONCE_BYTES: usize = 32;
const LICENSE_REVOKED_REASON: &str = "license_revoked";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SignedLicense {
    license_version: u32,
    license_id: String,
    product: String,
    activation_code_id: String,
    device_id: String,
    issued_at: String,
    signature: String,
}

#[derive(Serialize)]
struct LicenseSigningPayload<'a> {
    activation_code_id: &'a str,
    device_id: &'a str,
    issued_at: &'a str,
    license_id: &'a str,
    license_version: u32,
    product: &'a str,
}

#[derive(Serialize)]
struct ActivationRequest<'a> {
    code: &'a str,
    device_id: &'a str,
    product: &'static str,
}

#[derive(Deserialize)]
struct ActivationSuccess {
    license: SignedLicense,
}

#[derive(Deserialize)]
struct ActivationFailure {
    error: String,
}

#[derive(Serialize)]
struct LicenseStatusRequest<'a> {
    license_id: &'a str,
    device_id: &'a str,
    nonce: &'a str,
}

#[derive(Deserialize)]
struct LicenseStatusSuccess {
    status: SignedLicenseStatus,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedLicenseStatus {
    purpose: String,
    status_version: u32,
    license_id: String,
    device_id: String,
    nonce: String,
    state: String,
    checked_at: String,
    signature: String,
}

#[derive(Serialize)]
struct LicenseStatusSigningPayload<'a> {
    checked_at: &'a str,
    device_id: &'a str,
    license_id: &'a str,
    nonce: &'a str,
    purpose: &'a str,
    state: &'a str,
    status_version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemoteLicenseState {
    Active,
    Revoked,
}

/// The verified license on this device, as the status check identifies it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LicenseBinding {
    license_id: String,
    device_id: String,
}

#[derive(Clone)]
struct DeviceIdentity {
    device_id: String,
}

pub(crate) struct LicenseManager {
    license_dir: PathBuf,
    snapshot: LicenseSnapshot,
    binding: Option<LicenseBinding>,
}

impl LicenseManager {
    pub fn load(data_dir: &Path) -> Self {
        let license_dir = persistent_license_dir(data_dir);
        let (snapshot, binding) = match migrate_legacy_license_if_needed(data_dir, &license_dir) {
            Ok(()) => match load_saved_license(&license_dir) {
                Ok(Some(binding)) => (activated_snapshot(), Some(binding)),
                Ok(None) => (not_activated_snapshot(), None),
                Err(reason) => (invalid_snapshot(reason), None),
            },
            Err(_) => (invalid_snapshot("license_state_unavailable"), None),
        };
        Self {
            license_dir,
            snapshot,
            binding,
        }
    }

    pub fn snapshot(&self) -> LicenseSnapshot {
        self.snapshot.clone()
    }

    pub fn status_binding(&self) -> Option<LicenseBinding> {
        self.binding.clone().filter(|_| self.snapshot.is_active())
    }

    /// Drops the saved license after the service signed its revocation. The
    /// device identity stays, so a restored code re-activates this same device.
    pub fn revoke(&mut self, revoked: &LicenseBinding) -> LicenseSnapshot {
        if self.binding.as_ref() != Some(revoked) {
            return self.snapshot();
        }
        // A failed delete still stops this launch; the next launch asks again.
        let _ = remove_saved_license(&self.license_dir);
        self.binding = None;
        self.snapshot = LicenseSnapshot {
            status: LicenseStatus::NotActivated,
            reason_code: Some(LICENSE_REVOKED_REASON.to_string()),
        };
        self.snapshot()
    }

    pub async fn activate(&mut self, raw_code: &str) -> DesktopResult<LicenseSnapshot> {
        let code = normalize_activation_code(raw_code)?;
        let identity = load_or_create_identity(&self.license_dir)?;
        let verifying_key = production_verifying_key().map_err(|_| license_response_invalid())?;

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(ACTIVATION_TIMEOUT)
            .build()
            .map_err(|_| license_service_unreachable())?;
        let response = client
            .post(ACTIVATION_URL)
            .json(&ActivationRequest {
                code: &code,
                device_id: &identity.device_id,
                product: LICENSE_PRODUCT,
            })
            .send()
            .await
            .map_err(|_| license_service_unreachable())?;
        let status = response.status();
        let bytes = read_bounded_response(response).await?;

        if !status.is_success() {
            let failure = serde_json::from_slice::<ActivationFailure>(&bytes)
                .map_err(|_| license_response_invalid())?;
            return Err(activation_failure(&failure.error));
        }

        let success = serde_json::from_slice::<ActivationSuccess>(&bytes)
            .map_err(|_| license_response_invalid())?;
        verify_signed_license(&success.license, &identity.device_id, &verifying_key)
            .map_err(|_| license_response_invalid())?;
        persist_signed_license(&self.license_dir, &success.license)
            .map_err(|_| license_store_failed())?;

        self.binding = Some(LicenseBinding {
            license_id: success.license.license_id,
            device_id: identity.device_id,
        });
        self.snapshot = activated_snapshot();
        Ok(self.snapshot())
    }
}

/// Asks the license service whether `binding` is still active. Every failure
/// answers `None`: the check must never lock out a buyer who is offline.
pub(crate) async fn fetch_license_status(binding: &LicenseBinding) -> Option<RemoteLicenseState> {
    let verifying_key = production_verifying_key().ok()?;
    let nonce = generate_status_nonce().ok()?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(LICENSE_STATUS_TIMEOUT)
        .build()
        .ok()?;
    let response = client
        .post(LICENSE_STATUS_URL)
        .json(&LicenseStatusRequest {
            license_id: &binding.license_id,
            device_id: &binding.device_id,
            nonce: &nonce,
        })
        .send()
        .await
        .ok()?;
    if response.status() != reqwest::StatusCode::OK {
        return None;
    }
    let bytes = read_bounded_response(response).await.ok()?;
    parse_license_status(&bytes, binding, &nonce, &verifying_key)
}

fn parse_license_status(
    body: &[u8],
    binding: &LicenseBinding,
    nonce: &str,
    verifying_key: &[u8; 32],
) -> Option<RemoteLicenseState> {
    let success = serde_json::from_slice::<LicenseStatusSuccess>(body).ok()?;
    verify_license_status(&success.status, binding, nonce, verifying_key).ok()
}

/// Reads the saved license: `None` when this device was never activated,
/// the binding when it verifies, otherwise the reason it cannot be trusted.
fn load_saved_license(license_dir: &Path) -> Result<Option<LicenseBinding>, &'static str> {
    let dir_metadata = match fs::symlink_metadata(&license_dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("license_state_unavailable"),
    };
    if !dir_metadata.is_dir() {
        return Err("license_state_invalid");
    }

    let identity = load_identity(&license_dir)?;
    let license_path = license_dir.join(LICENSE_FILE_NAME);
    let license_metadata = match fs::symlink_metadata(&license_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("license_state_unavailable"),
    };
    if !license_metadata.is_file() || license_metadata.len() > LICENSE_FILE_MAX_BYTES {
        return Err("license_file_invalid");
    }
    let Some(identity) = identity else {
        return Err("device_identity_missing");
    };

    let bytes = read_bounded_file(&license_path, LICENSE_FILE_MAX_BYTES)
        .map_err(|_| "license_file_invalid")?;
    let signed =
        serde_json::from_slice::<SignedLicense>(&bytes).map_err(|_| "license_file_invalid")?;
    let verifying_key = production_verifying_key().map_err(|_| "license_verifier_unavailable")?;
    verify_signed_license(&signed, &identity.device_id, &verifying_key)
        .map_err(|_| "license_invalid")?;
    Ok(Some(LicenseBinding {
        license_id: signed.license_id,
        device_id: identity.device_id,
    }))
}

fn remove_saved_license(license_dir: &Path) -> io::Result<()> {
    let path = license_dir.join(LICENSE_FILE_NAME);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() => fs::remove_file(&path),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "license destination is not a regular file",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn activated_snapshot() -> LicenseSnapshot {
    LicenseSnapshot {
        status: LicenseStatus::Activated,
        reason_code: None,
    }
}

fn not_activated_snapshot() -> LicenseSnapshot {
    LicenseSnapshot {
        status: LicenseStatus::NotActivated,
        reason_code: None,
    }
}

fn invalid_snapshot(reason: &str) -> LicenseSnapshot {
    LicenseSnapshot {
        status: LicenseStatus::Invalid,
        reason_code: Some(reason.to_string()),
    }
}

fn verify_signed_license(
    signed: &SignedLicense,
    expected_device_id: &str,
    verifying_key: &[u8; 32],
) -> Result<(), ()> {
    if signed.license_version != LICENSE_VERSION
        || signed.product != LICENSE_PRODUCT
        || signed.device_id != expected_device_id
        || !bounded_identifier(&signed.license_id, 128)
        || !bounded_identifier(&signed.activation_code_id, 128)
        || signed.issued_at.is_empty()
        || signed.issued_at.len() > 128
        || !signed.issued_at.is_ascii()
    {
        return Err(());
    }
    let signature = URL_SAFE_NO_PAD
        .decode(signed.signature.as_bytes())
        .map_err(|_| ())?;
    UnparsedPublicKey::new(&ED25519, verifying_key)
        .verify(&license_signing_message(signed)?, &signature)
        .map_err(|_| ())
}

fn license_signing_message(signed: &SignedLicense) -> Result<Vec<u8>, ()> {
    serde_json::to_vec(&LicenseSigningPayload {
        activation_code_id: &signed.activation_code_id,
        device_id: &signed.device_id,
        issued_at: &signed.issued_at,
        license_id: &signed.license_id,
        license_version: signed.license_version,
        product: &signed.product,
    })
    .map_err(|_| ())
}

fn verify_license_status(
    status: &SignedLicenseStatus,
    binding: &LicenseBinding,
    nonce: &str,
    verifying_key: &[u8; 32],
) -> Result<RemoteLicenseState, ()> {
    let state = match status.state.as_str() {
        "active" => RemoteLicenseState::Active,
        "revoked" => RemoteLicenseState::Revoked,
        _ => return Err(()),
    };
    if status.purpose != LICENSE_STATUS_PURPOSE
        || status.status_version != LICENSE_STATUS_VERSION
        || status.license_id != binding.license_id
        || status.device_id != binding.device_id
        || status.nonce != nonce
        || status.checked_at.is_empty()
        || status.checked_at.len() > 128
        || !status.checked_at.is_ascii()
    {
        return Err(());
    }
    let signature = URL_SAFE_NO_PAD
        .decode(status.signature.as_bytes())
        .map_err(|_| ())?;
    UnparsedPublicKey::new(&ED25519, verifying_key)
        .verify(&license_status_signing_message(status)?, &signature)
        .map_err(|_| ())?;
    Ok(state)
}

fn license_status_signing_message(status: &SignedLicenseStatus) -> Result<Vec<u8>, ()> {
    serde_json::to_vec(&LicenseStatusSigningPayload {
        checked_at: &status.checked_at,
        device_id: &status.device_id,
        license_id: &status.license_id,
        nonce: &status.nonce,
        purpose: &status.purpose,
        state: &status.state,
        status_version: status.status_version,
    })
    .map_err(|_| ())
}

fn generate_status_nonce() -> Result<String, ()> {
    let mut bytes = [0u8; LICENSE_STATUS_NONCE_BYTES];
    ring::rand::SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| ())?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn bounded_identifier(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn normalize_activation_code(raw: &str) -> DesktopResult<String> {
    let value = raw.trim().to_ascii_uppercase();
    if value.len() < 10
        || value.len() > 32
        || !value.starts_with("MYWS-")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(activation_code_invalid());
    }
    Ok(value)
}

fn production_verifying_key() -> Result<[u8; 32], ()> {
    URL_SAFE_NO_PAD
        .decode(LICENSE_PUBLIC_KEY_B64URL.as_bytes())
        .map_err(|_| ())?
        .try_into()
        .map_err(|_| ())
}

#[cfg(windows)]
fn persistent_license_dir(data_dir: &Path) -> PathBuf {
    data_dir
        .parent()
        .map(|parent| parent.join(WINDOWS_DEVICE_ACTIVATION_DIR_NAME))
        .unwrap_or_else(|| data_dir.join("license"))
}

#[cfg(not(windows))]
fn persistent_license_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("license")
}

#[cfg(windows)]
fn migrate_legacy_license_if_needed(data_dir: &Path, license_dir: &Path) -> io::Result<()> {
    if let Some(parent) = data_dir.parent() {
        migrate_license_dir_if_needed(
            &parent.join(WINDOWS_PREVIOUS_DEVICE_ACTIVATION_DIR_NAME),
            license_dir,
        )?;
    }
    migrate_license_dir_if_needed(&data_dir.join("license"), license_dir)
}

#[cfg(windows)]
fn migrate_license_dir_if_needed(legacy_dir: &Path, license_dir: &Path) -> io::Result<()> {
    if legacy_dir == license_dir {
        return Ok(());
    }

    match fs::symlink_metadata(legacy_dir) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "legacy license state is not a real directory",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    }

    let identity = read_optional_regular_file(
        &legacy_dir.join(DEVICE_IDENTITY_FILE_NAME),
        DEVICE_IDENTITY_MAX_BYTES,
    )?;
    let license =
        read_optional_regular_file(&legacy_dir.join(LICENSE_FILE_NAME), LICENSE_FILE_MAX_BYTES)?;
    let (Some(identity), Some(license)) = (identity, license) else {
        return Ok(());
    };

    let existing_identity = read_optional_regular_file(
        &license_dir.join(DEVICE_IDENTITY_FILE_NAME),
        DEVICE_IDENTITY_MAX_BYTES,
    )?;
    let existing_license =
        read_optional_regular_file(&license_dir.join(LICENSE_FILE_NAME), LICENSE_FILE_MAX_BYTES)?;
    ensure_license_directory(license_dir)?;
    match (existing_identity, existing_license) {
        (None, None) => {
            crate::state::write_atomic_file(&license_dir.join(LICENSE_FILE_NAME), &license)?;
            crate::state::write_atomic_file(
                &license_dir.join(DEVICE_IDENTITY_FILE_NAME),
                &identity,
            )?;
        }
        (Some(existing_identity), None) if existing_identity == identity => {
            crate::state::write_atomic_file(&license_dir.join(LICENSE_FILE_NAME), &license)?;
        }
        (None, Some(existing_license)) if existing_license == license => {
            crate::state::write_atomic_file(
                &license_dir.join(DEVICE_IDENTITY_FILE_NAME),
                &identity,
            )?;
        }
        _ => {}
    }
    Ok(())
}

#[cfg(not(windows))]
fn migrate_legacy_license_if_needed(_data_dir: &Path, _license_dir: &Path) -> io::Result<()> {
    Ok(())
}

fn load_or_create_identity(license_dir: &Path) -> DesktopResult<DeviceIdentity> {
    ensure_license_directory(license_dir).map_err(|_| device_identity_unavailable())?;
    match load_identity(license_dir) {
        Ok(Some(identity)) => Ok(identity),
        Ok(None) => {
            let secret = generate_device_secret().map_err(|_| device_identity_unavailable())?;
            persist_identity(&license_dir, &secret).map_err(|_| license_store_failed())?;
            Ok(DeviceIdentity {
                device_id: derive_device_id(&secret),
            })
        }
        Err("device_identity_missing") => Err(device_identity_missing()),
        Err("device_identity_invalid") => Err(device_identity_invalid()),
        Err(_) => Err(device_identity_unavailable()),
    }
}

fn load_identity(license_dir: &Path) -> Result<Option<DeviceIdentity>, &'static str> {
    let path = license_dir.join(DEVICE_IDENTITY_FILE_NAME);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("device_identity_unavailable"),
    };
    if !metadata.is_file() || metadata.len() > DEVICE_IDENTITY_MAX_BYTES {
        return Err("device_identity_invalid");
    }
    let bytes = read_bounded_file(&path, DEVICE_IDENTITY_MAX_BYTES)
        .map_err(|_| "device_identity_unavailable")?;
    if bytes.len() <= DEVICE_IDENTITY_MAGIC.len()
        || &bytes[..DEVICE_IDENTITY_MAGIC.len()] != DEVICE_IDENTITY_MAGIC
    {
        return Err("device_identity_invalid");
    }
    let secret =
        crate::secret_protection::unprotect_current_user(&bytes[DEVICE_IDENTITY_MAGIC.len()..])
            .map_err(|_| "device_identity_invalid")?;
    let secret: [u8; DEVICE_SECRET_BYTES] =
        secret.try_into().map_err(|_| "device_identity_invalid")?;
    Ok(Some(DeviceIdentity {
        device_id: derive_device_id(&secret),
    }))
}

fn persist_identity(license_dir: &Path, secret: &[u8; DEVICE_SECRET_BYTES]) -> io::Result<()> {
    let protected = crate::secret_protection::protect_current_user(secret)
        .map_err(|_| io::Error::other("device identity protection failed"))?;
    let mut encoded = Vec::with_capacity(DEVICE_IDENTITY_MAGIC.len() + protected.len());
    encoded.extend_from_slice(DEVICE_IDENTITY_MAGIC);
    encoded.extend_from_slice(&protected);
    if encoded.len() as u64 > DEVICE_IDENTITY_MAX_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "device identity exceeds bounded size",
        ));
    }
    let destination = license_dir.join(DEVICE_IDENTITY_FILE_NAME);
    reject_non_file_destination(&destination)?;
    crate::state::write_atomic_file(&destination, &encoded)
}

fn persist_signed_license(license_dir: &Path, signed: &SignedLicense) -> io::Result<()> {
    let encoded = serde_json::to_vec_pretty(signed)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "license serialization failed"))?;
    if encoded.len() as u64 > LICENSE_FILE_MAX_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "license exceeds bounded size",
        ));
    }
    ensure_license_directory(license_dir)?;
    let destination = license_dir.join(LICENSE_FILE_NAME);
    reject_non_file_destination(&destination)?;
    crate::state::write_atomic_file(&destination, &encoded)
}

fn reject_non_file_destination(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "license destination is not a regular file",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn ensure_license_directory(path: &Path) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "device activation directory has no parent",
        )
    })?;
    ensure_real_directory(parent)?;
    ensure_real_directory(path)
}

fn read_optional_regular_file(path: &Path, limit: u64) -> io::Result<Option<Vec<u8>>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !metadata.is_file() || metadata.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "device activation source is not a bounded regular file",
        ));
    }
    read_bounded_file(path, limit).map(Some)
}

fn ensure_real_directory(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "license state directory is not a real directory",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(path)?;
            if fs::symlink_metadata(path)?.is_dir() {
                Ok(())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "license state directory is not a real directory",
                ))
            }
        }
        Err(error) => Err(error),
    }
}

fn read_bounded_file(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let mut file = fs::File::open(path)?.take(limit.saturating_add(1));
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "license state exceeds bounded size",
        ));
    }
    Ok(bytes)
}

async fn read_bounded_response(mut response: reqwest::Response) -> DesktopResult<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > ACTIVATION_RESPONSE_MAX_BYTES as u64)
    {
        return Err(license_response_invalid());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| license_service_unreachable())?
    {
        if bytes.len().saturating_add(chunk.len()) > ACTIVATION_RESPONSE_MAX_BYTES {
            return Err(license_response_invalid());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn derive_device_id(secret: &[u8; DEVICE_SECRET_BYTES]) -> String {
    let digest = Sha256::digest(secret);
    let mut value = String::with_capacity(DEVICE_ID_PREFIX.len() + digest.len() * 2);
    value.push_str(DEVICE_ID_PREFIX);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut value, "{byte:02x}");
    }
    value
}

#[cfg(windows)]
fn generate_device_secret() -> Result<[u8; DEVICE_SECRET_BYTES], ()> {
    use windows_sys::Win32::Security::Cryptography::{
        BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG,
    };

    let mut secret = [0u8; DEVICE_SECRET_BYTES];
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            secret.as_mut_ptr(),
            secret.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status < 0 {
        Err(())
    } else {
        Ok(secret)
    }
}

#[cfg(not(windows))]
fn generate_device_secret() -> Result<[u8; DEVICE_SECRET_BYTES], ()> {
    Err(())
}

fn activation_failure(code: &str) -> DesktopError {
    match code {
        "activation_code_invalid" => activation_code_invalid(),
        "activation_code_bound_elsewhere" => DesktopError::new(
            "activation_code_bound_elsewhere",
            "This activation code is already bound to another device",
            "Use the device that was first activated, or purchase a new license for another device.",
        ),
        "activation_code_stopped" => DesktopError::new(
            "activation_code_stopped",
            "This activation code has been stopped",
            "Contact support@syzdq.xyz if you have questions about this license.",
        ),
        "license_rate_limited" => DesktopError::new(
            "license_rate_limited",
            "Too many activation attempts were made",
            "Wait a moment, then try again.",
        ),
        "license_service_unavailable" => DesktopError::new(
            "license_service_unavailable",
            "The CodexLink activation service is temporarily unavailable",
            "Try activation again later.",
        ),
        _ => DesktopError::new(
            "license_activation_failed",
            "CodexLink could not complete activation",
            "Check the activation code and network connection, then try again.",
        ),
    }
}

fn activation_code_invalid() -> DesktopError {
    DesktopError::new(
        "activation_code_invalid",
        "This sales activation code is not valid",
        "Check the code from your purchase message and try again.",
    )
}

fn license_service_unreachable() -> DesktopError {
    DesktopError::new(
        "license_service_unreachable",
        "CodexLink could not reach the activation service",
        "Check the network connection and try activation again.",
    )
}

fn license_response_invalid() -> DesktopError {
    DesktopError::new(
        "license_response_invalid",
        "The activation service returned an invalid license response",
        "Retry later or contact support if this persists.",
    )
}

fn license_store_failed() -> DesktopError {
    DesktopError::new(
        "license_store_failed",
        "CodexLink could not save local activation data",
        "Check access to the CodexLink device activation store and retry.",
    )
}

fn device_identity_missing() -> DesktopError {
    DesktopError::new(
        "device_identity_missing",
        "The local activation identity is missing",
        "Enter the original activation code again. If the device identity was deleted after binding, contact support.",
    )
}

fn device_identity_invalid() -> DesktopError {
    DesktopError::new(
        "device_identity_invalid",
        "The local activation identity is invalid",
        "Restore this device's local activation data or contact support.",
    )
}

fn device_identity_unavailable() -> DesktopError {
    DesktopError::new(
        "device_identity_unavailable",
        "The local activation identity cannot be opened by this Windows user",
        "Use the Windows account that activated CodexLink, or restore this device's local activation data.",
    )
}

pub(crate) fn license_required_error() -> DesktopError {
    DesktopError::new(
        "license_required",
        "CodexLink is not activated on this Windows account",
        "Enter the sales activation code before starting the CodexLink runtime.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    fn signing_key() -> Ed25519KeyPair {
        let rng = SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap()
    }

    fn signed_license(key: &Ed25519KeyPair, device_id: &str) -> SignedLicense {
        let mut signed = SignedLicense {
            license_version: LICENSE_VERSION,
            license_id: "11111111-2222-3333-4444-555555555555".into(),
            product: LICENSE_PRODUCT.into(),
            activation_code_id: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".into(),
            device_id: device_id.into(),
            issued_at: "2026-09-12T09:15:27Z".into(),
            signature: String::new(),
        };
        signed.signature = URL_SAFE_NO_PAD.encode(
            key.sign(&license_signing_message(&signed).unwrap())
                .as_ref(),
        );
        signed
    }

    #[test]
    fn rc5_signing_message_is_sorted_compact_json() {
        let license = SignedLicense {
            license_version: 1,
            license_id: "license-a".into(),
            product: "webcodex-deployment".into(),
            activation_code_id: "activation-a".into(),
            device_id: "wcd-device-a".into(),
            issued_at: "2026-09-12T09:15:27Z".into(),
            signature: "ignored".into(),
        };
        assert_eq!(
            String::from_utf8(license_signing_message(&license).unwrap()).unwrap(),
            r#"{"activation_code_id":"activation-a","device_id":"wcd-device-a","issued_at":"2026-09-12T09:15:27Z","license_id":"license-a","license_version":1,"product":"webcodex-deployment"}"#
        );
    }

    #[test]
    fn production_public_key_is_valid_ed25519() {
        assert!(production_verifying_key().is_ok());
    }

    #[test]
    fn signed_license_verifies_only_for_bound_device() {
        let key = signing_key();
        let signed = signed_license(&key, "wcd-device-a");
        let public_key: [u8; 32] = key.public_key().as_ref().try_into().unwrap();
        assert!(verify_signed_license(&signed, "wcd-device-a", &public_key).is_ok());
        assert!(verify_signed_license(&signed, "wcd-device-b", &public_key).is_err());
    }

    #[test]
    fn activation_code_validation_is_bounded_and_keeps_myws_prefix() {
        assert_eq!(
            normalize_activation_code(" myws-abcde-fghij-klmno-pqrst ").unwrap(),
            "MYWS-ABCDE-FGHIJ-KLMNO-PQRST"
        );
        assert!(normalize_activation_code("OTHER-ABCDE-FGHIJ-KLMNO-PQRST").is_err());
        assert!(normalize_activation_code(&format!("MYWS-{}", "A".repeat(40))).is_err());
    }

    #[test]
    fn device_id_matches_rc5_shape() {
        let id = derive_device_id(&[9u8; DEVICE_SECRET_BYTES]);
        assert!(id.starts_with("wcd-"));
        assert_eq!(id.len(), 68);
        assert!(id[4..].bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    #[test]
    fn activation_failures_preserve_rc5_error_codes() {
        for code in [
            "activation_code_invalid",
            "activation_code_bound_elsewhere",
            "activation_code_stopped",
            "license_rate_limited",
            "license_service_unavailable",
        ] {
            assert_eq!(activation_failure(code).code, code);
        }
        assert_eq!(
            activation_failure("unknown").code,
            "license_activation_failed"
        );
    }

    #[cfg(windows)]
    #[test]
    fn dpapi_identity_roundtrips_with_rc5_header() {
        let secret = [11u8; DEVICE_SECRET_BYTES];
        let protected = crate::secret_protection::protect_current_user(&secret).unwrap();
        let mut file = DEVICE_IDENTITY_MAGIC.to_vec();
        file.extend_from_slice(&protected);
        assert_eq!(&file[..DEVICE_IDENTITY_MAGIC.len()], DEVICE_IDENTITY_MAGIC);
        assert_eq!(
            crate::secret_protection::unprotect_current_user(&file[DEVICE_IDENTITY_MAGIC.len()..])
                .unwrap(),
            secret
        );
    }

    #[cfg(windows)]
    #[test]
    fn device_activation_store_is_outside_bundle_app_data() {
        let local = PathBuf::from(r"C:\Users\tester\AppData\Local");
        let app_data = local.join("dev.codexlink.desktop");
        assert_eq!(
            persistent_license_dir(&app_data),
            local.join(WINDOWS_DEVICE_ACTIVATION_DIR_NAME)
        );
        assert!(!persistent_license_dir(&app_data).starts_with(&app_data));
    }

    #[cfg(windows)]
    #[test]
    fn legacy_bundle_license_payload_migrates_to_device_activation_store() {
        let root = std::env::temp_dir().join(format!(
            "codexlink-license-migration-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let app_data = root.join("dev.codexlink.desktop");
        let legacy = app_data.join("license");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join(DEVICE_IDENTITY_FILE_NAME), b"legacy-device").unwrap();
        fs::write(legacy.join(LICENSE_FILE_NAME), b"legacy-license").unwrap();

        let target = persistent_license_dir(&app_data);
        migrate_legacy_license_if_needed(&app_data, &target).unwrap();
        assert_eq!(
            fs::read(target.join(DEVICE_IDENTITY_FILE_NAME)).unwrap(),
            b"legacy-device"
        );
        assert_eq!(
            fs::read(target.join(LICENSE_FILE_NAME)).unwrap(),
            b"legacy-license"
        );

        fs::remove_dir_all(&app_data).unwrap();
        assert_eq!(
            fs::read(target.join(DEVICE_IDENTITY_FILE_NAME)).unwrap(),
            b"legacy-device"
        );
        assert_eq!(
            fs::read(target.join(LICENSE_FILE_NAME)).unwrap(),
            b"legacy-license"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn previous_device_activation_store_migrates_to_codexlink_secret_key_store() {
        let root = std::env::temp_dir().join(format!(
            "codexlink-secret-key-migration-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let app_data = root.join("dev.codexlink.desktop");
        let previous = root.join(WINDOWS_PREVIOUS_DEVICE_ACTIVATION_DIR_NAME);
        fs::create_dir_all(&previous).unwrap();
        fs::write(previous.join(DEVICE_IDENTITY_FILE_NAME), b"previous-device").unwrap();
        fs::write(previous.join(LICENSE_FILE_NAME), b"previous-license").unwrap();

        let target = persistent_license_dir(&app_data);
        migrate_legacy_license_if_needed(&app_data, &target).unwrap();
        assert_eq!(target, root.join("CodexLinkSecretKey"));
        assert_eq!(
            fs::read(target.join(DEVICE_IDENTITY_FILE_NAME)).unwrap(),
            b"previous-device"
        );
        assert_eq!(
            fs::read(target.join(LICENSE_FILE_NAME)).unwrap(),
            b"previous-license"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_signature_fails_closed() {
        let key = signing_key();
        let mut signed = signed_license(&key, "wcd-device-a");
        signed.signature = URL_SAFE_NO_PAD.encode([0u8; 64]);
        let public_key: [u8; 32] = key.public_key().as_ref().try_into().unwrap();
        assert!(verify_signed_license(&signed, "wcd-device-a", &public_key).is_err());
    }

    // Signed by license-service `sign_license_status` with a throwaway test key
    // (seed 0x07 * 32), so these pin the wire format both sides agree on.
    const STATUS_TEST_PUBLIC_KEY: &str = "6kpsY-KcUgq-9VB7Ey7F-ZVHdq6-vnuSQh7qaRRG0iw";
    const STATUS_TEST_NONCE: &str = "nonce-fixture-0123456789";
    const SERVER_ACTIVE_REPLY: &str = r#"{"ok":true,"status":{"purpose":"license_status","status_version":1,"license_id":"11111111-2222-3333-4444-555555555555","device_id":"wcd-abababababababababababababababababababababababababababababababab","nonce":"nonce-fixture-0123456789","state":"active","checked_at":"2026-09-29T08:00:00Z","signature":"WicIb4l2iKxbRh_PG7p6NhX5wxfDfC9RgkEx4I2uxhym8SVsT_rTBrwo8H4FRW5bj5AH27ZWdHQJXpvKhCyLDw"}}"#;
    const SERVER_REVOKED_REPLY: &str = r#"{"ok":true,"status":{"purpose":"license_status","status_version":1,"license_id":"11111111-2222-3333-4444-555555555555","device_id":"wcd-abababababababababababababababababababababababababababababababab","nonce":"nonce-fixture-0123456789","state":"revoked","checked_at":"2026-09-29T08:00:00Z","signature":"L_DaHUrgg8wszV6tFUePRRZRlWJ7zivFEU2t6Axqi5-ycrv9SPQj0OlJepygYUJz9jmvyBiw3y1i_4bZoZ5mDQ"}}"#;

    fn status_test_key() -> [u8; 32] {
        URL_SAFE_NO_PAD
            .decode(STATUS_TEST_PUBLIC_KEY)
            .unwrap()
            .try_into()
            .unwrap()
    }

    fn status_test_binding() -> LicenseBinding {
        LicenseBinding {
            license_id: "11111111-2222-3333-4444-555555555555".into(),
            device_id: format!("wcd-{}", "ab".repeat(32)),
        }
    }

    fn server_status(reply: &str) -> SignedLicenseStatus {
        serde_json::from_str::<LicenseStatusSuccess>(reply)
            .unwrap()
            .status
    }

    #[test]
    fn status_signing_message_matches_server_canonical_payload() {
        assert_eq!(
            String::from_utf8(
                license_status_signing_message(&server_status(SERVER_REVOKED_REPLY)).unwrap()
            )
            .unwrap(),
            r#"{"checked_at":"2026-09-29T08:00:00Z","device_id":"wcd-abababababababababababababababababababababababababababababababab","license_id":"11111111-2222-3333-4444-555555555555","nonce":"nonce-fixture-0123456789","purpose":"license_status","state":"revoked","status_version":1}"#
        );
    }

    #[test]
    fn server_signed_status_replies_verify() {
        let key = status_test_key();
        let binding = status_test_binding();
        assert_eq!(
            parse_license_status(
                SERVER_ACTIVE_REPLY.as_bytes(),
                &binding,
                STATUS_TEST_NONCE,
                &key
            ),
            Some(RemoteLicenseState::Active)
        );
        assert_eq!(
            parse_license_status(
                SERVER_REVOKED_REPLY.as_bytes(),
                &binding,
                STATUS_TEST_NONCE,
                &key
            ),
            Some(RemoteLicenseState::Revoked)
        );
    }

    #[test]
    fn status_reply_must_answer_this_request() {
        let key = status_test_key();
        let binding = status_test_binding();
        let reply = SERVER_REVOKED_REPLY.as_bytes();

        assert_eq!(
            parse_license_status(reply, &binding, "another-nonce-0123456789", &key),
            None
        );
        let other_license = LicenseBinding {
            license_id: "99999999-2222-3333-4444-555555555555".into(),
            ..binding.clone()
        };
        assert_eq!(
            parse_license_status(reply, &other_license, STATUS_TEST_NONCE, &key),
            None
        );
        let other_device = LicenseBinding {
            device_id: format!("wcd-{}", "cd".repeat(32)),
            ..binding.clone()
        };
        assert_eq!(
            parse_license_status(reply, &other_device, STATUS_TEST_NONCE, &key),
            None
        );
        let production = production_verifying_key().unwrap();
        assert_eq!(
            parse_license_status(reply, &binding, STATUS_TEST_NONCE, &production),
            None
        );
    }

    #[test]
    fn tampered_or_foreign_status_documents_are_ignored() {
        let key = status_test_key();
        let binding = status_test_binding();
        let flipped = SERVER_ACTIVE_REPLY.replace(r#""state":"active""#, r#""state":"revoked""#);
        assert_eq!(
            parse_license_status(flipped.as_bytes(), &binding, STATUS_TEST_NONCE, &key),
            None
        );

        let mut status = server_status(SERVER_REVOKED_REPLY);
        status.purpose = "license".into();
        assert!(verify_license_status(&status, &binding, STATUS_TEST_NONCE, &key).is_err());
        let mut status = server_status(SERVER_REVOKED_REPLY);
        status.status_version = 2;
        assert!(verify_license_status(&status, &binding, STATUS_TEST_NONCE, &key).is_err());
        let mut status = server_status(SERVER_REVOKED_REPLY);
        status.state = "suspended".into();
        assert!(verify_license_status(&status, &binding, STATUS_TEST_NONCE, &key).is_err());

        let extra_field = SERVER_REVOKED_REPLY
            .replace(r#""state":"revoked""#, r#""state":"revoked","reason":"x""#);
        assert_eq!(
            parse_license_status(extra_field.as_bytes(), &binding, STATUS_TEST_NONCE, &key),
            None
        );
        let license =
            serde_json::to_string(&signed_license(&signing_key(), &binding.device_id)).unwrap();
        let license_as_status = format!(r#"{{"ok":true,"status":{license}}}"#);
        assert_eq!(
            parse_license_status(
                license_as_status.as_bytes(),
                &binding,
                STATUS_TEST_NONCE,
                &key
            ),
            None
        );
        let unknown = r#"{"ok":false,"error":"license_unknown"}"#;
        assert_eq!(
            parse_license_status(unknown.as_bytes(), &binding, STATUS_TEST_NONCE, &key),
            None
        );
    }

    #[test]
    fn status_nonce_is_fresh_and_matches_server_shape() {
        let first = generate_status_nonce().unwrap();
        let second = generate_status_nonce().unwrap();
        assert_ne!(first, second);
        for nonce in [first, second] {
            assert_eq!(nonce.len(), 43);
            assert!(nonce
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')));
        }
    }

    fn revocation_fixture(name: &str) -> (PathBuf, LicenseManager) {
        let license_dir = std::env::temp_dir().join(format!(
            "codexlink-license-revoke-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&license_dir).unwrap();
        fs::write(
            license_dir.join(DEVICE_IDENTITY_FILE_NAME),
            b"device-identity",
        )
        .unwrap();
        fs::write(license_dir.join(LICENSE_FILE_NAME), b"signed-license").unwrap();
        let manager = LicenseManager {
            license_dir: license_dir.clone(),
            snapshot: activated_snapshot(),
            binding: Some(status_test_binding()),
        };
        (license_dir, manager)
    }

    #[test]
    fn revocation_deletes_only_the_license_and_reports_why() {
        let (license_dir, mut manager) = revocation_fixture("match");
        assert_eq!(manager.status_binding(), Some(status_test_binding()));

        let snapshot = manager.revoke(&status_test_binding());
        assert_eq!(snapshot.status, LicenseStatus::NotActivated);
        assert_eq!(snapshot.reason_code.as_deref(), Some("license_revoked"));
        assert_eq!(manager.snapshot(), snapshot);
        assert_eq!(manager.status_binding(), None);
        assert!(!license_dir.join(LICENSE_FILE_NAME).exists());
        assert_eq!(
            fs::read(license_dir.join(DEVICE_IDENTITY_FILE_NAME)).unwrap(),
            b"device-identity"
        );

        fs::remove_dir_all(license_dir).unwrap();
    }

    #[test]
    fn revocation_for_another_license_changes_nothing() {
        let (license_dir, mut manager) = revocation_fixture("mismatch");
        let other = LicenseBinding {
            license_id: "99999999-2222-3333-4444-555555555555".into(),
            ..status_test_binding()
        };

        assert!(manager.revoke(&other).is_active());
        assert_eq!(manager.status_binding(), Some(status_test_binding()));
        assert!(license_dir.join(LICENSE_FILE_NAME).is_file());

        fs::remove_dir_all(license_dir).unwrap();
    }

    #[test]
    fn inactive_license_is_never_status_checked() {
        let manager = LicenseManager {
            license_dir: PathBuf::from("unused"),
            snapshot: invalid_snapshot("license_invalid"),
            binding: Some(status_test_binding()),
        };
        assert_eq!(manager.status_binding(), None);
    }
}
