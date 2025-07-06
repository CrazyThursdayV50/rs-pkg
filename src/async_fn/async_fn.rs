use std::{pin::Pin, sync::Arc};

use tokio::sync::Mutex;

pub(crate) type AsyncFn<Args, Resp> = dyn Fn(Args) -> Pin<Box<dyn Future<Output = Resp> + Send + Sync + 'static>>
    + Send
    + Sync
    + 'static;

pub(crate) fn wrap_fn<F, Fut, Args, Resp>(h: F) -> Arc<AsyncFn<Args, Resp>>
where
    F: Fn(Args) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Resp> + Send + Sync + 'static,
{
    Arc::new(move |args| Box::pin(h(args)))
}

pub(crate) type AsyncFnOnce<Args, Resp> = dyn FnOnce(Args) -> Pin<Box<dyn Future<Output = Resp> + Send + Sync + 'static>>
    + Send
    + Sync
    + 'static;

pub(crate) fn wrap_fn_once<F, Fut, Args, Resp>(h: F) -> Arc<AsyncFnOnce<Args, Resp>>
where
    F: FnOnce(Args) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Resp> + Send + Sync + 'static,
{
    Arc::new(move |args| Box::pin(h(args)))
}

pub(crate) type AsyncFnMut<Args, Resp> = dyn FnMut(Args) -> Pin<Box<dyn Future<Output = Resp> + Send + Sync + 'static>>
    + Send
    + Sync
    + 'static;

pub(crate) fn wrap_fn_mut<F, Fut, Args, Resp>(mut h: F) -> Arc<AsyncFnMut<Args, Resp>>
where
    F: FnMut(Args) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Resp> + Send + Sync + 'static,
{
    Arc::new(move |args| Box::pin(h(args)))
}
