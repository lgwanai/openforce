use axum::http::StatusCode;

pub mod error {
    use super::StatusCode;
    pub fn tool_error_to_http(code: i32) -> StatusCode {
        match code {
            1 => StatusCode::CONFLICT,      // lease_invalid
            2 => StatusCode::CONFLICT,      // fencing_token_stale
            3 => StatusCode::FORBIDDEN,      // path_not_allowed
            4 => StatusCode::CONFLICT,      // snapshot_mismatch
            5 => StatusCode::ACCEPTED,       // approval_required -> 202
            6 => StatusCode::FORBIDDEN,      // approval_token_invalid
            7 => StatusCode::CONFLICT,      // approval_token_expired
            8 => StatusCode::CONFLICT,      // approval_token_already_used
            9 => StatusCode::ACCEPTED,       // sensitive_patch_detected -> 202
            10 => StatusCode::BAD_REQUEST,   // patch_rejected
            11 => StatusCode::CONFLICT,      // command_id_replayed
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn grpc_to_http(status: tonic::Code) -> StatusCode {
        match status {
            tonic::Code::InvalidArgument => StatusCode::BAD_REQUEST,
            tonic::Code::Unauthenticated => StatusCode::UNAUTHORIZED,
            tonic::Code::PermissionDenied => StatusCode::FORBIDDEN,
            tonic::Code::NotFound => StatusCode::NOT_FOUND,
            tonic::Code::AlreadyExists => StatusCode::CONFLICT,
            tonic::Code::FailedPrecondition => StatusCode::CONFLICT,
            tonic::Code::ResourceExhausted => StatusCode::TOO_MANY_REQUESTS,
            tonic::Code::DeadlineExceeded => StatusCode::GATEWAY_TIMEOUT,
            tonic::Code::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
