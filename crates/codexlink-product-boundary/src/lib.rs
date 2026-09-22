use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use ring::signature::{UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const PRODUCT_MODE_DENIED: &str =
    "bundled CodexLink Core requires an activated CodexLink Desktop";

const ACTIVATION_FINGERPRINT_DOMAIN: &[u8; 16] = &[
    0x93, 0x2b, 0xd4, 0x71, 0x8c, 0x06, 0xee, 0x59, 0x31, 0xa7, 0xc2, 0x4d, 0xf0, 0x18, 0x65, 0xba,
];
const RUNTIME_SEAL_DOMAIN: &[u8; 16] = &[
    0x4f, 0xb8, 0x12, 0xe3, 0x76, 0x9d, 0x2a, 0xc5, 0x08, 0xf1, 0x6b, 0x34, 0xad, 0x57, 0x80, 0xde,
];
const CLI_STAGE_DOMAIN: &[u8; 16] = &[
    0xd1, 0x07, 0x5c, 0xa9, 0x3e, 0xf4, 0x82, 0x1b, 0x68, 0xcf, 0x25, 0x90, 0x47, 0xb3, 0xea, 0x6d,
];
const SERVER_STAGE_DOMAIN: &[u8; 16] = &[
    0x2a, 0xe6, 0x79, 0x0c, 0xb5, 0x43, 0xdf, 0x91, 0x14, 0x68, 0xc7, 0x3b, 0xf2, 0x85, 0x50, 0xad,
];
const RUNNER_STAGE_DOMAIN: &[u8; 16] = &[
    0x7c, 0x31, 0xe8, 0x56, 0x0b, 0xd2, 0x94, 0x6f, 0xa1, 0x45, 0x29, 0xfb, 0x83, 0x17, 0xc0, 0x5e,
];
static STARTUP_RUNTIME_SEAL: OnceLock<[u8; 32]> = OnceLock::new();

const LICENSE_VERSION: u32 = 1;
const LICENSE_PRODUCT: &str = "webcodex-deployment";
const LICENSE_FILE_NAME: &str = "license.json";
const DEVICE_IDENTITY_FILE_NAME: &str = "device_identity.dpapi";
const WINDOWS_DEVICE_ACTIVATION_DIR_NAME: &str = "CodexLinkSecretKey";
const DEVICE_IDENTITY_MAGIC: &[u8; 6] = b"WCDL1\0";
const DEVICE_SECRET_BYTES: usize = 32;
const DEVICE_ID_PREFIX: &str = "wcd-";
const LICENSE_PUBLIC_KEY_B64URL: &str = "rqXd_KnEWjUubLQ_2qW-FQ20HcInvwyx4lpGFhjfGpk";
const LICENSE_FILE_MAX_BYTES: u64 = 16 * 1024;
const DEVICE_IDENTITY_MAX_BYTES: u64 = 16 * 1024;

#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Operational,
    Diagnostic,
}

pub fn bundled_product_mode() -> bool {
    option_env!("CODEXLINK_BUNDLED_PRODUCT_MODE") == Some("1")
}

pub fn admit_current_process<I>(args: I) -> Result<Admission, String>
where
    I: IntoIterator<Item = OsString>,
{
    let args = args.into_iter().collect::<Vec<_>>();
    if is_exact_argument(&args, "--version") || is_exact_argument(&args, "-V") {
        return Ok(Admission::Diagnostic);
    }
    if !bundled_product_mode() {
        return Ok(Admission::Operational);
    }

    let fingerprint = verify_startup_activation().map_err(|_| PRODUCT_MODE_DENIED.to_string())?;
    if !current_process_has_product_parent() {
        return Err(PRODUCT_MODE_DENIED.to_string());
    }
    install_startup_runtime_seal(&fingerprint).map_err(|_| PRODUCT_MODE_DENIED.to_string())?;
    Ok(Admission::Operational)
}

#[inline(never)]
pub fn require_cli_runtime() -> Result<(), String> {
    require_operational_stage(CLI_STAGE_DOMAIN)
}

#[inline(never)]
pub fn require_server_runtime() -> Result<(), String> {
    require_operational_stage(SERVER_STAGE_DOMAIN)
}

#[inline(never)]
pub fn require_runner_runtime() -> Result<(), String> {
    require_operational_stage(RUNNER_STAGE_DOMAIN)
}

fn require_operational_stage(stage_domain: &[u8]) -> Result<(), String> {
    if !bundled_product_mode() {
        return Ok(());
    }
    let fingerprint = verify_stage_activation().map_err(|_| PRODUCT_MODE_DENIED.to_string())?;
    if !current_process_has_product_parent() {
        return Err(PRODUCT_MODE_DENIED.to_string());
    }
    let expected = runtime_seal(&fingerprint).map_err(|_| PRODUCT_MODE_DENIED.to_string())?;
    if stage_seal_matches(STARTUP_RUNTIME_SEAL.get(), &expected, stage_domain) {
        Ok(())
    } else {
        Err(PRODUCT_MODE_DENIED.to_string())
    }
}

fn install_startup_runtime_seal(fingerprint: &[u8; 32]) -> Result<(), ()> {
    let seal = runtime_seal(fingerprint)?;
    match STARTUP_RUNTIME_SEAL.set(seal) {
        Ok(()) => Ok(()),
        Err(seal) => STARTUP_RUNTIME_SEAL
            .get()
            .filter(|existing| fixed_32_eq(existing, &seal))
            .map(|_| ())
            .ok_or(()),
    }
}

fn stage_seal_matches(
    startup_seal: Option<&[u8; 32]>,
    expected_runtime_seal: &[u8; 32],
    stage_domain: &[u8],
) -> bool {
    let Some(startup_seal) = startup_seal else {
        return false;
    };
    let actual = derive_stage_proof(startup_seal, stage_domain);
    let expected = derive_stage_proof(expected_runtime_seal, stage_domain);
    fixed_32_eq(&actual, &expected)
}

fn fixed_32_eq(left: &[u8; 32], right: &[u8; 32]) -> bool {
    left.iter()
        .zip(right.iter())
        .fold(0u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}

#[cfg(windows)]
#[inline(never)]
fn verify_startup_activation() -> Result<[u8; 32], ()> {
    let local_app_data = std::env::var_os("LOCALAPPDATA").ok_or(())?;
    let license_dir = PathBuf::from(local_app_data).join(WINDOWS_DEVICE_ACTIVATION_DIR_NAME);
    verify_activation_dir_startup(&license_dir)
}

#[cfg(not(windows))]
#[inline(never)]
fn verify_startup_activation() -> Result<[u8; 32], ()> {
    Err(())
}

#[cfg(windows)]
#[inline(never)]
fn verify_stage_activation() -> Result<[u8; 32], ()> {
    let local_app_data = std::env::var_os("LOCALAPPDATA").ok_or(())?;
    let license_dir = PathBuf::from(local_app_data).join(WINDOWS_DEVICE_ACTIVATION_DIR_NAME);
    verify_activation_dir_stage(&license_dir)
}

#[cfg(not(windows))]
#[inline(never)]
fn verify_stage_activation() -> Result<[u8; 32], ()> {
    Err(())
}

#[inline(never)]
fn verify_activation_dir_startup(license_dir: &Path) -> Result<[u8; 32], ()> {
    let metadata = fs::symlink_metadata(license_dir).map_err(|_| ())?;
    if !metadata.is_dir() {
        return Err(());
    }
    let identity_blob = read_bounded_regular_file(
        &license_dir.join(DEVICE_IDENTITY_FILE_NAME),
        DEVICE_IDENTITY_MAX_BYTES,
    )?;
    let license_bytes =
        read_bounded_regular_file(&license_dir.join(LICENSE_FILE_NAME), LICENSE_FILE_MAX_BYTES)?;
    let verifying_key = production_verifying_key()?;
    verify_activation_material_startup(&identity_blob, &license_bytes, &verifying_key)
}

#[inline(never)]
fn verify_activation_material_startup(
    identity_blob: &[u8],
    license_bytes: &[u8],
    verifying_key: &[u8; 32],
) -> Result<[u8; 32], ()> {
    let mut secret = device_secret_from_identity_blob(identity_blob)?;
    let result = (|| {
        let expected_device_id = derive_device_id(&secret);
        verify_signed_license_bytes(license_bytes, &expected_device_id, verifying_key)?;
        Ok(derive_activation_fingerprint(&secret, license_bytes))
    })();
    secret.fill(0);
    result
}

#[inline(never)]
fn verify_activation_dir_stage(license_dir: &Path) -> Result<[u8; 32], ()> {
    let metadata = fs::symlink_metadata(license_dir).map_err(|_| ())?;
    if !metadata.is_dir() {
        return Err(());
    }
    let license_bytes =
        read_bounded_regular_file(&license_dir.join(LICENSE_FILE_NAME), LICENSE_FILE_MAX_BYTES)?;
    let identity_blob = read_bounded_regular_file(
        &license_dir.join(DEVICE_IDENTITY_FILE_NAME),
        DEVICE_IDENTITY_MAX_BYTES,
    )?;
    let verifying_key = production_verifying_key()?;
    verify_activation_material_stage(&license_bytes, &identity_blob, &verifying_key)
}

#[inline(never)]
fn verify_activation_material_stage(
    license_bytes: &[u8],
    identity_blob: &[u8],
    verifying_key: &[u8; 32],
) -> Result<[u8; 32], ()> {
    let license_digest: [u8; 32] = Sha256::digest(license_bytes).into();
    let signed = serde_json::from_slice::<SignedLicense>(license_bytes).map_err(|_| ())?;
    let mut secret = device_secret_from_identity_blob(identity_blob)?;
    let result = (|| {
        let expected_device_id = derive_device_id(&secret);
        verify_signed_license_stage(&signed, &expected_device_id, verifying_key)?;
        Ok(derive_activation_fingerprint_from_digest(
            &secret,
            &license_digest,
        ))
    })();
    secret.fill(0);
    result
}

fn read_bounded_regular_file(path: &Path, limit: u64) -> Result<Vec<u8>, ()> {
    let metadata = fs::symlink_metadata(path).map_err(|_| ())?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(());
    }
    let mut file = fs::File::open(path)
        .map_err(|_| ())?
        .take(limit.saturating_add(1));
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|_| ())?;
    if bytes.len() as u64 > limit {
        return Err(());
    }
    Ok(bytes)
}

fn device_secret_from_identity_blob(bytes: &[u8]) -> Result<[u8; DEVICE_SECRET_BYTES], ()> {
    if bytes.len() <= DEVICE_IDENTITY_MAGIC.len()
        || &bytes[..DEVICE_IDENTITY_MAGIC.len()] != DEVICE_IDENTITY_MAGIC
    {
        return Err(());
    }
    let secret = unprotect_current_user(&bytes[DEVICE_IDENTITY_MAGIC.len()..])?;
    secret.try_into().map_err(|_| ())
}

#[cfg(test)]
fn device_id_from_identity_blob(bytes: &[u8]) -> Result<String, ()> {
    let mut secret = device_secret_from_identity_blob(bytes)?;
    let device_id = derive_device_id(&secret);
    secret.fill(0);
    Ok(device_id)
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

fn derive_activation_fingerprint(
    secret: &[u8; DEVICE_SECRET_BYTES],
    license_bytes: &[u8],
) -> [u8; 32] {
    let license_digest: [u8; 32] = Sha256::digest(license_bytes).into();
    derive_activation_fingerprint_from_digest(secret, &license_digest)
}

fn derive_activation_fingerprint_from_digest(
    secret: &[u8; DEVICE_SECRET_BYTES],
    license_digest: &[u8; 32],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(ACTIVATION_FINGERPRINT_DOMAIN);
    hasher.update(secret);
    hasher.update(license_digest);
    hasher.finalize().into()
}

fn runtime_seal(fingerprint: &[u8; 32]) -> Result<[u8; 32], ()> {
    let current_exe = std::env::current_exe().map_err(|_| ())?;
    let current_exe = fs::canonicalize(&current_exe).unwrap_or(current_exe);
    Ok(derive_runtime_seal_for(
        fingerprint,
        std::process::id(),
        current_exe.to_string_lossy().as_bytes(),
    ))
}

fn derive_runtime_seal_for(fingerprint: &[u8; 32], pid: u32, executable_path: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(RUNTIME_SEAL_DOMAIN);
    hasher.update(fingerprint);
    hasher.update(pid.to_le_bytes());
    hasher.update(executable_path);
    hasher.finalize().into()
}

fn derive_stage_proof(runtime_seal: &[u8; 32], stage_domain: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(stage_domain);
    hasher.update(runtime_seal);
    hasher.finalize().into()
}

fn verify_signed_license_bytes(
    bytes: &[u8],
    expected_device_id: &str,
    verifying_key: &[u8; 32],
) -> Result<(), ()> {
    let signed = serde_json::from_slice::<SignedLicense>(bytes).map_err(|_| ())?;
    verify_signed_license(&signed, expected_device_id, verifying_key)
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

#[inline(never)]
fn verify_signed_license_stage(
    signed: &SignedLicense,
    expected_device_id: &str,
    verifying_key: &[u8; 32],
) -> Result<(), ()> {
    if signed.product != LICENSE_PRODUCT || signed.license_version != LICENSE_VERSION {
        return Err(());
    }
    if signed.device_id != expected_device_id {
        return Err(());
    }
    if !bounded_identifier(&signed.activation_code_id, 128)
        || !bounded_identifier(&signed.license_id, 128)
    {
        return Err(());
    }
    if signed.issued_at.is_empty() || signed.issued_at.len() > 128 || !signed.issued_at.is_ascii() {
        return Err(());
    }
    let message = serde_json::to_vec(&LicenseSigningPayload {
        activation_code_id: &signed.activation_code_id,
        device_id: &signed.device_id,
        issued_at: &signed.issued_at,
        license_id: &signed.license_id,
        license_version: signed.license_version,
        product: &signed.product,
    })
    .map_err(|_| ())?;
    let signature = URL_SAFE_NO_PAD
        .decode(signed.signature.as_bytes())
        .map_err(|_| ())?;
    UnparsedPublicKey::new(&ED25519, verifying_key)
        .verify(&message, &signature)
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

fn production_verifying_key() -> Result<[u8; 32], ()> {
    URL_SAFE_NO_PAD
        .decode(LICENSE_PUBLIC_KEY_B64URL.as_bytes())
        .map_err(|_| ())?
        .try_into()
        .map_err(|_| ())
}

#[cfg(windows)]
fn unprotect_current_user(protected: &[u8]) -> Result<Vec<u8>, ()> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    if protected.is_empty() || protected.len() > u32::MAX as usize {
        return Err(());
    }
    let input = CRYPT_INTEGER_BLOB {
        cbData: protected.len() as u32,
        pbData: protected.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 || output.pbData.is_null() {
        return Err(());
    }
    let plain =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(output.pbData as *mut _);
    }
    Ok(plain)
}

#[cfg(not(windows))]
fn unprotect_current_user(_protected: &[u8]) -> Result<Vec<u8>, ()> {
    Err(())
}

fn is_exact_argument(args: &[OsString], expected: &str) -> bool {
    args.len() == 1 && args[0] == OsStr::new(expected)
}

#[cfg(windows)]
fn current_process_has_product_parent() -> bool {
    let Some(parent_pid) = windows_parent_process_id(std::process::id()) else {
        return false;
    };
    let Some(parent_exe) = windows_process_executable(parent_pid) else {
        return false;
    };
    let Ok(current_exe) = std::env::current_exe() else {
        return false;
    };
    product_parent_matches(&current_exe, &parent_exe)
}

#[cfg(not(windows))]
fn current_process_has_product_parent() -> bool {
    false
}

#[cfg(windows)]
fn windows_parent_process_id(pid: u32) -> Option<u32> {
    use std::mem::{size_of, zeroed};
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    // SAFETY: the ToolHelp snapshot handle is checked against INVALID_HANDLE_VALUE,
    // PROCESSENTRY32W is initialized with its documented size, and the handle is
    // closed on every path after successful creation.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return None;
    }
    let mut entry: PROCESSENTRY32W = unsafe { zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    let mut found = None;
    let mut ok = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while ok {
        if entry.th32ProcessID == pid {
            found = (entry.th32ParentProcessID > 0).then_some(entry.th32ParentProcessID);
            break;
        }
        ok = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    unsafe { CloseHandle(snapshot) };
    found
}

#[cfg(windows)]
fn windows_process_executable(pid: u32) -> Option<std::path::PathBuf> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    // SAFETY: OpenProcess returns either a valid handle or NULL. The UTF-16
    // buffer and in/out length remain valid for QueryFullProcessImageNameW.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let mut buffer = [0u16; 32768];
    let mut size = buffer.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size) };
    unsafe { CloseHandle(handle) };
    if ok == 0 {
        return None;
    }
    Some(std::path::PathBuf::from(String::from_utf16_lossy(
        &buffer[..size as usize],
    )))
}

#[cfg(windows)]
fn product_parent_matches(current_exe: &std::path::Path, parent_exe: &std::path::Path) -> bool {
    let Some(core_dir) = current_exe.parent() else {
        return false;
    };
    let Some(install_dir) = core_dir.parent() else {
        return false;
    };
    if !os_name_eq(core_dir.file_name(), "codexlink-core") {
        return false;
    }
    let current_name = current_exe.file_name();
    if ![
        "codexlink-core.exe",
        "codexlink-core-server.exe",
        "codexlink-core-runner.exe",
    ]
    .iter()
    .any(|name| os_name_eq(current_name, name))
    {
        return false;
    }

    if os_name_eq(parent_exe.file_name(), "CodexLink.exe")
        && path_eq(parent_exe.parent(), Some(install_dir))
    {
        return true;
    }

    [
        "codexlink-core.exe",
        "codexlink-core-server.exe",
        "codexlink-core-runner.exe",
    ]
    .iter()
    .any(|name| os_name_eq(parent_exe.file_name(), name))
        && path_eq(parent_exe.parent(), Some(core_dir))
}

#[cfg(windows)]
fn os_name_eq(actual: Option<&OsStr>, expected: &str) -> bool {
    actual.is_some_and(|actual| actual.to_string_lossy().eq_ignore_ascii_case(expected))
}

#[cfg(windows)]
fn path_eq(left: Option<&std::path::Path>, right: Option<&std::path::Path>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => {
            normalized_windows_path(left) == normalized_windows_path(right)
        }
        _ => false,
    }
}

#[cfg(windows)]
fn normalized_windows_path(path: &std::path::Path) -> String {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut value = canonical.to_string_lossy().replace('/', "\\");
    if let Some(stripped) = value.strip_prefix("\\\\?\\") {
        value = stripped.to_string();
    }
    value.trim_end_matches('\\').to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decide(
        product_mode: bool,
        args: &[&str],
        activation_ok: bool,
        parent_ok: bool,
    ) -> Result<Admission, String> {
        let args = args.iter().map(OsString::from).collect::<Vec<_>>();
        if is_exact_argument(&args, "--version") || is_exact_argument(&args, "-V") {
            return Ok(Admission::Diagnostic);
        }
        if !product_mode {
            return Ok(Admission::Operational);
        }
        if activation_ok && parent_ok {
            Ok(Admission::Operational)
        } else {
            Err(PRODUCT_MODE_DENIED.to_string())
        }
    }

    #[test]
    fn generic_engine_mode_preserves_standalone_behavior() {
        assert_eq!(
            decide(false, &["server", "run"], false, false),
            Ok(Admission::Operational)
        );
    }

    #[test]
    fn bundled_mode_keeps_exact_version_diagnostic_only() {
        assert_eq!(
            decide(true, &["--version"], false, false),
            Ok(Admission::Diagnostic)
        );
        assert_eq!(
            decide(true, &["--version", "extra"], false, false),
            Err(PRODUCT_MODE_DENIED.to_string())
        );
    }

    #[test]
    fn bundled_mode_requires_activation_and_product_parent_for_operational_entrypoints() {
        assert_eq!(
            decide(true, &["server", "run"], true, true),
            Ok(Admission::Operational)
        );
        assert_eq!(
            decide(true, &["server", "run"], false, true),
            Err(PRODUCT_MODE_DENIED.to_string())
        );
        assert_eq!(
            decide(true, &["server", "run"], true, false),
            Err(PRODUCT_MODE_DENIED.to_string())
        );
    }

    #[test]
    fn runtime_seal_binds_activation_process_and_binary() {
        let fingerprint = [7u8; 32];
        let baseline =
            derive_runtime_seal_for(&fingerprint, 42, b"C:/CodexLink/codexlink-core.exe");
        assert_ne!(
            baseline,
            derive_runtime_seal_for(&[8u8; 32], 42, b"C:/CodexLink/codexlink-core.exe")
        );
        assert_ne!(
            baseline,
            derive_runtime_seal_for(&fingerprint, 43, b"C:/CodexLink/codexlink-core.exe")
        );
        assert_ne!(
            baseline,
            derive_runtime_seal_for(&fingerprint, 42, b"C:/CodexLink/codexlink-core-runner.exe")
        );
    }

    #[test]
    fn operational_stage_requires_matching_startup_seal() {
        let fingerprint = [11u8; 32];
        let expected = derive_runtime_seal_for(&fingerprint, 99, b"runtime.exe");
        assert!(stage_seal_matches(
            Some(&expected),
            &expected,
            CLI_STAGE_DOMAIN
        ));
        assert!(!stage_seal_matches(None, &expected, CLI_STAGE_DOMAIN));
        let other = derive_runtime_seal_for(&[12u8; 32], 99, b"runtime.exe");
        assert!(!stage_seal_matches(
            Some(&other),
            &expected,
            CLI_STAGE_DOMAIN
        ));
    }

    #[cfg(windows)]
    #[test]
    fn startup_and_stage_activation_paths_agree_but_verify_independently() {
        use ring::rand::SystemRandom;
        use ring::signature::{Ed25519KeyPair, KeyPair};

        let rng = SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let secret = [23u8; DEVICE_SECRET_BYTES];
        let device_id = derive_device_id(&secret);
        let mut identity_blob = DEVICE_IDENTITY_MAGIC.to_vec();
        identity_blob.extend_from_slice(&protect_current_user_for_test(&secret));
        let mut signed = SignedLicense {
            license_version: LICENSE_VERSION,
            license_id: "license-independent-paths".to_string(),
            product: LICENSE_PRODUCT.to_string(),
            activation_code_id: "activation-independent-paths".to_string(),
            device_id,
            issued_at: "2026-09-22T00:00:00Z".to_string(),
            signature: String::new(),
        };
        let signature = key_pair.sign(&license_signing_message(&signed).unwrap());
        signed.signature = URL_SAFE_NO_PAD.encode(signature.as_ref());
        let license_bytes = serde_json::to_vec(&signed).unwrap();
        let public_key: [u8; 32] = key_pair.public_key().as_ref().try_into().unwrap();

        let startup =
            verify_activation_material_startup(&identity_blob, &license_bytes, &public_key)
                .unwrap();
        let stage =
            verify_activation_material_stage(&license_bytes, &identity_blob, &public_key).unwrap();
        assert_eq!(startup, stage);

        signed.license_id = "tampered-independent-paths".to_string();
        let tampered = serde_json::to_vec(&signed).unwrap();
        assert_eq!(
            verify_activation_material_startup(&identity_blob, &tampered, &public_key),
            Err(())
        );
        assert_eq!(
            verify_activation_material_stage(&tampered, &identity_blob, &public_key),
            Err(())
        );
    }

    #[test]
    fn signed_license_must_match_exact_device_and_signature() {
        use ring::rand::SystemRandom;
        use ring::signature::{Ed25519KeyPair, KeyPair};

        let rng = SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let device_id = derive_device_id(&[7u8; DEVICE_SECRET_BYTES]);
        let mut signed = SignedLicense {
            license_version: LICENSE_VERSION,
            license_id: "license-test-1".to_string(),
            product: LICENSE_PRODUCT.to_string(),
            activation_code_id: "activation-test-1".to_string(),
            device_id: device_id.clone(),
            issued_at: "2026-09-18T00:00:00Z".to_string(),
            signature: String::new(),
        };
        let signature = key_pair.sign(&license_signing_message(&signed).unwrap());
        signed.signature = URL_SAFE_NO_PAD.encode(signature.as_ref());
        let bytes = serde_json::to_vec(&signed).unwrap();
        let public_key: [u8; 32] = key_pair.public_key().as_ref().try_into().unwrap();

        assert_eq!(
            verify_signed_license_bytes(&bytes, &device_id, &public_key),
            Ok(())
        );
        assert_eq!(
            verify_signed_license_bytes(&bytes, "wcd-wrong-device", &public_key),
            Err(())
        );
        signed.product = "other-product".to_string();
        let tampered = serde_json::to_vec(&signed).unwrap();
        assert_eq!(
            verify_signed_license_bytes(&tampered, &device_id, &public_key),
            Err(())
        );
    }

    #[cfg(windows)]
    fn protect_current_user_for_test(plain: &[u8]) -> Vec<u8> {
        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::Security::Cryptography::{
            CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        };
        let input = CRYPT_INTEGER_BLOB {
            cbData: plain.len() as u32,
            pbData: plain.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        let ok = unsafe {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        assert_ne!(ok, 0);
        let protected =
            unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
        unsafe {
            LocalFree(output.pbData as *mut _);
        }
        protected
    }

    #[cfg(windows)]
    #[test]
    fn current_user_dpapi_identity_derives_expected_device_id() {
        let secret = [19u8; DEVICE_SECRET_BYTES];
        let mut blob = DEVICE_IDENTITY_MAGIC.to_vec();
        blob.extend_from_slice(&protect_current_user_for_test(&secret));
        assert_eq!(
            device_id_from_identity_blob(&blob),
            Ok(derive_device_id(&secret))
        );
    }

    #[cfg(windows)]
    #[test]
    fn installed_layout_accepts_desktop_or_core_parent_and_rejects_other_parent() {
        let root = std::path::Path::new(r"C:\Program Files\CodexLink");
        let current = root.join(r"codexlink-core\codexlink-core-runner.exe");
        assert!(product_parent_matches(
            &current,
            &root.join("CodexLink.exe")
        ));
        assert!(product_parent_matches(
            &current,
            &root.join(r"codexlink-core\codexlink-core.exe")
        ));
        assert!(!product_parent_matches(
            &current,
            &std::path::Path::new(r"C:\Windows\System32\cmd.exe")
        ));
        assert!(!product_parent_matches(
            &root.join(r"other\codexlink-core-runner.exe"),
            &root.join("CodexLink.exe")
        ));
    }
}
