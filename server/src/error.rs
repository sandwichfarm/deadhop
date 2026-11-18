//! Error type shared by every layer of the service.

/// Result alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Everything that can go wrong. Variants map one-to-one to stable wire codes.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid configuration: {0}")]
    Config(String),
    #[error("invalid parameters: {0}")]
    InvalidParams(String),
    #[error("proof of work rejected: {0}")]
    Pow(String),
    #[error("rate limit exceeded")]
    RateLimited,
    #[error("not authorized")]
    Unauthorized,
    #[error("invoice not found")]
    NotFound,
    #[error("service at capacity")]
    Capacity,
    #[error("invoicing is stopped by the operator")]
    Paused,
    #[error("chain backend: {0}")]
    Chain(String),
    #[error("storage: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("transport: {0}")]
    Transport(String),
}

impl From<contextvm_sdk::Error> for Error {
    fn from(e: contextvm_sdk::Error) -> Self {
        Error::Transport(e.to_string())
    }
}

impl Error {
    /// Stable machine-readable code sent to clients.
    pub fn code(&self) -> &'static str {
        match self {
            Error::InvalidParams(_) => "invalid_params",
            Error::Pow(_) => "pow_rejected",
            Error::RateLimited => "rate_limited",
            Error::Unauthorized => "unauthorized",
            Error::NotFound => "not_found",
            Error::Capacity => "capacity",
            Error::Paused => "paused",
            Error::Config(_) | Error::Chain(_) | Error::Db(_) | Error::Io(_) | Error::Transport(_) => "internal",
        }
    }

    /// Message that is safe to send to an untrusted caller. Internal failures
    /// are collapsed so that paths, SQL and backend addresses never leak.
    pub fn public_message(&self) -> String {
        match self.code() {
            "internal" => "internal error".to_string(),
            _ => self.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_errors_convert() {
        let e: Error = contextvm_sdk::Error::Transport("relay gone".into()).into();
        assert!(matches!(&e, Error::Transport(m) if m.contains("relay gone")));
        assert_eq!(e.public_message(), "internal error");
    }

    #[test]
    fn codes_and_public_messages() {
        let io = Error::Io(std::io::Error::other("/secret/path"));
        let db = Error::Db(rusqlite::Error::InvalidQuery);
        let cases: Vec<(Error, &str)> = vec![
            (Error::InvalidParams("x".into()), "invalid_params"),
            (Error::Pow("x".into()), "pow_rejected"),
            (Error::RateLimited, "rate_limited"),
            (Error::Unauthorized, "unauthorized"),
            (Error::NotFound, "not_found"),
            (Error::Capacity, "capacity"),
            (Error::Paused, "paused"),
            (Error::Config("x".into()), "internal"),
            (Error::Chain("10.0.0.1 refused".into()), "internal"),
            (Error::Transport("x".into()), "internal"),
            (io, "internal"),
            (db, "internal"),
        ];
        for (e, code) in cases {
            assert_eq!(e.code(), code);
            if code == "internal" {
                assert_eq!(e.public_message(), "internal error");
            } else {
                assert_eq!(e.public_message(), e.to_string());
            }
        }
    }
}
