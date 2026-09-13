use crate::error::{DesktopError, DesktopResult};
use crate::models::{LicenseSnapshot, LicenseStatus};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
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
const DEVICE_IDENTITY_MAGIC: &[u8; 6] = b"WCDL1\0";
const DEVICE_SECRET_BYTES: usize = 32;
const DEVICE_ID_PREFIX: &str = "wcd-";
const ACTIVATION_URL: &str = "https://deploy.syzdq.xyz/api/activate";
const LICENSE_PUBLIC_KEY_B64URL: &str = "rqXd_KnEWjUubLQ_2qW-FQ20HcInvwyx4lpGFhjfGpk";
const LICENSE_FILE_MAX_BYTES: u64 = 16 * 1024;
const DEVICE_IDENTITY_MAX_BYTES: u64 = 16 * 1024;
const ACTIVATION_RESPONSE_MAX_BYTES: usize = 64 * 1024;
const ACTIVATION_TIMEOUT: Duration = Duration::from_secs(15);

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

#[derive(Clone)]
struct DeviceIdentity {
    device_id: String,
}

pub(crate) struct LicenseManager {
    data_dir: PathBuf,
    snapshot: LicenseSnapshot,
}

impl LicenseManager {
    pub fn load(data_dir: &Path) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
            snapshot: inspect_saved_license(data_dir),
        }
    }

    pub fn snapshot(&self) -> LicenseSnapshot {
        self.snapshot.clone()
    }

    pub async fn activate(&mut self, raw_code: &str) -> DesktopResult<LicenseSnapshot> {
        let code = normalize_activation_code(raw_code)?;
        let identity = load_or_create_identity(&self.data_dir)?;
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
        persist_signed_license(&self.data_dir, &success.license)
            .map_err(|_| license_store_failed())?;

        self.snapshot = LicenseSnapshot {
            status: LicenseStatus::Activated,
            reason_code: None,
        };
        Ok(self.snapshot())
    }
}

fn inspect_saved_license(data_dir: &Path) -> LicenseSnapshot {
    let license_dir = data_dir.join("license");
    let dir_metadata = match fs::symlink_metadata(&license_dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return not_activated_snapshot();
        }
        Err(_) => return invalid_snapshot("license_state_unavailable"),
    };
    if !dir_metadata.is_dir() {
        return invalid_snapshot("license_state_invalid");
    }

    let identity = match load_identity(&license_dir) {
        Ok(identity) => identity,
        Err(reason) => return invalid_snapshot(reason),
    };
    let license_path = license_dir.join(LICENSE_FILE_NAME);
    let license_metadata = match fs::symlink_metadata(&license_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return not_activated_snapshot();
        }
        Err(_) => return invalid_snapshot("license_state_unavailable"),
    };
    if !license_metadata.is_file() || license_metadata.len() > LICENSE_FILE_MAX_BYTES {
        return invalid_snapshot("license_file_invalid");
    }
    let Some(identity) = identity else {
        return invalid_snapshot("device_identity_missing");
    };

    let bytes = match read_bounded_file(&license_path, LICENSE_FILE_MAX_BYTES) {
        Ok(bytes) => bytes,
        Err(_) => return invalid_snapshot("license_file_invalid"),
    };
    let signed = match serde_json::from_slice::<SignedLicense>(&bytes) {
        Ok(signed) => signed,
        Err(_) => return invalid_snapshot("license_file_invalid"),
    };
    let verifying_key = match production_verifying_key() {
        Ok(key) => key,
        Err(_) => return invalid_snapshot("license_verifier_unavailable"),
    };
    if verify_signed_license(&signed, &identity.device_id, &verifying_key).is_ok() {
        LicenseSnapshot {
            status: LicenseStatus::Activated,
            reason_code: None,
        }
    } else {
        invalid_snapshot("license_invalid")
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

fn load_or_create_identity(data_dir: &Path) -> DesktopResult<DeviceIdentity> {
    let license_dir = data_dir.join("license");
    ensure_real_directory(data_dir).map_err(|_| device_identity_unavailable())?;
    ensure_real_directory(&license_dir).map_err(|_| device_identity_unavailable())?;
    match load_identity(&license_dir) {
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

fn persist_signed_license(data_dir: &Path, signed: &SignedLicense) -> io::Result<()> {
    let encoded = serde_json::to_vec_pretty(signed)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "license serialization failed"))?;
    if encoded.len() as u64 > LICENSE_FILE_MAX_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "license exceeds bounded size",
        ));
    }
    ensure_real_directory(data_dir)?;
    let license_dir = data_dir.join("license");
    ensure_real_directory(&license_dir)?;
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
            "Use the device that was first activated, or contact support.",
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
        "Check access to the CodexLink application data directory and retry.",
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

    #[test]
    fn invalid_signature_fails_closed() {
        let key = signing_key();
        let mut signed = signed_license(&key, "wcd-device-a");
        signed.signature = URL_SAFE_NO_PAD.encode([0u8; 64]);
        let public_key: [u8; 32] = key.public_key().as_ref().try_into().unwrap();
        assert!(verify_signed_license(&signed, "wcd-device-a", &public_key).is_err());
    }
}
