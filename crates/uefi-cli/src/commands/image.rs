use crate::client::Client;
use crate::output::OutputFormat;
use uefi_common::error::AppError;
use uefi_common::state;

pub async fn open(
    path: &str,
    name: Option<&str>,
    mode: i32,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let mut st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st.clone()).await?;
    let (image_id, root_guid, confirmed_name) = client
        .image_open(path, name.unwrap_or(""), mode)
        .await?;
    st.active_image_id = Some(image_id.clone());
    state::write_state(&st)?;
    match format {
        OutputFormat::Json => println!(
            "{{\"image_id\":\"{image_id}\",\"root_guid\":\"{root_guid}\",\"name\":\"{confirmed_name}\"}}"
        ),
        _ => println!("{image_id}\t{root_guid}\t{confirmed_name}"),
    }
    Ok(())
}

pub async fn switch(image_id: &str, _format: OutputFormat) -> Result<(), AppError> {
    let mut st = state::require_state()?;
    st.active_image_id = Some(image_id.into());
    state::write_state(&st)?;
    Ok(())
}

pub async fn close(
    image_id: Option<&str>,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let mut st = state::require_state()?;
    let target_id = match image_id {
        Some(id) => id.to_string(),
        None => st.active_image_id.clone().ok_or_else(|| {
            AppError::new(
                uefi_common::error::ErrKind::NoActiveImage,
                "no active image; specify image_id explicitly",
            )
        })?,
    };
    let mut client = Client::connect(cli_sock, st.clone()).await?;
    client.image_close(&target_id).await?;
    if st.active_image_id.as_deref() == Some(target_id.as_str()) {
        st.active_image_id = None;
        state::write_state(&st)?;
    }
    crate::output::print_ok(format);
    Ok(())
}

pub async fn save(
    output: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    client.image_save(&image_id, output).await?;
    crate::output::print_ok(format);
    Ok(())
}

pub async fn list(cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let images = client.images_list().await?;
    crate::output::print_images_list(&images, format);
    Ok(())
}

pub async fn status(cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    let st = state::require_state()?;
    let active = st.active_image_id.clone().ok_or_else(|| {
        AppError::new(
            uefi_common::error::ErrKind::NoActiveImage,
            "no active image",
        )
    })?;
    let mut client = Client::connect(cli_sock, st).await?;
    let info = client.image_status(&active).await?;
    crate::output::print_image_status(&info, format);
    Ok(())
}
