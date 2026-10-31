//! The admin API error contract.
//!
//! [`Error`] is the error half of every admin handler `Result`. Its
//! `IntoResponse` (feature `axum`) emits the `{ "errorCode", "message" }`
//! envelope ([`ErrorResponse`]) that Delta Sharing and Unity Catalog servers
//! share. The type is independent of the recipient-facing protocol's error in
//! `opensharing-api`, and a host maps its own errors onto these variants.

/// Result type of the admin handlers and routes.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// The error half of every admin handler `Result`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The addressed share, recipient or provider does not exist. → 404.
    #[error("Not found")]
    NotFound,

    /// The caller may not perform this action. → 403.
    #[error("Not allowed")]
    NotAllowed,

    /// No or invalid credentials. → 401.
    #[error("Unauthenticated")]
    Unauthenticated,

    /// A malformed request or invalid parameter. → 400.
    #[error("Invalid Argument: {0}")]
    InvalidArgument(String),

    /// A request or response (de)serialization failure. → 500.
    #[error("Malformed response: {source}")]
    MalformedResponse {
        #[from]
        source: serde_json::Error,
    },

    /// An operation the host does not serve. → 501.
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),

    /// An unexpected failure. → 500.
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

    /// A 501 [`NotImplemented`](Error::NotImplemented).
    pub fn not_implemented(what: &'static str) -> Self {
        Error::NotImplemented(what)
    }

    /// The machine-readable error code, as in the Unity Catalog API.
    pub fn error_code(&self) -> &str {
        match self {
            Error::InvalidArgument(_) => "INVALID_PARAMETER_VALUE",
            Error::NotFound => "NOT_FOUND",
            Error::NotAllowed => "PERMISSION_DENIED",
            Error::Unauthenticated => "UNAUTHENTICATED",
            Error::NotImplemented(_) => "NOT_IMPLEMENTED",
            Error::MalformedResponse { .. } | Error::Generic(_) => "INTERNAL_ERROR",
        }
    }
}

/// The error body every admin endpoint returns.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    /// A machine-readable code, e.g. `NOT_FOUND` (see [`Error::error_code`]).
    pub error_code: String,
    pub message: String,
}

#[cfg(feature = "axum")]
mod server {
    use axum::extract::Json;
    use axum::http::StatusCode;
    use axum::response::{IntoResponse, Response};
    use tracing::error;

    use super::{Error, ErrorResponse};

    impl Error {
        fn status_and_message(self) -> (StatusCode, String) {
            match self {
                // Written for the caller (a duplicate name, a bad identifier, …).
                Error::InvalidArgument(message) => (StatusCode::BAD_REQUEST, message),
                Error::NotFound => (
                    StatusCode::NOT_FOUND,
                    "The requested resource was not found.".to_string(),
                ),
                Error::NotAllowed => (
                    StatusCode::FORBIDDEN,
                    "The caller is not permitted to perform this action.".to_string(),
                ),
                Error::Unauthenticated => (
                    StatusCode::UNAUTHORIZED,
                    "No or invalid provider credentials provided.".to_string(),
                ),
                Error::NotImplemented(what) => (StatusCode::NOT_IMPLEMENTED, what.to_string()),
                Error::MalformedResponse { source } => {
                    error!("Malformed response: {}", source);
                    internal()
                }
                Error::Generic(message) => {
                    error!("Generic error: {}", message);
                    internal()
                }
            }
        }
    }

    fn internal() -> (StatusCode, String) {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "The request is not handled correctly due to a server error.".to_string(),
        )
    }

    impl IntoResponse for Error {
        fn into_response(self) -> Response {
            let error_code = self.error_code().to_string();
            let (status, message) = self.status_and_message();
            (
                status,
                Json(ErrorResponse {
                    error_code,
                    message,
                }),
            )
                .into_response()
        }
    }
}

#[cfg(all(test, feature = "axum"))]
mod tests {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    use super::*;

    async fn render(error: Error) -> (StatusCode, ErrorResponse) {
        let response = error.into_response();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    #[tokio::test]
    async fn errors_render_the_shared_envelope() {
        let (status, body) = render(Error::NotFound).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body.error_code, "NOT_FOUND");

        let (status, body) = render(Error::invalid_argument("bad name")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body.error_code, "INVALID_PARAMETER_VALUE");
        assert_eq!(body.message, "bad name");

        let (status, body) = render(Error::Unauthenticated).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(body.message.contains("provider"));

        // Internal details are logged, not returned.
        let (status, body) = render(Error::generic("db password wrong")).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body.error_code, "INTERNAL_ERROR");
        assert!(!body.message.contains("password"));
    }

    #[test]
    fn the_envelope_is_camel_case() {
        let value = serde_json::to_value(ErrorResponse {
            error_code: "NOT_FOUND".into(),
            message: "m".into(),
        })
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({ "errorCode": "NOT_FOUND", "message": "m" })
        );
    }
}
