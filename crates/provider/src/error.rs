//! The server's error type.
//!
//! The server serves two surfaces with separate error contracts: the
//! recipient-facing protocol ([`opensharing_api::Error`]) and, with the `admin`
//! feature, the provider admin API (`opensharing_admin_api::Error`). Its own
//! code (stores, secrets, sources, storage access) returns this [`Error`],
//! which converts into either one, so `?` works in handlers of both surfaces.
//! It has no `IntoResponse`. Each route returns its own surface's `Result`.

/// Result type of the server's own code.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// An error from the server's own code. See the module docs.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The addressed object does not exist.
    #[error("Not found")]
    NotFound,

    /// The caller may not perform this action.
    #[error("Not allowed")]
    NotAllowed,

    /// No or invalid credentials.
    #[error("Unauthenticated")]
    Unauthenticated,

    /// A malformed request or invalid parameter; the message is for the caller.
    #[error("Invalid Argument: {0}")]
    InvalidArgument(String),

    /// A feature this server does not serve.
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),

    /// A (de)serialization failure.
    #[error("Malformed data: {source}")]
    Serde {
        #[from]
        source: serde_json::Error,
    },

    /// A malformed URL.
    #[error("Malformed url: {source}")]
    Url {
        #[from]
        source: url::ParseError,
    },

    /// An error from the protocol crate without a counterpart here, passed
    /// through unchanged to the protocol surface.
    #[error(transparent)]
    Api(opensharing_api::Error),

    /// An unexpected failure.
    #[error("Generic error: {0}")]
    Generic(String),
}

impl Error {
    pub fn generic(message: impl ToString) -> Self {
        Error::Generic(message.to_string())
    }

    pub fn invalid_argument(message: impl ToString) -> Self {
        Error::InvalidArgument(message.to_string())
    }

    pub fn not_implemented(what: &'static str) -> Self {
        Error::NotImplemented(what)
    }
}

impl From<opensharing_api::Error> for Error {
    fn from(err: opensharing_api::Error) -> Self {
        use opensharing_api::Error as Api;
        match err {
            Api::NotFound => Error::NotFound,
            Api::NotAllowed => Error::NotAllowed,
            Api::Unauthenticated => Error::Unauthenticated,
            Api::InvalidArgument(message) => Error::InvalidArgument(message),
            Api::NotImplemented(what) => Error::NotImplemented(what),
            Api::Generic(message) => Error::Generic(message),
            other => Error::Api(other),
        }
    }
}

impl From<Error> for opensharing_api::Error {
    fn from(err: Error) -> Self {
        use opensharing_api::Error as Api;
        match err {
            Error::NotFound => Api::NotFound,
            Error::NotAllowed => Api::NotAllowed,
            Error::Unauthenticated => Api::Unauthenticated,
            Error::InvalidArgument(message) => Api::InvalidArgument(message),
            Error::NotImplemented(what) => Api::NotImplemented(what),
            Error::Serde { source } => Api::MalformedResponse { source },
            Error::Url { source } => Api::MalformedUrl { source },
            Error::Api(err) => err,
            Error::Generic(message) => Api::Generic(message),
        }
    }
}

#[cfg(feature = "admin")]
impl From<Error> for opensharing_admin_api::Error {
    fn from(err: Error) -> Self {
        use opensharing_admin_api::Error as Admin;
        match err {
            Error::NotFound => Admin::NotFound,
            Error::NotAllowed => Admin::NotAllowed,
            Error::Unauthenticated => Admin::Unauthenticated,
            Error::InvalidArgument(message) => Admin::InvalidArgument(message),
            Error::NotImplemented(what) => Admin::NotImplemented(what),
            Error::Serde { source } => Admin::MalformedResponse { source },
            // Protocol-crate errors keep their caller-facing meaning where the
            // admin contract has one.
            Error::Api(err) => match Error::from(err) {
                Error::Api(err) => match err {
                    opensharing_api::Error::InvalidPredicate(message) => {
                        Admin::InvalidArgument(message)
                    }
                    other => Admin::Generic(other.to_string()),
                },
                err => err.into(),
            },
            Error::Url { source } => Admin::Generic(source.to_string()),
            Error::Generic(message) => Admin::Generic(message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_keep_their_meaning_on_both_surfaces() {
        let api: opensharing_api::Error = Error::NotFound.into();
        assert!(matches!(api, opensharing_api::Error::NotFound));
        let back: Error = opensharing_api::Error::invalid_argument("x").into();
        assert!(matches!(back, Error::InvalidArgument(m) if m == "x"));
        // Protocol-only variants pass through to the protocol unchanged.
        let passed: Error = opensharing_api::Error::invalid_predicate("p").into();
        assert!(matches!(
            opensharing_api::Error::from(passed),
            opensharing_api::Error::InvalidPredicate(_)
        ));
    }

    #[cfg(feature = "admin")]
    #[test]
    fn errors_map_onto_the_admin_contract() {
        let admin: opensharing_admin_api::Error = Error::Unauthenticated.into();
        assert!(matches!(
            admin,
            opensharing_admin_api::Error::Unauthenticated
        ));
        let admin: opensharing_admin_api::Error =
            Error::from(opensharing_api::Error::invalid_predicate("p")).into();
        assert!(matches!(
            admin,
            opensharing_admin_api::Error::InvalidArgument(_)
        ));
    }
}
