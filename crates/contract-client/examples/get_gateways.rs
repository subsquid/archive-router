use clap::Parser;
use libp2p::PeerId;
use simple_logger::SimpleLogger;

use contract_client;
use contract_client::RpcArgs;

#[derive(Parser)]
struct Cli {
    #[command(flatten)]
    rpc: RpcArgs,
    worker_id: PeerId,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .env()
        .init()?;
    let cli = Cli::parse();

    let client = contract_client::get_client(&cli.rpc).await?;
    let on_chain_id = client.worker_id(cli.worker_id).await?;
    let clusters = client.all_gateways(on_chain_id).await?;
    clusters.iter().for_each(|c| println!("{c:?}"));
    Ok(())
}
