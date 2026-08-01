mod client;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "uefi-cli", version)]
struct Cli {
    #[arg(
        long,
        env = "UEFIPATCHER_SOCK",
        default_value = "/run/uefipatcher.sock"
    )]
    sock: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Session {
        #[command(subcommand)]
        sub: SessionCmd,
    },
    Open {
        session_id: String,
        path: PathBuf,
        #[arg(long, default_value = "read")]
        mode: String,
    },
    Dump {
        image_id: String,
        #[arg(long, default_value = "text")]
        format: String,
    },
    List {
        image_id: String,
        #[arg(long)]
        filter: Option<String>,
    },
    Find {
        image_id: String,
        target: String,
    },
    Insert {
        image_id: String,
        target: String,
        ffs: PathBuf,
        #[arg(long, default_value = "into")]
        mode: String,
    },
    Remove {
        image_id: String,
        target: String,
    },
    Replace {
        image_id: String,
        target: String,
        ffs: PathBuf,
        #[arg(long)]
        body_only: bool,
    },
    Rebuild {
        image_id: String,
        target: String,
    },
    SetVisibility {
        image_id: String,
        item_id: String,
        #[arg(long, default_value = "true")]
        visible: bool,
    },
    Save {
        image_id: String,
        output: PathBuf,
    },
}

#[derive(Subcommand)]
enum SessionCmd {
    Create,
    Destroy { id: String },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let mut c = client::Client::connect(&cli.sock).await?;
    match cli.cmd {
        Cmd::Session {
            sub: SessionCmd::Create,
        } => {
            let name = std::env::var("PWD").unwrap_or_default();
            let (id, tok) = c.create_session(&name).await?;
            println!("{id}\t{tok}");
        }
        Cmd::Session {
            sub: SessionCmd::Destroy { id },
        } => {
            c.destroy_session(&id).await?;
        }
        _ => {
            anyhow::bail!("command not fully implemented in smoke CLI");
        }
    }
    Ok(())
}
