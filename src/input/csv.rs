use std::path::Path;

use anyhow::{Result, Context as _};
use futures::{StreamExt as _, channel::{mpsc, oneshot}};
use tokio::{fs, io::{self, AsyncBufReadExt as _}};

fn parse_line(line: &[u8]) -> Result<Vec<String>> {
    let mut builder = csv::ReaderBuilder::new();
    builder.has_headers(false);
    let mut records = builder.from_reader(line).into_records();
    match records.next() {
        Some(record) => Ok(record.context("CSV parse error")?.into_iter().map(str::to_string).collect()),
        None => Ok(Vec::new()),
    }
}

async fn open_csv(path: &Path, mut new_name: impl FnMut(String) -> Result<mpsc::Sender<f64>>, shutdown: oneshot::Receiver<()>) -> Result<()> {
    let fd = fs::File::open(path).await.context("cannot open file for reading")?;
    let mut read = io::BufReader::new(fd);
    let mut line = String::new();
    read.read_line(&mut line).await.context("read header line")?;

    let columns = parse_line(line.as_bytes()).context("parse header line")?;

    let lines = read.lines();
    let senders = columns.iter().map(|column| {
        new_name(column.clone())
    }).collect::<Result<Vec<_>>>()?;

    tokio::spawn(async move {

    });

    Ok(())
}
