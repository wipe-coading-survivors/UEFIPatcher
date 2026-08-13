use clap::Parser;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "uefi-engine", version, about = "UEFIPatcher engine server")]
struct Args {
    #[arg(long, env = "UEFIPATCHER_DATA")]
    data_dir: Option<PathBuf>,
    #[arg(long, env = "UEFIPATCHER_SOCK")]
    sock: Option<PathBuf>,
    #[arg(long, env = "UEFIPATCHER_SESSION_TTL_SECS", default_value = "864000")]
    ttl: u64,
    #[arg(
        long,
        env = "UEFIPATCHER_SESSION_GC_INTERVAL_SECS",
        default_value = "3600"
    )]
    gc_interval: u64,
    #[arg(long, env = "UEFIPATCHER_PURGE_ARTIFACTS", default_value = "false")]
    purge_artifacts: bool,
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count)]
    verbose: u8,
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    uefi_engine::logging::init(args.verbose, args.quiet);
    let data_dir = args.data_dir.unwrap_or_else(|| {
        directories::ProjectDirs::from("", "", "uefipatcher")
            .map(|d| d.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("./data"))
    });
    let sock = args
        .sock
        .unwrap_or_else(|| PathBuf::from("/run/uefipatcher.sock"));
    std::fs::create_dir_all(&data_dir)?;
    if sock.parent().is_some() {
        let _ = std::fs::remove_file(&sock);
    }
    let db = uefi_engine::storage::open_db(&data_dir.join("uefipatcher.db"))?;
    tracing::info!(
        "Starting engine: sock={}, data={}, purge_artifacts={}",
        sock.display(),
        data_dir.display(),
        args.purge_artifacts
    );
    uefi_engine::rpc::server::serve(
        &sock,
        db,
        data_dir,
        Duration::from_secs(args.ttl),
        Duration::from_secs(args.gc_interval),
        args.purge_artifacts,
    )
}
