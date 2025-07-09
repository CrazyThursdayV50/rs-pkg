use crate::worker::Worker;
use futures_util::{SinkExt, StreamExt, stream::SplitSink};
use std::sync::Arc;
use tokio::{net::TcpStream, sync::Mutex};
use tokio_tungstenite::{
    WebSocketStream, accept_async,
    tungstenite::{Error, Message, Result},
};

pub async fn convert_message_default(msg: Message) -> Message {
    msg
}

pub async fn create_stream_handler<F, Fut, H, Hut>(
    stream: TcpStream,
    how: F,
) -> impl Fn(TcpStream) -> Hut + Send + Sync + 'static
where
    F: Fn(Message) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Message> + Send + Sync + 'static,
    H: Fn(TcpStream) -> Hut + Send + Sync + 'static,
    Hut: Future<Output = ()> + Send + Sync + 'static,
{
    let how = Arc::new(how);
    let x = move |stream: TcpStream| {
        async move {}
        // let how = how.clone();
        // let peer = Arc::new(
        //     stream
        //         .peer_addr()
        //         .expect("connected streams should have a peer address"),
        // );

        // async move {
        //     let ws_stream = accept_async(stream).await.expect("Failed to accept");
        //     let (sender, mut recv) = ws_stream.split();
        //     let message_worker = Worker::new(format!("Stream({})", peer.to_string()).as_str(), 100);
        //     let sender = Arc::new(Mutex::new(sender));

        //     let how = move |j: Message| {
        //         let sender = sender.clone();
        //         async move {
        //             let x = how(j).await;
        //             let mut guard = sender.lock().await;
        //             _ = guard.send(x).await;
        //         }
        //     };
        //     message_worker.run(how);
        // }
    };
    x
}

pub async fn handle_error<F, Fut, Hut>(fut: Hut, e: F)
where
    Hut: Future<Output = Result<(), Error>> + Send + Sync + 'static,
    F: Fn(Error) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + Sync + 'static,
{
    if let Err(err) = fut.await {
        e(err).await
    }
}

pub async fn send_message(
    message: Message,
    sender: Arc<Mutex<SplitSink<WebSocketStream<TcpStream>, Message>>>,
) -> impl Future<Output = Result<(), Error>> + Send + Sync + 'static {
    async move {
        let mut guard = sender.lock().await;
        guard.send(message).await
    }
}
