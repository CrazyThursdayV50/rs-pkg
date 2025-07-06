mod client;
mod server;

use crate::async_fn::AsyncFn;
use std::error::Error;
type MessageHandlerType<M> = AsyncFn<M, Option<M>>;
type ErrorHandlerType = AsyncFn<Box<dyn Error + Send + Sync + 'static>, ()>;
type BytesGenerator = AsyncFn<(), Bytes>;

use bytes::Bytes;
pub use client::*;
pub use server::*;
