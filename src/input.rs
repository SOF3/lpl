use std::path::PathBuf;
use std::pin::Pin;
use std::time::{Duration, SystemTime};
use std::{fmt, thread};

use anyhow::{Context as _, Result};
use arcstr::ArcStr;
use futures::channel::mpsc;
use futures::Future;
use tokio::fs;
use tokio_util::sync::CancellationToken;

use self::notifier::open_poll;

mod csv;
mod json;

mod notifier;

#[derive(Debug, clap::Args)]
#[group(id = "Inputs")]
pub struct Options {
    /// Read inputs from a CSV stream with an initial header line.
    #[clap(long)]
    pub csv:                Vec<PathBuf>,
    /// Poll new changes from a CSV file periodically.
    #[clap(long)]
    pub csv_poll:           Vec<String>,
    /// Delimiter used in CSV files
    #[clap(long, default_value_t = ',')]
    pub csv_poll_delimiter: char,

    /// Read inputs from a JSON Lines stream.
    #[clap(long)]
    pub json:      Vec<PathBuf>,
    /// Poll new changes from a JSON file periodically.
    #[clap(long)]
    pub json_poll: Vec<PathBuf>,

    /// The frequency of polling files for *-poll inputs in seconds.
    #[arg(long, value_parser = |v: &str| v.parse::<f32>().map(Duration::from_secs_f32), default_value = "1")]
    pub poll_period: Duration,
}

impl Options {
    pub async fn open(&self, cancel: &CancellationToken) -> Result<Input> {
        let (input_send, input_recv) = mpsc::channel(0);
        let (warn_send, warn_recv) = mpsc::channel(16);
        let warnings = WarningSender { prefix: ArcStr::default(), sender: warn_send };

        let mut workers = Vec::new();

        let watcher = notifier::start(warnings.with_prefix("inotify: "))?;

        for path in &self.json {
            let worker = json::open(path.clone(), &input_send)
                .await
                .with_context(|| format!("open {}", path.display()))?;
            workers.push((path.clone(), worker));
        }

        for path in &self.json_poll {
            let worker =
                open_poll(path.clone(), self.poll_period, &watcher, &input_send, json::PollParser)?;
            workers.push((path.clone(), worker));
        }

        for path in &self.csv {
            let worker = csv::open(path, &input_send, self.csv_poll_delimiter)
                .await
                .with_context(|| format!("open {}", path.display()))?;
            workers.push((path.clone(), worker));
        }

        for arg in &self.csv_poll {
            let (path, parser) = csv::Parser::new(arg, self.csv_poll_delimiter)?;
            let worker =
                open_poll(path.to_path_buf(), self.poll_period, &watcher, &input_send, parser)?;
            workers.push((path.to_path_buf(), worker));
        }

        for (path, worker) in workers {
            let mut warn_send = warnings.with_prefix(&format!("{}: ", path.display()));

            let worker = worker(warn_send.clone(), cancel.clone());
            tokio::spawn(async move {
                if let Err(err) = worker.await {
                    warn_send.send(format!("Error: {err}"));
                }
            });
        }

        Ok(Input {
            messages:       input_recv,
            warnings:       warn_recv,
            warning_sender: warnings,
        })
    }
}

#[derive(Clone)]
pub struct WarningSender {
    prefix: ArcStr,
    sender: mpsc::Sender<(SystemTime, String)>,
}

impl WarningSender {
    pub fn send(&mut self, message: impl fmt::Display) {
        let _ = self.sender.try_send((SystemTime::now(), format!("{}{message}", &self.prefix)));
    }

    pub fn with_prefix(&self, prefix: &str) -> Self {
        Self { prefix: arcstr::format!("{prefix}{}", &self.prefix), sender: self.sender.clone() }
    }
}

pub struct Input {
    pub messages:       mpsc::Receiver<Message>,
    pub warnings:       mpsc::Receiver<(SystemTime, String)>,
    pub warning_sender: WarningSender,
}

#[derive(Debug)]
pub struct Message {
    pub label: String,
    pub value: f64,
    pub time:  SystemTime,
}

type WorkerBuilder = Box<dyn FnOnce(WarningSender, CancellationToken) -> Worker>;
type Worker = Pin<Box<dyn Future<Output = Result<()>> + Send>>;

/// Workaround for tokio workers unable to perform non-blocking reads on non-regular files.
async fn thread_line_reader(
    tokio_file: fs::File,
    cancel: CancellationToken,
    mut warn_send: WarningSender,
) -> tokio::sync::mpsc::Receiver<(String, SystemTime)> {
    let (send, recv) = tokio::sync::mpsc::channel(1);

    let std_file = tokio_file.into_std().await;
    thread::spawn(move || {
        let mut buf = std::io::BufReader::new(std_file);
        while !cancel.is_cancelled() {
            use std::io::BufRead as _;

            let mut line = String::new();
            match buf.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => drop(send.blocking_send((line, SystemTime::now()))),
                Err(err) => warn_send.send(format!("{err:?}")),
            }
        }
    });

    recv
}
