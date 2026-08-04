use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRuntimeError {
    MissingApiKey(String),
    UnsupportedProvider(String),
    LaunchFailed(String),
    RequestFailed(i64, String),
    DirectoryTrustRequired(String),
    InvalidProviderResponse(String),
}

impl fmt::Display for AgentRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingApiKey(provider) => {
                write!(f, "Sign in or add a key for {provider} in Settings → Providers.")
            }
            Self::UnsupportedProvider(provider) => {
                write!(f, "{provider} is not supported by the Cos harness.")
            }
            Self::LaunchFailed(detail) => {
                write!(f, "The Cos harness could not run the task: {detail}")
            }
            Self::RequestFailed(code, detail) => {
                write!(f, "The model request failed (HTTP {code}): {detail}")
            }
            Self::DirectoryTrustRequired(path) => {
                write!(f, "Cos needs permission to trust {path} before it can work there.")
            }
            Self::InvalidProviderResponse(detail) => {
                write!(f, "The provider returned an invalid response: {detail}")
            }
        }
    }
}

impl std::error::Error for AgentRuntimeError {}

pub type RuntimeResult<T> = Result<T, AgentRuntimeError>;
