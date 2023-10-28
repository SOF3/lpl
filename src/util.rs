use anyhow::Result;
use futures::future::pending;
use futures::{Stream, StreamExt as _};

pub struct Finally<T, F: FnOnce(T) -> Result<()>>(pub Option<(T, F)>);
impl<T, F: FnOnce(T) -> Result<()>> Drop for Finally<T, F> {
    fn drop(&mut self) {
        let (data, closure) = self.0.take().unwrap();
        if let Err(err) = closure(data) {
            eprintln!("Error: {err}");
        }
    }
}

pub async fn some_or_pending<T>(option: &mut Option<impl Stream<Item = T> + Unpin>) -> T {
    match option {
        Some(stream) => {
            let item = stream.next().await;
            match item {
                Some(item) => item,
                None => {
                    *option = None;
                    pending().await
                }
            }
        }
        None => pending().await,
    }
}
