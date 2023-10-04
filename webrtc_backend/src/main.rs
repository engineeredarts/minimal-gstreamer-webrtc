use anyhow::Result;
use tokio::sync::mpsc;

mod session;
mod signals;
mod webrtc;

use signals::Signal;
use webrtc::WebRtc;

#[tokio::main]
async fn main() -> Result<()> {
    println!("Minimal GStreamer WebRTC - Backend");

    let (incoming_signals_tx, mut incoming_signals_rx) = mpsc::unbounded_channel::<Signal>();
    let (outgoing_signals_tx, outgoing_signals_rx) = mpsc::unbounded_channel::<Signal>();

    signals::connect(
        "ws://127.0.0.1:10001",
        incoming_signals_tx,
        outgoing_signals_rx,
    )
    .await;

    let webrtc = WebRtc::new(outgoing_signals_tx).await?;

    loop {
        tokio::select! {
            Some(signal) = incoming_signals_rx.recv() => {
                println!("SIGNAL {signal:?}");
                webrtc.on_incoming_signal(signal).await;
            }
        }
    }

    #[allow(unreachable_code)]
    Ok(())
}
