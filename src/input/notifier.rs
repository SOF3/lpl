use std::collections::{hash_map, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{self, AtomicU64};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::{Context as _, Result};
use futures::channel::mpsc;
use futures::future::FusedFuture;
use futures::{select, Future, FutureExt as _};
use notify::Watcher;
use parking_lot::{Mutex, RwLock};
use tokio::{fs, time};

use super::{Message, WarningSender, WorkerBuilder};
use crate::util;

pub fn start(
    warnings: WarningSender,
) -> Result<Notifier<impl notify::Watcher + Send + Sync + 'static>> {
    let senders = Arc::new(RwLock::new(AllSenders::default()));

    let watcher = notify::recommended_watcher(Handler { warnings, senders: senders.clone() })
        .context("create inotify watcher")?;

    Ok(Notifier { watcher_id: <_>::default(), senders, watcher: Arc::new(Mutex::new(watcher)) })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct WatcherId(u64);

pub struct Notifier<W> {
    watcher_id: Arc<AtomicU64>,
    senders:    Arc<RwLock<AllSenders>>,
    watcher:    Arc<Mutex<W>>,
}

#[derive(Default)]
struct AllSenders {
    paths: HashMap<PathBuf, PathSenders>,
}

#[derive(Default)]
struct PathSenders {
    watchers: HashMap<WatcherId, WatchHandleSender>,
}

impl<W: notify::Watcher + Send + Sync + 'static> Notifier<W> {
    pub fn watch(&self, path: &Path) -> Result<WatchHandle<W>> {
        let id = WatcherId(self.watcher_id.fetch_add(1, atomic::Ordering::SeqCst));
        let (event_send, event_recv) = mpsc::channel(16);

        let handle = WatchHandle {
            id,
            path: path.to_owned(),
            senders: self.senders.clone(),
            watcher: self.watcher.clone(),
            events: Some(event_recv),
        };

        let mut all_senders = self.senders.write();
        let path_entry = all_senders.paths.entry(path.to_owned());
        let path_senders = match path_entry {
            hash_map::Entry::Occupied(entry) => entry.into_mut(),
            hash_map::Entry::Vacant(entry) => {
                let mut watcher = self.watcher.lock();
                watcher
                    .watch(path, notify::RecursiveMode::NonRecursive)
                    .with_context(|| format!("register watcher for {}", path.display()))?;
                entry.insert(<_>::default())
            }
        };
        path_senders.watchers.insert(id, WatchHandleSender { event_ch: event_send });

        Ok(handle)
    }
}

struct WatchHandleSender {
    event_ch: mpsc::Sender<()>,
}

pub struct WatchHandle<W: notify::Watcher> {
    id:      WatcherId,
    path:    PathBuf,
    senders: Arc<RwLock<AllSenders>>,
    watcher: Arc<Mutex<W>>,
    events:  Option<mpsc::Receiver<()>>,
}

impl<W: notify::Watcher> WatchHandle<W> {
    pub fn wait(&mut self) -> impl FusedFuture<Output = ()> + '_ {
        util::some_or_pending(&mut self.events).fuse()
    }
}

impl<W: notify::Watcher> Drop for WatchHandle<W> {
    fn drop(&mut self) {
        let mut all_senders = self.senders.write();
        let hash_map::Entry::Occupied(mut path_senders) =
            all_senders.paths.entry(self.path.clone())
        else {
            panic!("some watcher not dropped yet but PathSenders is removed")
        };
        path_senders.get_mut().watchers.remove(&self.id);
        if path_senders.get_mut().watchers.is_empty() {
            path_senders.remove();
            {
                let mut watcher = self.watcher.lock();
                if let Err(err) = watcher.unwatch(&self.path) {
                    eprintln!("Error closing watcher: {err:?}");
                }
            }
        }
    }
}

struct Handler {
    warnings: WarningSender,
    senders:  Arc<RwLock<AllSenders>>,
}

impl notify::EventHandler for Handler {
    fn handle_event(&mut self, event: notify::Result<notify::Event>) {
        match event {
            Ok(event) => {
                if matches!(event.kind, notify::EventKind::Modify(..)) {
                    let all_senders = self.senders.read();
                    for path in event.paths {
                        if let Some(path_senders) = all_senders.paths.get(&path) {
                            for sender in path_senders.watchers.values() {
                                _ = sender.event_ch.clone().try_send(());
                            }
                        }
                    }
                }
            }
            Err(err) => self.warnings.send(format!("{err:?}")),
        }
    }
}

pub trait FieldParser: Send + Sync {
    fn parse(
        &self,
        time: SystemTime,
        content: &str,
        send: &mut mpsc::Sender<Message>,
    ) -> impl Future<Output = Result<()>> + Send;
}

pub fn open_poll(
    path: PathBuf,
    poll_period: Duration,
    notifier: &Notifier<impl Watcher + Send + Sync + 'static>,
    send: &mpsc::Sender<Message>,
    parser: impl FieldParser + 'static,
) -> Result<WorkerBuilder> {
    async fn read_once(
        path: &Path,
        send: &mut mpsc::Sender<Message>,
        parser: &impl FieldParser,
    ) -> Result<()> {
        let contents = fs::read_to_string(path).await.context("reading file contents")?;
        let time = SystemTime::now();
        parser.parse(time, &contents, send).await.context("send fields")
    }

    let mut send = send.clone();
    let mut watcher = notifier.watch(&path)?;

    Ok(Box::new(move |mut warnings, cancel| {
        Box::pin(async move {
            let mut timer = time::interval(poll_period);

            loop {
                select! {
                    () = cancel.cancelled().fuse() => break,
                    _ = timer.tick().fuse() => {},
                    () = watcher.wait().fuse() => {},
                }

                if let Err(err) = read_once(&path, &mut send, &parser).await {
                    warnings.send(format!("{err:?}"));
                }
            }

            Ok(())
        })
    }))
}
