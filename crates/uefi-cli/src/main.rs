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
    #[arg(long, default_value = "text", value_enum)]
    format: OutputFormatCli,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum OutputFormatCli {
    Json,
    Text,
    Tsv,
}

impl From<OutputFormatCli> for output::OutputFormat {
    fn from(c: OutputFormatCli) -> Self {
        match c {
            OutputFormatCli::Json => output::OutputFormat::Json,
            OutputFormatCli::Text => output::OutputFormat::Text,
            OutputFormatCli::Tsv => output::OutputFormat::Tsv,
        }
    }
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum ImageModeCli {
    Read,
    Write,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum InsertModeCli {
    Into,
    Before,
    After,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum SearchModeCli {
    Name,
    Utf8,
    Utf16,
    Bytes,
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
    Node {
        #[command(subcommand)]
        sub: NodeCmd,
    },
    Artifact {
        #[command(subcommand)]
        sub: ArtifactCmd,
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
        name: Option<String>,
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
        #[arg(long)]
        name: Option<String>,
        #[arg(long, value_enum, default_value_t = ImageModeCli::Read)]
        mode: ImageModeCli,
    },
    Switch {
        image_id: String,
    },
    Close {
        image_id: Option<String>,
    },
    Save {
        output: String,
    },
    List,
    Status,
}

#[derive(Subcommand)]
enum NodeCmd {
    List {
        #[arg(long)]
        filter: Option<String>,
        #[arg(long)]
        tree: bool,
    },
    Search {
        query: String,
        #[arg(long, value_enum, num_args = 0.., default_values_t = vec![SearchModeCli::Name])]
        mode: Vec<SearchModeCli>,
        #[arg(long, default_value = "100")]
        limit: u32,
    },
    Insert {
        target: String,
        #[arg(long, group = "source")]
        file: Option<String>,
        #[arg(long, group = "source")]
        artifact: Option<String>,
        #[arg(long, value_enum, default_value_t = InsertModeCli::Into)]
        mode: InsertModeCli,
    },
    Remove {
        target: String,
    },
    Replace {
        target: String,
        #[arg(long, group = "source")]
        file: Option<String>,
        #[arg(long, group = "source")]
        artifact: Option<String>,
        #[arg(long)]
        body_only: bool,
    },
    Rebuild {
        target: String,
    },
    Extract {
        target: String,
        #[arg(long)]
        body_only: bool,
    },
}

#[derive(Subcommand)]
enum ArtifactCmd {
    List,
    Import {
        path: String,
    },
    Export {
        artifact_id: String,
        output_path: Option<String>,
    },
}

#[derive(Subcommand)]
enum SetupCmd {
    Form {
        #[command(subcommand)]
        sub: SetupFormCmd,
    },
    String {
        #[command(subcommand)]
        sub: SetupStringCmd,
    },
}

#[derive(Subcommand)]
enum SetupFormCmd {
    List,
    SetVisibility {
        form_id: String,
        #[arg(long, conflicts_with = "hidden")]
        visible: bool,
        #[arg(long)]
        hidden: bool,
    },
}

#[derive(Subcommand)]
enum SetupStringCmd {
    List,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let format: output::OutputFormat = cli.format.into();
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
            SessionCmd::Init { name, force } => {
                commands::session::init(name.as_deref(), sock, *force, format).await
            }
            SessionCmd::List => commands::session::list(sock, format).await,
            SessionCmd::Destroy => commands::session::destroy(sock, format).await,
        },
        Cmd::Image { sub } => match sub {
            ImageCmd::Open { path, name, mode } => {
                let mode_i = match mode {
                    ImageModeCli::Read => 0,
                    ImageModeCli::Write => 1,
                };
                commands::image::open(path, name.as_deref(), mode_i, sock, format).await
            }
            ImageCmd::Switch { image_id } => commands::image::switch(image_id, format).await,
            ImageCmd::Close { image_id } => {
                commands::image::close(image_id.as_deref(), sock, format).await
            }
            ImageCmd::Save { output } => commands::image::save(output, sock, format).await,
            ImageCmd::List => commands::image::list(sock, format).await,
            ImageCmd::Status => commands::image::status(sock, format).await,
        },
        Cmd::Node { sub } => match sub {
            NodeCmd::List { filter, tree } => {
                commands::node::list(filter.as_deref(), *tree, sock, format).await
            }
            NodeCmd::Search { query, mode, limit } => {
                let modes: Vec<i32> = mode
                    .iter()
                    .map(|m| match m {
                        SearchModeCli::Name => uefi_proto::SearchMode::Name as i32,
                        SearchModeCli::Utf8 => uefi_proto::SearchMode::Utf8 as i32,
                        SearchModeCli::Utf16 => uefi_proto::SearchMode::Utf16 as i32,
                        SearchModeCli::Bytes => uefi_proto::SearchMode::Bytes as i32,
                    })
                    .collect();
                commands::node::search(query, &modes, *limit, sock, format).await
            }
            NodeCmd::Insert {
                target,
                file,
                artifact,
                mode,
            } => {
                let mode_s = match mode {
                    InsertModeCli::Into => "into",
                    InsertModeCli::Before => "before",
                    InsertModeCli::After => "after",
                };
                commands::node::insert(
                    target,
                    file.as_deref(),
                    artifact.as_deref(),
                    mode_s,
                    sock,
                    format,
                )
                .await
            }
            NodeCmd::Remove { target } => commands::node::remove(target, sock, format).await,
            NodeCmd::Replace {
                target,
                file,
                artifact,
                body_only,
            } => {
                commands::node::replace(
                    target,
                    file.as_deref(),
                    artifact.as_deref(),
                    *body_only,
                    sock,
                    format,
                )
                .await
            }
            NodeCmd::Rebuild { target } => commands::node::rebuild(target, sock, format).await,
            NodeCmd::Extract { target, body_only } => {
                commands::node::extract(target, *body_only, sock, format).await
            }
        },
        Cmd::Artifact { sub } => match sub {
            ArtifactCmd::List => commands::artifact::list(sock, format).await,
            ArtifactCmd::Import { path } => commands::artifact::import(path, sock, format).await,
            ArtifactCmd::Export {
                artifact_id,
                output_path,
            } => {
                commands::artifact::export(artifact_id, output_path.as_deref(), sock, format).await
            }
        },
        Cmd::Setup { sub } => match sub {
            SetupCmd::Form { sub } => match sub {
                SetupFormCmd::List => commands::setup::form_list(sock, format).await,
                SetupFormCmd::SetVisibility {
                    form_id,
                    visible,
                    hidden,
                } => {
                    let vis = if *hidden { false } else { *visible };
                    commands::setup::form_set_visibility(form_id, vis, sock, format).await
                }
            },
            SetupCmd::String { sub } => match sub {
                SetupStringCmd::List => commands::setup::string_list(sock, format).await,
            },
        },
    }
}
