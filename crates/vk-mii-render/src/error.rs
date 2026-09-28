#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("invalid rendering resource: {0}")]
    InvalidResource(String),

    #[error("invalid studio data: {0}")]
    InvalidStudioData(String),

    #[error("the Mii produced nothing to draw: {0}")]
    NothingToDraw(String),

    #[error("PNG encoding failed: {0}")]
    Encoding(String),
}

pub type RenderResult<T> = Result<T, RenderError>;
