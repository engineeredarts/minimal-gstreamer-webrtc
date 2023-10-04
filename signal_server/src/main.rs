use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tungstenite::protocol::Message;

type Sender = Arc<broadcast::Sender<String>>;

#[tokio::main]
async fn main() {
    println!("Signal Server");

    let (tx_1, _rx_1) = broadcast::channel::<String>(16);
    let (tx_2, _rx_2) = broadcast::channel::<String>(16);

    let tx_1 = Arc::new(tx_1);
    let tx_2 = Arc::new(tx_2);

    tokio::select! {
        _ = listen("backend", "127.0.0.1:10001", tx_1.clone(), tx_2.clone()) => {}
        _ = listen("frontend", "127.0.0.1:10002", tx_2.clone(), tx_1.clone()) => {}
    }
}

async fn listen(label: &'static str, address: &str, tx_forward: Sender, tx_send: Sender) {
    println!("[{label}] LISTEN {address}");

    let server = TcpListener::bind(address).await.unwrap();

    while let Ok((socket, _)) = server.accept().await {
        tokio::spawn(handle_connection(
            label,
            socket,
            tx_forward.clone(),
            tx_send.subscribe(),
        ));
    }
}

async fn handle_connection(
    label: &'static str,
    socket: TcpStream,
    tx_forward: Sender,
    mut rx_send: broadcast::Receiver<String>,
) {
    let address = socket.peer_addr().unwrap();
    println!("[{label}] CONNECTION from {address}");

    // websocket handshake
    let ws_stream = tokio_tungstenite::accept_async(socket).await.unwrap();

    let (mut write, mut read) = ws_stream.split();

    loop {
        tokio::select! {
            Some(m) = read.next() => {
                match m {
                    Ok(m) => match m {
                        Message::Text(text) => {
                            println!("[{label}] FORWARD {text}");
                            tx_forward.send(text).unwrap();
                        }
                        _ => {}
                    }
                    Err(e) => {
                        println!("[{label}] receive error {e:?}")
                    }
                }
            }
            Ok(text) = rx_send.recv() => {
                println!("[{label}] SEND {text}");
                write.send(Message::Text(text)).await.unwrap();
            }
        }
    }
}
