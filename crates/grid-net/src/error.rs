use thiserror::Error;

/// Errors produced by the networking layer.
#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("transport error: {0}")]
    Transport(String),

    #[error("dial error: {0}")]
    Dial(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("topic subscription error: {0}")]
    Subscription(String),

    #[error("publish error: {0}")]
    Publish(String),

    #[error("failed to send response on channel")]
    ResponseFailed,
}
