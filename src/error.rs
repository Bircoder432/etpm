use thiserror::Error;

#[derive(Error, Debug)]
pub enum TpmError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Package '{0}' version '{1}' not found")]
    PackageNotFound(String, String),
    #[error("Invalid version format: {0}")]
    InvalidVersion(#[from] semver::Error),
    #[error("Security violation: Path traversal detected in archive (Tar Slip)")]
    PathTraversal,
    #[error("Repository error: {0}")]
    Repository(String),
    #[error("RON parsing error: {0}")]
    RonParse(#[from] ron::error::SpannedError),
    #[error("Invalid URL format: {0}")]
    UrlParse(#[from] url::ParseError),
    #[error(
        "Signature verification failed: the package may be tampered with or signed by an untrusted key"
    )]
    InvalidSignature,
    #[error("File '{0}' not found in the package's addition directory")]
    AdditionFileNotFound(String),
    #[error("Invalid path requested: path traversal or absolute path detected")]
    InvalidAdditionPath,
}
