use futures::Future;
use std::pin::Pin;

pub type WorkerFuture = Pin<Box<dyn Future<Output = Result<(), ()>> + Send>>;

pub trait Worker {
    fn run(&self) -> WorkerFuture;
}
