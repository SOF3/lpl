#![warn(clippy::pedantic)]
#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss, clippy::cast_possible_wrap)]

use anyhow::{Context as _, Result};
use clap::Parser as _;
use flexi_logger::FileSpec;
use tokio_util::sync::CancellationToken;

mod input;
mod options;
mod ui;
pub mod util;

#[tokio::main]
async fn main() -> Result<()> {
    let options = options::Options::parse();
    if options.log {
        flexi_logger::Logger::try_with_env()
            .context("parse RUST_LOG")?
            .log_to_file(FileSpec::default())
            .start()
            .context("logger setup")?;
        log::info!("start with options: {options:?}");
    }

    let cancel = CancellationToken::new();
    let input = options.inputs.open(&cancel).await?;
    ui::run(options.ui, input, cancel.clone()).await?;
    cancel.cancel();

    Ok(())
}
