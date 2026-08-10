use crate::client::Client;
use crate::output::OutputFormat;
use uefi_common::error::{AppError, ErrKind};
use uefi_common::state::{self, State};

pub async fn init(
    name: Option<&str>,
    cli_sock: Option<&str>,
    force: bool,
    format: OutputFormat,
) -> Result<(), AppError> {
    let existing = state::read_state()?;
    if existing.session_id.is_some() && !force {
        return Err(AppError::new(
            ErrKind::StateCorrupt,
            "state already exists; use --force to overwrite",
        ));
    }
    let st = State::default();
    let mut client = Client::connect(cli_sock, st).await?;
    let (sid, tok) = client.session_create(name.unwrap_or("")).await?;
    let sock = state::resolve_sock(cli_sock, &State::default());
    let new_state = State {
        session_id: Some(sid.clone()),
        token: Some(tok.clone()),
        active_image_id: None,
        sock_path: Some(sock.display().to_string()),
    };
    state::write_state(&new_state)?;
    crate::output::print_session_created(&sid, &tok, format);
    Ok(())
}

pub async fn list(cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let rows = client.sessions_list().await?;
    crate::output::print_sessions(&rows, format);
    Ok(())
}

pub async fn destroy(cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    let st = state::require_state()?;
    let sid = st
        .session_id
        .clone()
        .ok_or_else(|| AppError::new(ErrKind::StateMissing, "no session in state"))?;
    let mut client = Client::connect(cli_sock, st).await?;
    client.session_destroy(&sid).await?;
    let _ = std::fs::remove_file(state::state_path());
    crate::output::print_ok(format);
    Ok(())
}
