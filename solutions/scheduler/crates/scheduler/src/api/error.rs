use connectrpc::ConnectError;

use crate::domain::ErrorKind;
use crate::store::StoreError;

use super::text::error_text;

impl From<StoreError> for ConnectError {
    fn from(error: StoreError) -> Self {
        let message = error.to_string();
        let detail = home_proto::error_detail(&error_text(&error));
        let mapped = match error.kind() {
            Some(ErrorKind::NotFound) => Self::not_found(message),
            Some(ErrorKind::InvalidArgument) => Self::invalid_argument(message),
            Some(ErrorKind::Conflict) => Self::aborted(message),
            Some(ErrorKind::Idempotency) => Self::already_exists(message),
            Some(ErrorKind::Precondition) => Self::failed_precondition(message),
            Some(ErrorKind::LimitExceeded) => Self::resource_exhausted(message),
            Some(ErrorKind::ConsumerDenied) => Self::permission_denied(message),
            None => Self::internal("internal error"),
        };
        mapped.with_detail(detail)
    }
}
