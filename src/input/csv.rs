use std::path::Path;
use std::time::SystemTime;
use std::{iter, mem};

use anyhow::{Context as _, Result};
use futures::channel::mpsc;
use futures::SinkExt;
use tokio::fs;
use tokio::io::{self, AsyncBufReadExt as _};

use super::notifier::FieldParser;
use super::{Message, WorkerBuilder};

fn parse_line(line: &[u8], delimiter: Delimiter) -> Result<Vec<String>> {
    let mut records = csv::ReaderBuilder::new()
        .has_headers(false)
        .delimiter(delimiter.0)
        .from_reader(line)
        .into_records();
    match records.next() {
        Some(record) => {
            Ok(record.context("CSV parse error")?.into_iter().map(str::to_string).collect())
        }
        None => Ok(Vec::new()),
    }
}

#[derive(Clone, Copy)]
struct Delimiter(u8);

impl Delimiter {
    fn new(delimiter: char) -> Result<Self> {
        anyhow::ensure!(
            delimiter.is_ascii(),
            "--csv-poll-delimiter must be a single ASCII character"
        );
        Ok(Self(delimiter as u8))
    }
}

pub async fn open(
    path: &Path,
    send: &mpsc::Sender<Message>,
    delimiter: char,
) -> Result<WorkerBuilder> {
    let delimiter = Delimiter::new(delimiter)?;

    let mut fd = fs::File::open(path).await.context("cannot open file for reading")?;

    let labels = {
        let mut read = io::BufReader::new(&mut fd);
        let mut line = String::new();
        read.read_line(&mut line).await.context("read header line")?;
        parse_line(line.as_bytes(), delimiter).context("parse header line")?
    };

    let mut send = send.clone();

    let parser = Parser { labels, delimiter };

    Ok(Box::new(move |mut warnings, cancel| {
        Box::pin(async move {
            let mut read = super::thread_line_reader(fd, cancel, warnings.clone()).await;

            while let Some((line, time)) = read.recv().await {
                if let Err(err) = parser.send_fields(time, &line, &mut send, |_| true).await {
                    warnings.send(format!("Error: {err:?}"));
                }
            }

            Ok(())
        })
    }))
}

pub struct Parser {
    labels:    Vec<String>,
    delimiter: Delimiter,
}

impl Parser {
    pub fn new(arg: &str, delimiter: char) -> Result<(&Path, Self)> {
        let delimiter = Delimiter::new(delimiter)?;

        let (header, path) = arg.split_once('=').context(
            "--csv-poll argument should be in the form `column1,column2,column3=path/to/csv`",
        )?;
        let labels = parse_line(header.as_bytes(), delimiter)?;
        Ok((Path::new(path), Parser { labels, delimiter }))
    }

    async fn send_fields(
        &self,
        time: SystemTime,
        line: &str,
        send: &mut mpsc::Sender<Message>,
        mut admit: impl FnMut(usize) -> bool,
    ) -> Result<()> {
        if line.is_empty() {
            return Ok(());
        }

        let line = parse_line(line.as_bytes(), self.delimiter)?;
        for (column_id, (label, value)) in iter::zip(&self.labels, line).enumerate() {
            if let Ok(value) = value.parse() {
                if admit(column_id) {
                    send.feed(Message { label: label.clone(), value, time }).await?;
                }
            }
        }

        Ok(())
    }
}

impl FieldParser for Parser {
    async fn parse(
        &self,
        time: SystemTime,
        content: &str,
        send: &mut mpsc::Sender<Message>,
    ) -> Result<()> {
        let mut dedup = vec![false; self.labels.len()];
        for line in content.lines() {
            self.send_fields(time, line, send, |column_id| {
                let existed = mem::replace(&mut dedup[column_id], true);
                !existed
            })
            .await?;
        }

        Ok(())
    }
}
