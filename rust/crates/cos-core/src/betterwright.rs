use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum BetterWrightRuntimeError {
    Unavailable,
    Failed(String),
}

impl fmt::Display for BetterWrightRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => write!(
                f,
                "The bundled BetterWright runtime is unavailable. Reinstall Cos or install BetterWright 1.6.3."
            ),
            Self::Failed(detail) => write!(f, "{detail}"),
        }
    }
}

impl std::error::Error for BetterWrightRuntimeError {}

#[derive(Debug, Clone)]
pub struct BetterWrightInvocation {
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub environment: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct BetterWrightCommandResult {
    pub status: i32,
    pub output: String,
    pub error_output: String,
}

pub struct CosBetterWrightRuntime;

impl CosBetterWrightRuntime {
    pub const PACKAGE_VERSION: &'static str = "1.6.3";
    pub const PROFILE: &'static str = "cos";
    pub const VIEWER_VIEWPORT_WIDTH: i64 = 900;
    pub const VIEWER_VIEWPORT_HEIGHT: i64 = 900;

    /// Locate the BetterWright CLI: bundled runtime first, then the
    /// COS_BETTERWRIGHT_EXECUTABLE override, then a global installation.
    pub fn invocation(arguments: &[String]) -> Result<BetterWrightInvocation, BetterWrightRuntimeError> {
        let mut environment: Vec<(String, String)> = std::env::vars().collect();
        let common_path = ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin"].join(":");
        let current_path = std::env::var("PATH").unwrap_or_default();
        set_env(&mut environment, "PATH", &format!("{common_path}:{current_path}"));
        set_env(&mut environment, "NODE_NO_WARNINGS", "1");

        if let Some(resources) = crate::bundle_resources_dir() {
            let root = resources.join("BetterWright");
            let node = root.join("runtime/node");
            let cli = root.join("package/node_modules/betterwright/dist/bin/betterwright.js");
            if is_executable(&node) && cli.exists() {
                let mut args = vec![cli.to_string_lossy().into_owned()];
                args.extend(arguments.iter().cloned());
                return Ok(BetterWrightInvocation { executable: node, arguments: args, environment });
            }
        }

        if let Ok(override_path) = std::env::var("COS_BETTERWRIGHT_EXECUTABLE") {
            if !override_path.is_empty() && is_executable(std::path::Path::new(&override_path)) {
                return Ok(BetterWrightInvocation {
                    executable: PathBuf::from(override_path),
                    arguments: arguments.to_vec(),
                    environment,
                });
            }
        }
        for path in ["/opt/homebrew/bin/betterwright", "/usr/local/bin/betterwright"] {
            let candidate = PathBuf::from(path);
            if is_executable(&candidate) {
                return Ok(BetterWrightInvocation { executable: candidate, arguments: arguments.to_vec(), environment });
            }
        }
        Err(BetterWrightRuntimeError::Unavailable)
    }

    pub fn doctor_blocking() -> Result<BetterWrightCommandResult, BetterWrightRuntimeError> {
        run(&["doctor".to_string(), "--json".to_string()], 2_000_000)
    }

    pub fn is_ready_blocking() -> bool {
        let Ok(result) = Self::doctor_blocking() else { return false };
        if result.status != 0 {
            return false;
        }
        let Ok(object) = serde_json::from_str::<serde_json::Value>(&result.output) else {
            return false;
        };
        object.get("ready").and_then(serde_json::Value::as_bool) == Some(true)
    }

    pub async fn is_ready() -> bool {
        tokio::task::spawn_blocking(Self::is_ready_blocking).await.unwrap_or(false)
    }

    pub async fn doctor() -> Result<BetterWrightCommandResult, BetterWrightRuntimeError> {
        tokio::task::spawn_blocking(Self::doctor_blocking)
            .await
            .map_err(|error| BetterWrightRuntimeError::Failed(error.to_string()))?
    }

    pub async fn setup() -> Result<BetterWrightCommandResult, BetterWrightRuntimeError> {
        tokio::task::spawn_blocking(|| run(&["setup".to_string()], 4_000_000))
            .await
            .map_err(|error| BetterWrightRuntimeError::Failed(error.to_string()))?
    }

    pub fn run_browser_blocking(code: &str, session: &str) -> Result<String, BetterWrightRuntimeError> {
        if code.len() > 64_000 {
            return Err(BetterWrightRuntimeError::Failed(
                "Browser code exceeded Cos’s 64 KB limit.".into(),
            ));
        }
        let prepared = format!(
            "await page.setViewportSize({{ width: {}, height: {} }});\n{code}",
            Self::VIEWER_VIEWPORT_WIDTH,
            Self::VIEWER_VIEWPORT_HEIGHT
        );
        let result = run(
            &[
                "run".to_string(),
                "-c".to_string(),
                prepared,
                "--session".to_string(),
                Self::sanitized_session(session),
                "--profile".to_string(),
                Self::PROFILE.to_string(),
            ],
            2_000_000,
        )?;
        let output = result.output.trim().to_string();
        if result.status != 0 {
            let detail = result.error_output.trim().to_string();
            return Err(BetterWrightRuntimeError::Failed(if detail.is_empty() {
                "BetterWright browser action failed.".into()
            } else {
                detail
            }));
        }
        Ok(output)
    }

    pub async fn run_browser(code: String, session: String) -> Result<String, BetterWrightRuntimeError> {
        tokio::task::spawn_blocking(move || Self::run_browser_blocking(&code, &session))
            .await
            .map_err(|error| BetterWrightRuntimeError::Failed(error.to_string()))?
    }

    pub async fn prepare_for_viewing(session: String) -> Result<(), BetterWrightRuntimeError> {
        Self::run_browser(
            "return { viewport: page.viewportSize(), url: page.url() }".to_string(),
            session,
        )
        .await?;
        Ok(())
    }

    pub fn sanitized_session(value: &str) -> String {
        let normalized: String = value
            .to_lowercase()
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || character == '-' {
                    character
                } else {
                    '-'
                }
            })
            .collect();
        static DASHES: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| regex::Regex::new("-+").unwrap());
        let compact = DASHES.replace_all(&normalized, "-");
        let trimmed = compact.trim_matches('-');
        let base = if trimmed.is_empty() { "default" } else { trimmed };
        base.chars().take(80).collect()
    }
}

fn set_env(environment: &mut Vec<(String, String)>, key: &str, value: &str) {
    environment.retain(|(existing, _)| existing != key);
    environment.push((key.to_string(), value.to_string()));
}

fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn run(arguments: &[String], maximum_bytes: usize) -> Result<BetterWrightCommandResult, BetterWrightRuntimeError> {
    let invocation = CosBetterWrightRuntime::invocation(arguments)?;
    let mut command = std::process::Command::new(&invocation.executable);
    command
        .args(&invocation.arguments)
        .env_clear()
        .envs(invocation.environment)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|error| BetterWrightRuntimeError::Failed(error.to_string()))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_thread = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(mut pipe) = stdout {
            use std::io::Read;
            let _ = pipe.read_to_end(&mut buffer);
        }
        buffer
    });
    let stderr_thread = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(mut pipe) = stderr {
            use std::io::Read;
            let _ = pipe.read_to_end(&mut buffer);
        }
        buffer
    });
    let status = child
        .wait()
        .map_err(|error| BetterWrightRuntimeError::Failed(error.to_string()))?;
    let output = stdout_thread.join().unwrap_or_default();
    let error_output = stderr_thread.join().unwrap_or_default();
    Ok(BetterWrightCommandResult {
        status: status.code().unwrap_or(-1),
        output: String::from_utf8_lossy(&output[..output.len().min(maximum_bytes)]).into_owned(),
        error_output: String::from_utf8_lossy(&error_output[..error_output.len().min(maximum_bytes)]).into_owned(),
    })
}
