use std::{pin::Pin, sync::Arc};

pub(crate) type AsyncFn<Args, Resp> = dyn Fn(Args) -> Pin<Box<dyn Future<Output = Resp> + Send + Sync + 'static>>
    + Send
    + Sync
    + 'static;

pub(crate) fn wrap<F, Fut, Args, Resp>(h: F) -> Arc<AsyncFn<Args, Resp>>
where
    F: Fn(Args) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Resp> + Send + Sync + 'static,
{
    Arc::new(move |args| Box::pin(h(args)))
}
