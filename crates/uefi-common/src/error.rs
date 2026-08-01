use std::process::ExitCode as StdExitCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    Success,
    Cli,
    Rpc,
    State,
}

impl ExitCode {
    pub fn as_u8(self) -> u8 {
        match self {
            ExitCode::Success => 0,
            ExitCode::Cli => 1,
            ExitCode::Rpc => 2,
            ExitCode::State => 3,
        }
    }

    pub fn to_std(self) -> StdExitCode {
        StdExitCode::from(self.as_u8())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrKind {
    StateMissing,
    StateCorrupt,
    NoActiveImage,
    RpcUnauthenticated,
    RpcNotFound,
    RpcInvalidArgument,
    RpcInternal,
    IoError,
}

impl ErrKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrKind::StateMissing => "STATE_MISSING",
            ErrKind::StateCorrupt => "STATE_CORRUPT",
            ErrKind::NoActiveImage => "NO_ACTIVE_IMAGE",
            ErrKind::RpcUnauthenticated => "RPC_UNAUTHENTICATED",
            ErrKind::RpcNotFound => "RPC_NOT_FOUND",
            ErrKind::RpcInvalidArgument => "RPC_INVALID_ARGUMENT",
            ErrKind::RpcInternal => "RPC_INTERNAL",
            ErrKind::IoError => "IO_ERROR",
        }
    }
}

#[derive(Debug)]
pub struct AppError {
    pub kind: ErrKind,
    pub message: String,
}

impl AppError {
    pub fn new(kind: ErrKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn exit_code(&self) -> ExitCode {
        match self.kind {
            ErrKind::StateMissing | ErrKind::StateCorrupt | ErrKind::NoActiveImage => {
                ExitCode::State
            }
            ErrKind::RpcUnauthenticated
            | ErrKind::RpcNotFound
            | ErrKind::RpcInvalidArgument
            | ErrKind::RpcInternal => ExitCode::Rpc,
            ErrKind::IoError => ExitCode::Cli,
        }
    }
}

impl From<tonic::Status> for AppError {
    fn from(s: tonic::Status) -> Self {
        let kind = match s.code() {
            tonic::Code::Unauthenticated => ErrKind::RpcUnauthenticated,
            tonic::Code::NotFound => ErrKind::RpcNotFound,
            tonic::Code::InvalidArgument => ErrKind::RpcInvalidArgument,
            _ => ErrKind::RpcInternal,
        };
        AppError::new(kind, s.message().to_string())
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.kind.as_str())
    }
}

impl std::error::Error for AppError {}

pub fn print_error(err: &AppError, format_json: bool) {
    if format_json {
        eprintln!(
            "{{\"error\":\"{}\",\"code\":\"{}\"}}",
            err.message.replace('"', "\\\""),
            err.kind.as_str()
        );
    } else {
        eprintln!("error: {} ({})", err.message, err.kind.as_str());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_mapping() {
        assert_eq!(
            AppError::new(ErrKind::StateMissing, "x")
                .exit_code()
                .as_u8(),
            3
        );
        assert_eq!(
            AppError::new(ErrKind::RpcNotFound, "x").exit_code().as_u8(),
            2
        );
        assert_eq!(AppError::new(ErrKind::IoError, "x").exit_code().as_u8(), 1);
    }

    #[test]
    fn from_tonic_status() {
        let s = tonic::Status::not_found("missing");
        let e: AppError = s.into();
        assert_eq!(e.kind, ErrKind::RpcNotFound);
    }
}
