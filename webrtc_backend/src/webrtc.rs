use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::session::Session;
use crate::signals::*;

pub struct WebRtc {
    session: Arc<Mutex<Session>>,
    // outgoing_signal_tx: Arc<SignalSender>,
}

impl WebRtc {
    pub async fn new(outgoing_signal_tx: SignalSender) -> Result<Self> {
        let main_loop = glib::MainLoop::new(None, false);
        std::thread::spawn(move || {
            main_loop.run();
        });

        gst::init()?;

        let outgoing_signal_tx = Arc::new(outgoing_signal_tx);

        let session = Session::start(0, outgoing_signal_tx.clone())?;
        let session = Arc::new(Mutex::new(session));

        Ok(Self {
            session,
            // outgoing_signal_tx,
        })
    }

    pub async fn on_incoming_signal(&self, signal: Signal) {
        println!("[WebRtc] INCOMING SIGNAL {signal:?}");

        let mut session = self.session.lock().await;

        match signal {
            Signal::WebRtcOffer { data } => {
                let WebRtcOfferData { webrtc_data, .. } = data;
                session.on_remote_offer(webrtc_data).unwrap();
            }
            _ => {}
        }
    }
}
