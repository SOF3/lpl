use std::fmt;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::result::Result as StdResult;

use anyhow::{Context as _, Result};
use futures::channel::mpsc;
use futures::SinkExt as _;
use serde::{de, Deserialize};
use tokio::fs;

use super::{Message, WorkerBuilder};

pub async fn open(path: PathBuf, send: &mpsc::Sender<Message>) -> Result<WorkerBuilder> {
    let fd = fs::File::open(&path).await.context("cannot open file for reading")?;
    let mut send = send.clone();

    Ok(Box::new(move |mut warnings, cancel| {
        Box::pin(async move {
            let mut read = super::thread_line_reader(fd, cancel, warnings.clone()).await;

            while let Some((line, time)) = read.recv().await {
                let fields = match serde_json::from_str(&line) {
                    Ok(KeyValues::<MaybeNumber>(fields)) => fields,
                    Err(err) => {
                        warnings.send(format!("Error parsing JSON line: {err:?}\nData: {line}",));
                        continue;
                    }
                };
                for (label, field) in fields {
                    if let MaybeNumber::Number(value) = field {
                        let message = Message { label, value, time };
                        send.feed(message).await?;
                    }
                }
                send.flush().await?;
            }

            Ok(())
        })
    }))
}

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

#[derive(Deserialize)]
#[serde(untagged)]
enum MaybeNumber {
    Number(f64),
    NotNumber(de::IgnoredAny),
}
