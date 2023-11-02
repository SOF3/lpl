use std::str::FromStr;

use anyhow::{Context as _, Error, Result};

use crate::{input, ui};

#[derive(clap::Parser)]
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

#[derive(Debug, Clone)]
pub struct Named<T> {
    pub name:  String,
    pub value: T,
}

impl<T: FromStr> FromStr for Named<T>
where
    Error: From<<T as FromStr>::Err>,
{
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let (name, value) = s.split_once('=').context("expected NAME=VALUE")?;
        Ok(Self { name: name.to_string(), value: value.parse()? })
    }
}
