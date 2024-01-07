use crate::{input, ui};

#[derive(Debug, clap::Parser)]
pub struct Options {
    /// Write logs to current directory.
    #[arg(long)]
    pub log: bool,

    /// Input sources.
    #[command(flatten)]
    pub inputs: input::Options,

    /// UI settings.
    #[command(flatten)]
    pub ui: ui::Options,
}
