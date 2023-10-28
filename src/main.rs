use anyhow::Result;
use clap::Parser as _;
use tokio_util::sync::CancellationToken;

mod input;
mod options;
mod ui;
pub mod util;

#[tokio::main]
async fn main() -> Result<()> {
    let options = options::Options::parse();

    let cancel = CancellationToken::new();
    let input = options.inputs.open(&cancel).await?;
    ui::run(options.ui, input, cancel.clone()).await?;
    cancel.cancel();

    Ok(())
}
