pub type AssemblywrightResult<T> = Result<T, AssemblywrightError>;

#[derive(Debug, thiserror::Error)]
pub enum AssemblywrightError {
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
