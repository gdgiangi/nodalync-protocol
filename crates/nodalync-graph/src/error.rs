use thiserror::Error;

#[derive(Error, Debug)]
pub enum GraphError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Invalid entity ID format: {0}")]
    InvalidEntityId(String),

    #[error("Entity not found: {0}")]
    EntityNotFound(String),

    #[error("Content not found: {0}")]
    ContentNotFound(String),

    #[error("Invalid relationship predicate: {0}")]
    InvalidPredicate(String),

    #[error("Extraction failed: {0}")]
    ExtractionFailed(String),

    #[error("Schema migration error: {0}")]
    SchemaMigration(String),

    #[cfg(feature = "ai-extraction")]
    #[error("AI API error: {0}")]
    AiApi(#[from] reqwest::Error),
}

pub type Result<T> = std::result::Result<T, GraphError>;
