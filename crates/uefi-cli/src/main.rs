mod client;
mod commands;
mod output;

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use uefi_common::error;

#[derive(Parser)]
#[command(name = "uefi-cli", version, about = "UEFIPatcher CLI")]
struct Cli {
    #[arg(long, env = "UEFIPATCHER_SOCK")]
    sock: Option<String>,
    #[arg(long, default_value = "json")]
    format: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Session {
        #[command(subcommand)]
        sub: SessionCmd,
    },
    Image {
        #[command(subcommand)]
        sub: ImageCmd,
    },
    Edit {
        #[command(subcommand)]
        sub: EditCmd,
    },
    Setup {
        #[command(subcommand)]
        sub: SetupCmd,
    },
}

#[derive(Subcommand)]
enum SessionCmd {
    Init {
        #[arg(long)]
        force: bool,
    },
    List,
    Destroy,
}

#[derive(Subcommand)]
enum ImageCmd {
    Open {
        path: String,
        #[arg(long, default_value = "read")]
        mode: String,
    },
    Switch {
        image_id: String,
    },
    Close,
    Dump {
        #[arg(long, default_value = "text")]
        format: String,
    },
    List {
        #[arg(long)]
        filter: Option<String>,
    },
    Find {
        target: String,
    },
    Search {
        query: String,
        #[arg(long, value_enum, num_args = 0.., default_values_t = vec![SearchModeCli::Name])]
        mode: Vec<SearchModeCli>,
        #[arg(long, default_value = "100")]
        limit: u32,
    },
    Save {
        output: String,
    },
    Extract {
        target: String,
        #[arg(long)]
        body_only: bool,
    },
    Export {
        artifact_id: String,
        output_path: Option<String>,
    },
    Import {
        file_path: String,
    },
    Artifacts,
}

#[derive(Subcommand)]
enum EditCmd {
    Insert {
        target: String,
        ffs: String,
        #[arg(long)]
        from_artifact: Option<String>,
        #[arg(long, default_value = "into")]
        mode: String,
    },
    Remove {
        target: String,
    },
    Replace {
        target: String,
        data: String,
        #[arg(long)]
        from_artifact: Option<String>,
        #[arg(long)]
        body_only: bool,
    },
    Rebuild {
        target: String,
    },
}

#[derive(Subcommand)]
enum SetupCmd {
    SetVisibility {
        item_id: String,
        #[arg(long, conflicts_with = "hidden")]
        visible: bool,
        #[arg(long)]
        hidden: bool,
    },
    ListItems,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum SearchModeCli {
    Name,
    Utf8,
    Utf16,
    Bytes,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let format = match output::parse_format(&cli.format) {
        Ok(f) => f,
        Err(e) => {
            error::print_error(&e, false);
            return ExitCode::from(1);
        }
    };
    let json_err = matches!(format, output::OutputFormat::Json);
    let result = dispatch(&cli, format).await;
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error::print_error(&e, json_err);
            e.exit_code().to_std()
        }
    }
}

async fn dispatch(cli: &Cli, format: output::OutputFormat) -> Result<(), error::AppError> {
    let sock = cli.sock.as_deref();
    match &cli.cmd {
        Cmd::Session { sub } => match sub {
            SessionCmd::Init { force } => commands::session::init(sock, *force, format).await,
            SessionCmd::List => commands::session::list(sock, format).await,
            SessionCmd::Destroy => commands::session::destroy(sock, format).await,
        },
        Cmd::Image { sub } => match sub {
            ImageCmd::Open { path, mode } => commands::image::open(path, mode, sock, format).await,
            ImageCmd::Switch { image_id } => commands::image::switch(image_id, format).await,
            ImageCmd::Close => commands::image::close(format).await,
            ImageCmd::Dump { format: dfmt } => commands::image::dump(dfmt, sock, format).await,
            ImageCmd::List { filter } => {
                commands::image::list(filter.as_deref(), sock, format).await
            }
            ImageCmd::Find { target } => commands::image::find(target, sock, format).await,
            ImageCmd::Search { query, mode, limit } => {
                let modes: Vec<i32> = mode
                    .iter()
                    .map(|m| match m {
                        SearchModeCli::Name => uefi_proto::SearchMode::Name as i32,
                        SearchModeCli::Utf8 => uefi_proto::SearchMode::Utf8 as i32,
                        SearchModeCli::Utf16 => uefi_proto::SearchMode::Utf16 as i32,
                        SearchModeCli::Bytes => uefi_proto::SearchMode::Bytes as i32,
                    })
                    .collect();
                commands::image::search(query, &modes, *limit, sock, format).await
            }
            ImageCmd::Save { output } => commands::image::save(output, sock, format).await,
            ImageCmd::Extract { target, body_only } => {
                commands::image::extract(target, *body_only, sock, format).await
            }
            ImageCmd::Export {
                artifact_id,
                output_path,
            } => commands::image::export(artifact_id, output_path.as_deref(), sock, format).await,
            ImageCmd::Import { file_path } => {
                commands::image::import(file_path, sock, format).await
            }
            ImageCmd::Artifacts => commands::image::artifacts(sock, format).await,
        },
        Cmd::Edit { sub } => match sub {
            EditCmd::Insert {
                target,
                ffs,
                from_artifact,
                mode,
            } => {
                commands::edit::insert(target, ffs, from_artifact.as_deref(), mode, sock, format)
                    .await
            }
            EditCmd::Remove { target } => commands::edit::remove(target, sock, format).await,
            EditCmd::Replace {
                target,
                data,
                from_artifact,
                body_only,
            } => {
                commands::edit::replace(
                    target,
                    data,
                    from_artifact.as_deref(),
                    *body_only,
                    sock,
                    format,
                )
                .await
            }
            EditCmd::Rebuild { target } => commands::edit::rebuild(target, sock, format).await,
        },
        Cmd::Setup { sub } => match sub {
            SetupCmd::SetVisibility {
                item_id,
                visible,
                hidden,
            } => {
                let vis = if *hidden { false } else { *visible };
                commands::setup::set_visibility(item_id, vis, sock, format).await
            }
            SetupCmd::ListItems => commands::setup::list_items(sock, format).await,
        },
    }
}
