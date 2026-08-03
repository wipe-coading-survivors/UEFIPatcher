#[tokio::main]
async fn main() -> anyhow::Result<()> {
    uefi_gateway::main_inner().await
}
