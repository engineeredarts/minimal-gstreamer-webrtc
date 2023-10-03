use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tungstenite::protocol::Message;

#[tokio::main]
async fn main() {
    println!("Signal Server");

    let (tx_1, rx_1) = broadcast::channel::<String>(16);
    let tx_1 = Arc::new(tx_1);
    listen("127.0.0.1:10001", tx_1.clone()).await;
}

async fn listen(address: &str, tx: Arc<broadcast::Sender<String>>) {
    println!("LISTEN {address}");

    let server = TcpListener::bind(address).await.unwrap();

    while let Ok((socket, _)) = server.accept().await {
        tokio::spawn(handle_connection(socket, tx.subscribe()));
    }
}

async fn handle_connection(socket: TcpStream, mut rx: broadcast::Receiver<String>) {
    let address = socket.peer_addr().unwrap();
    println!("CONNECTION from {address}");

    // websocket handshake
    let ws_stream = tokio_tungstenite::accept_async(socket).await.unwrap();

    let (mut write, mut read) = ws_stream.split();

    loop {
        tokio::select! {
            Some(m) = read.next() => {
                match m {
                    Ok(m) => match m {
                        Message::Text(text) => {
                            println!("RECEIVED {text}")
                        }
                        _ => {}
                    }
                    Err(e) => {
                        println!("receive error {e:?}")
                    }
                }
            }
            Ok(text) = rx.recv() => {
                println!("SEND {text}");
                write.send(Message::Text(text)).await.unwrap();
            }
        }
    }
}
