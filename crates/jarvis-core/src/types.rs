pub type JarvisResult<T> = Result<T, JarvisError>;

#[derive(Debug, thiserror::Error)]
pub enum JarvisError {
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
