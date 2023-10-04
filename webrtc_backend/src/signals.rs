#![allow(dead_code)]
#![allow(unused_imports)]

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tungstenite::Message;
use url::Url;

pub type SignalSender = mpsc::UnboundedSender<Signal>;
pub type SignalReceiver = mpsc::UnboundedReceiver<Signal>;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Signal {
    #[serde(rename = "webrtc_offer")]
    WebRtcOffer { data: WebRtcOfferData },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WebRtcOfferData {
    sdp: String,
}

pub async fn connect(
    url: &str,
    incoming_signals_tx: SignalSender,
    mut outgoing_signals_rx: SignalReceiver,
) {
    println!("[Signals] connecting to signal server {url}");

    let url = Url::parse(url).unwrap();
    let (socket, _response) = tokio_tungstenite::connect_async(url).await.unwrap();

    println!("[Signals] CONNECTED");

    let (mut ws_write, mut ws_read) = socket.split();

    let _task = tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(msg) = ws_read.next() => {
                    match msg.unwrap() {
                        Message::Text(text) => {
                            println!("[Signals] RECEIVE {text}");
                            let signal: Signal = serde_json::from_str(&text).unwrap();
                            incoming_signals_tx.send(signal).unwrap();
                        }
                        _ => {}
                    }

                }
                Some(signal) = outgoing_signals_rx.recv() => {
                    let text = serde_json::to_string(&signal).unwrap();
                    println!("[Signals] SEND {text}");
                    ws_write.send(Message::Text(text)).await.unwrap();
                }
            }
        }
    });
}
