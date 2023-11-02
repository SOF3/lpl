use std::fmt;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::result::Result as StdResult;
use std::time::{Duration, SystemTime};

use anyhow::{Context as _, Result};
use futures::channel::mpsc;
use futures::{select, FutureExt as _, SinkExt as _};
use serde::{de, Deserialize};
use tokio::{fs, time};

use super::notifier::Notifier;
use super::{Message, WorkerBuilder};

pub async fn open(path: PathBuf, send: &mpsc::Sender<Message>) -> Result<WorkerBuilder> {
    let fd = fs::File::open(&path).await.context("cannot open file for reading")?;
    let mut send = send.clone();

    Ok(Box::new(move |mut warnings, cancel| {
        Box::pin(async move {
            let mut read = super::thread_line_reader(fd, cancel, warnings.clone()).await;

            while let Some((line, time)) = read.recv().await {
                if let Err(err) = send_fields(time, &line, &mut send).await {
                    warnings.send(format!("Error: {err:?}"));
                }
            }

            Ok(())
        })
    }))
}

pub async fn open_poll(
    path: PathBuf,
    poll_period: Duration,
    notifier: &Notifier<impl notify::Watcher + Send + Sync + 'static>,
    send: &mpsc::Sender<Message>,
) -> Result<WorkerBuilder> {
    async fn read_once(path: &Path, send: &mut mpsc::Sender<Message>) -> Result<()> {
        let contents = fs::read_to_string(path).await.context("reading file contents")?;
        let time = SystemTime::now();
        send_fields(time, &contents, send).await.context("send fields")
    }

    let mut send = send.clone();
    let mut watcher = notifier.watch(path.clone())?;

    Ok(Box::new(move |mut warnings, cancel| {
        Box::pin(async move {
            let mut timer = time::interval(poll_period);

            loop {
                select! {
                    _ = cancel.cancelled().fuse() => break,
                    _ = timer.tick().fuse() => {}
                    _ = watcher.wait().fuse() => {}
                }

                if let Err(err) = read_once(&path, &mut send).await {
                    warnings.send(format!("{err:?}"));
                }
            }

            Ok(())
        })
    }))
}

async fn send_fields(time: SystemTime, json: &str, send: &mut mpsc::Sender<Message>) -> Result<()> {
    if json.is_empty() {
        return Ok(());
    }

    let KeyValues::<MaybeNumber>(fields) = match serde_json::from_str(json).context("parsing JSON")
    {
        Ok(obj) => obj,
        Err(err) => {
            log::error!("Encountered invalid JSON: {err}");
            log::debug!("Data ({}): {json}", json.len());
            return Err(err);
        }
    };

    for (label, field) in fields {
        if let MaybeNumber::Number(value) = field {
            let message = Message { label, value, time };
            send.feed(message).await?;
        } else {
            log::debug!("Key {label:?} is not a number");
        }
    }
    send.flush().await?;

    Ok(())
}

#[derive(Debug)]
struct KeyValues<T>(Vec<(String, T)>);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for KeyValues<T> {
    fn deserialize<D>(d: D) -> StdResult<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct MapVisitor<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> de::Visitor<'de> for MapVisitor<T> {
            type Value = Vec<(String, T)>;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "a JSON object")
            }

            fn visit_map<A>(self, mut map: A) -> StdResult<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                let mut vec = Vec::new();
                while let Some((key, value)) = map.next_entry::<String, T>()? {
                    vec.push((key, value));
                }
                Ok(vec)
            }
        }

        let vec = d.deserialize_map(MapVisitor(PhantomData))?;
        Ok(Self(vec))
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum MaybeNumber {
    Number(f64),
    NotNumber(de::IgnoredAny),
}
