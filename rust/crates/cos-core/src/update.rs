use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::fmt;
use std::path::{Path, PathBuf};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CosUpdateManifest {
    pub version: String,
    pub build: i64,
    #[serde(rename = "downloadURL")]
    pub download_url: Url,
    pub sha256: String,
    pub minimum_system_version: String,
    pub release_notes: String,
}

#[derive(Debug, Clone)]
pub struct PreparedCosUpdate {
    pub app_url: PathBuf,
    pub working_directory: PathBuf,
    pub manifest: CosUpdateManifest,
}

#[derive(Debug)]
pub enum CosUpdateError {
    InvalidResponse,
    Http(i64),
    InvalidManifest,
    UntrustedDownloadHost,
    UnsupportedSystem(String),
    ArchiveTooLarge,
    HashMismatch,
    MissingApp,
    InvalidBundle,
    ValidationFailed(String),
    NotRunningFromApp,
    RunningFromDiskImage,
    InstallLocationNotWritable(String),
    CouldNotStartInstaller(String),
}

impl fmt::Display for CosUpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidResponse => write!(f, "The update server returned an invalid response."),
            Self::Http(code) => write!(f, "The update server returned HTTP {code}."),
            Self::InvalidManifest => write!(f, "The update manifest is malformed."),
            Self::UntrustedDownloadHost => write!(f, "Cos refused an update from an untrusted download host."),
            Self::UnsupportedSystem(version) => write!(f, "This update requires macOS {version} or later."),
            Self::ArchiveTooLarge => write!(f, "The update archive is empty or exceeds 250 MB."),
            Self::HashMismatch => write!(f, "The update failed its SHA-256 integrity check."),
            Self::MissingApp => write!(f, "The update archive does not contain Cos.app."),
            Self::InvalidBundle => write!(f, "The update is not a valid Cos application bundle."),
            Self::ValidationFailed(detail) => write!(f, "The update’s code signature is invalid. {detail}"),
            Self::NotRunningFromApp => write!(f, "Cos must be launched from its application bundle to update itself."),
            Self::RunningFromDiskImage => {
                write!(f, "Move Cos to Applications, reopen it there, and try the update again.")
            }
            Self::InstallLocationNotWritable(path) => {
                write!(f, "Cos cannot update {path}. Move it to Applications or another writable folder.")
            }
            Self::CouldNotStartInstaller(detail) => write!(f, "Cos could not start the update installer. {detail}"),
        }
    }
}

impl std::error::Error for CosUpdateError {}

type UpdateResult<T> = Result<T, CosUpdateError>;

pub struct CosUpdateService {
    feed_url: Url,
    client: reqwest::Client,
}

impl CosUpdateService {
    pub const DEFAULT_FEED_URL: &'static str = "https://cos.ssh.codes/api/update";

    pub fn new(feed_url: Url) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { feed_url, client }
    }

    pub async fn check(
        &self,
        current_version: &str,
        current_build: i64,
    ) -> UpdateResult<Option<CosUpdateManifest>> {
        let response = self
            .client
            .get(self.feed_url.clone())
            .header("Accept", "application/json")
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(|_| CosUpdateError::InvalidResponse)?;
        if !response.status().is_success() {
            return Err(CosUpdateError::Http(response.status().as_u16() as i64));
        }
        let data = response.bytes().await.map_err(|_| CosUpdateError::InvalidResponse)?;
        if data.len() > 64_000 {
            return Err(CosUpdateError::InvalidManifest);
        }
        let manifest: CosUpdateManifest =
            serde_json::from_slice(&data).map_err(|_| CosUpdateError::InvalidManifest)?;
        validate_manifest(&manifest)?;
        let version_is_newer = is_newer(&manifest.version, current_version);
        let version_matches = !version_is_newer && !is_newer(current_version, &manifest.version);
        if !(version_is_newer || (version_matches && manifest.build > current_build)) {
            return Ok(None);
        }
        if !is_system_version_supported(&manifest.minimum_system_version) {
            return Err(CosUpdateError::UnsupportedSystem(manifest.minimum_system_version.clone()));
        }
        Ok(Some(manifest))
    }

    pub async fn download_and_verify(&self, manifest: &CosUpdateManifest) -> UpdateResult<PreparedCosUpdate> {
        validate_manifest(manifest)?;
        let response = self
            .client
            .get(manifest.download_url.clone())
            .header("Accept", "application/zip")
            .send()
            .await
            .map_err(|_| CosUpdateError::InvalidResponse)?;
        if !response.status().is_success() {
            return Err(CosUpdateError::Http(response.status().as_u16() as i64));
        }

        let working_directory = std::env::temp_dir().join(format!("CosUpdate-{}", uuid::Uuid::new_v4()));
        let archive_url = working_directory.join("Cos.zip");
        let unpacked_url = working_directory.join("Unpacked");
        std::fs::create_dir_all(&unpacked_url).map_err(|_| CosUpdateError::InvalidResponse)?;

        let result = async {
            let data = response.bytes().await.map_err(|_| CosUpdateError::InvalidResponse)?;
            if data.is_empty() || data.len() > 250_000_000 {
                return Err(CosUpdateError::ArchiveTooLarge);
            }
            std::fs::write(&archive_url, &data).map_err(|_| CosUpdateError::InvalidResponse)?;
            let actual_hash = sha256_file(&archive_url)?;
            if !actual_hash.eq_ignore_ascii_case(&manifest.sha256) {
                return Err(CosUpdateError::HashMismatch);
            }
            run_checked(
                "/usr/bin/ditto",
                &["-x", "-k", &archive_url.to_string_lossy(), &unpacked_url.to_string_lossy()],
            )?;
            let app_url = find_cos_app(&unpacked_url)?;
            validate_bundle(&app_url, manifest)?;
            Ok(PreparedCosUpdate {
                app_url,
                working_directory: working_directory.clone(),
                manifest: manifest.clone(),
            })
        }
        .await;
        if result.is_err() {
            let _ = std::fs::remove_dir_all(&working_directory);
        }
        result
    }

    pub fn validate_install_location(current_app_url: &Path) -> UpdateResult<()> {
        let current = crate::canonical_path(current_app_url);
        if current.extension().and_then(|value| value.to_str()) != Some("app") || !current.exists() {
            return Err(CosUpdateError::NotRunningFromApp);
        }
        if current.to_string_lossy().starts_with("/Volumes/") {
            return Err(CosUpdateError::RunningFromDiskImage);
        }
        let Some(parent) = current.parent() else {
            return Err(CosUpdateError::NotRunningFromApp);
        };
        let probe = parent.join(format!(".cos-write-probe-{}", uuid::Uuid::new_v4()));
        match std::fs::write(&probe, b"") {
            Ok(()) => {
                let _ = std::fs::remove_file(&probe);
            }
            Err(_) => return Err(CosUpdateError::InstallLocationNotWritable(parent.to_string_lossy().into_owned())),
        }
        Ok(())
    }

    pub fn schedule_replacement(
        prepared: &PreparedCosUpdate,
        current_app_url: &Path,
        process_id: i32,
    ) -> UpdateResult<()> {
        let current = crate::canonical_path(current_app_url);
        Self::validate_install_location(&current)?;
        let parent = current.parent().ok_or(CosUpdateError::NotRunningFromApp)?;

        let identifier = uuid::Uuid::new_v4().to_string().to_uppercase();
        let staged = parent.join(format!(".Cos-update-{identifier}.app"));
        let backup = parent.join(format!(".Cos-previous-{identifier}.app"));
        copy_directory(&prepared.app_url, &staged).map_err(|error| {
            CosUpdateError::CouldNotStartInstaller(error.to_string())
        })?;
        if let Err(error) = validate_bundle(&staged, &prepared.manifest) {
            let _ = std::fs::remove_dir_all(&staged);
            return Err(error);
        }

        let script = r#"set -u
current="$1"
staged="$2"
backup="$3"
old_pid="$4"
cleanup="$5"

while /bin/kill -0 "$old_pid" 2>/dev/null; do /bin/sleep 0.15; done
if ! /bin/mv "$current" "$backup"; then exit 20; fi
if /bin/mv "$staged" "$current"; then
  /usr/bin/nohup "$current/Contents/MacOS/Cos" >/dev/null 2>&1 &
  new_pid=$!
  /bin/sleep 3
  if /bin/kill -0 "$new_pid" 2>/dev/null; then
    /bin/rm -rf "$backup" "$cleanup"
    exit 0
  fi
fi
/bin/rm -rf "$current"
/bin/mv "$backup" "$current"
/usr/bin/open -n "$current"
/bin/rm -rf "$staged" "$cleanup"
exit 21
"#;
        let spawn = std::process::Command::new("/bin/zsh")
            .arg("-c")
            .arg(script)
            .arg("cos-updater")
            .arg(current.to_string_lossy().into_owned())
            .arg(staged.to_string_lossy().into_owned())
            .arg(backup.to_string_lossy().into_owned())
            .arg(process_id.to_string())
            .arg(prepared.working_directory.to_string_lossy().into_owned())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        match spawn {
            Ok(_) => Ok(()),
            Err(error) => {
                let _ = std::fs::remove_dir_all(&staged);
                Err(CosUpdateError::CouldNotStartInstaller(error.to_string()))
            }
        }
    }
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    let candidate_parts = numeric_version(candidate);
    let current_parts = numeric_version(current);
    for index in 0..candidate_parts.len().max(current_parts.len()) {
        let lhs = candidate_parts.get(index).copied().unwrap_or(0);
        let rhs = current_parts.get(index).copied().unwrap_or(0);
        if lhs != rhs {
            return lhs > rhs;
        }
    }
    false
}

fn numeric_version(value: &str) -> Vec<i64> {
    value
        .split('.')
        .map(|component| {
            let digits: String = component.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse().unwrap_or(0)
        })
        .collect()
}

fn validate_manifest(manifest: &CosUpdateManifest) -> UpdateResult<()> {
    static VERSION: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"^[0-9]+(\.[0-9]+){1,3}$").unwrap());
    static HASH: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"^[a-fA-F0-9]{64}$").unwrap());
    if manifest.download_url.scheme() != "https" || manifest.download_url.host_str() != Some("cos.ssh.codes") {
        return Err(CosUpdateError::UntrustedDownloadHost);
    }
    if !VERSION.is_match(&manifest.version) || manifest.build <= 0 || !HASH.is_match(&manifest.sha256) {
        return Err(CosUpdateError::InvalidManifest);
    }
    Ok(())
}

fn is_system_version_supported(minimum: &str) -> bool {
    let current = system_version_string();
    !is_newer(minimum, &current)
}

fn system_version_string() -> String {
    use objc2_foundation::NSProcessInfo;
    let info = NSProcessInfo::processInfo();
    let version = info.operatingSystemVersion();
    format!(
        "{}.{}.{}",
        version.majorVersion, version.minorVersion, version.patchVersion
    )
}

fn sha256_file(path: &Path) -> UpdateResult<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|_| CosUpdateError::InvalidResponse)?;
    let mut hasher = sha2::Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|_| CosUpdateError::InvalidResponse)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect())
}

fn find_cos_app(root: &Path) -> UpdateResult<PathBuf> {
    let direct = root.join("Cos.app");
    if direct.exists() {
        return Ok(direct);
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            let path = entry.path();
            if name == "Cos.app" {
                return Ok(path);
            }
            // Skip package descendants like the Swift enumerator does.
            if path.is_dir() && path.extension().and_then(|value| value.to_str()) != Some("app") {
                stack.push(path);
            }
        }
    }
    Err(CosUpdateError::MissingApp)
}

fn validate_bundle(app_url: &Path, manifest: &CosUpdateManifest) -> UpdateResult<()> {
    let info = app_url.join("Contents/Info.plist");
    let value: plist::Value = plist::from_file(&info).map_err(|_| CosUpdateError::InvalidBundle)?;
    let dictionary = value.as_dictionary().ok_or(CosUpdateError::InvalidBundle)?;
    let identifier = dictionary
        .get("CFBundleIdentifier")
        .and_then(plist::Value::as_string)
        .unwrap_or_default();
    let version = dictionary
        .get("CFBundleShortVersionString")
        .and_then(plist::Value::as_string)
        .unwrap_or_default();
    let build = dictionary
        .get("CFBundleVersion")
        .and_then(plist::Value::as_string)
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or_default();
    let executable_name = dictionary
        .get("CFBundleExecutable")
        .and_then(plist::Value::as_string)
        .unwrap_or("Cos");
    let executable = app_url.join("Contents/MacOS").join(executable_name);
    if identifier != "codes.ssh.cos"
        || version != manifest.version
        || build != manifest.build
        || !is_executable(&executable)
    {
        return Err(CosUpdateError::InvalidBundle);
    }
    run_checked("/usr/bin/codesign", &["--verify", "--deep", "--strict", &app_url.to_string_lossy()])
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn run_checked(executable: &str, arguments: &[&str]) -> UpdateResult<()> {
    let output = std::process::Command::new(executable)
        .args(arguments)
        .output()
        .map_err(|error| CosUpdateError::ValidationFailed(error.to_string()))?;
    if !output.status.success() {
        let mut combined = output.stdout;
        combined.extend_from_slice(&output.stderr);
        let clipped = &combined[..combined.len().min(4_000)];
        let detail = String::from_utf8_lossy(clipped).trim().to_string();
        return Err(CosUpdateError::ValidationFailed(detail));
    }
    Ok(())
}

fn copy_directory(source: &Path, target: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let destination = target.join(entry.file_name());
        if path.is_dir() {
            copy_directory(&path, &destination)?;
        } else {
            std::fs::copy(&path, &destination)?;
        }
    }
    Ok(())
}
