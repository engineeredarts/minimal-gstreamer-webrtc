use tokio::sync::mpsc;

mod signals;

use signals::Signal;

#[tokio::main]
async fn main() {
    println!("Minimal GStreamer WebRTC - Backend");

    let (incoming_signals_tx, mut incoming_signals_rx) = mpsc::unbounded_channel::<Signal>();
    let (_outgoing_signals_tx, outgoing_signals_rx) = mpsc::unbounded_channel::<Signal>();

    signals::connect(
        "ws://127.0.0.1:10001",
        incoming_signals_tx,
        outgoing_signals_rx,
    )
    .await;

    loop {
        tokio::select! {
            signal = incoming_signals_rx.recv() => {
                println!("SIGNAL {signal:?}");

            }
        }
    }
}
