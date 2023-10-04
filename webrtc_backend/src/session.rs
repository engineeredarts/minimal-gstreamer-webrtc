#![allow(dead_code)]
#![allow(unused_imports)]

use anyhow::{anyhow, bail, Result};
use std::ops::{Deref, Drop};
use std::sync::{Arc, Mutex as StdMutex, Weak};

use gst::prelude::*;
use gst_rtp::prelude::*;

use crate::signals::*;

// upgrade weak reference or return
#[macro_export]
macro_rules! upgrade_weak {
    ($x:ident, $r:expr) => {{
        match $x.upgrade() {
            Some(o) => o,
            None => return $r,
        }
    }};
    ($x:ident) => {
        upgrade_weak!($x, ())
    };
}

#[derive(Debug, Clone)]
pub struct Session(Arc<Inner>);

#[derive(Debug, Clone)]
pub struct SessionWeak(Weak<Inner>);

#[derive(Debug)]
pub struct Inner {
    session_id: u64,
    outgoing_signal_tx: Arc<SignalSender>,
    pipeline: gst::Pipeline,
    bus_watch: Arc<StdMutex<Option<gst::bus::BusWatchGuard>>>,
    webrtcbin: gst::Element,
}

impl Deref for Session {
    type Target = Inner;

    fn deref(&self) -> &Inner {
        &self.0
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        let session_id = self.session_id;
        println!("[WebRTC Session {session_id}] DROP");
        self.end();
    }
}

impl Inner {
    fn end(&self) {
        let session_id = self.session_id;
        println!("[WebRTC Session {session_id}] END");
    }
}

impl SessionWeak {
    // try weak reference -> strong
    pub fn upgrade(&self) -> Option<Session> {
        self.0.upgrade().map(Session)
    }
}

impl Session {
    pub fn session_id(&self) -> u64 {
        self.0.session_id
    }

    // strong reference -> weak
    fn downgrade(&self) -> SessionWeak {
        SessionWeak(Arc::downgrade(&self.0))
    }

    pub fn start(session_id: u64, outgoing_signal_tx: Arc<SignalSender>) -> Result<Self> {
        println!("[WebRTC Session {session_id}] START");

        let webrtcbin = gst::ElementFactory::make("webrtcbin")
            .property("latency", 30u32) // jitterbuffer size, ms (default 200)
            .build()
            .expect("failed to make webrtcbin");

        // ICE servers
        // webrtcbin.set_property_from_str("stun-server", &ice_servers.stun_server_uri);
        // if let Some(uri) = ice_servers.turn_server_uri_with_auth() {
        //     webrtcbin.set_property_from_str("turn-server", &uri);
        // }

        // allow as many media streams as the peer wants
        webrtcbin.set_property_from_str("bundle-policy", "max-compat");

        // create empty pipeline
        let pipeline = gst::Pipeline::builder()
            .name(format!("pipeline-{session_id}"))
            .build();

        // add the webrtcbin element
        pipeline.add(&webrtcbin).unwrap();

        // start playing
        pipeline
            .set_state(gst::State::Playing)
            .expect("failed to set the pipeline to the `Playing` state");

        let session = Session(Arc::new(Inner {
            session_id,
            outgoing_signal_tx,
            pipeline,
            bus_watch: Arc::new(StdMutex::new(None)),
            webrtcbin,
        }));

        // consume bus messages
        session.watch_bus()?;

        //////////////////////////////////////////////////////////////////////
        // hook up GStreamer signals

        // send our ICE candidates to peer
        let session_clone = session.downgrade();
        session
            .webrtcbin
            .connect("on-ice-candidate", false, move |values| {
                let _webrtc = values[0].get::<gst::Element>().expect("Invalid argument");
                let line_index = values[1].get::<u32>().expect("Invalid argument");
                let candidate = values[2].get::<String>().expect("Invalid argument");

                let session = upgrade_weak!(session_clone, None);

                if let Err(err) = session.on_local_ice_candidate(candidate, line_index) {
                    gst::element_error!(
                        session.pipeline,
                        gst::LibraryError::Failed,
                        ("failed to send ICE candidate: {:?}", err)
                    );
                }

                None
            });

        // watch for new data channels
        let session_clone = session.downgrade();
        session
            .webrtcbin
            .connect("on-data-channel", false, move |values| {
                let _webrtc = values[0].get::<gst::Element>().expect("Invalid argument");
                let data_channel = values[1]
                    .get::<gst_webrtc::WebRTCDataChannel>()
                    .expect("Invalid argument");

                let session = upgrade_weak!(session_clone, None);

                if let Err(err) = session.on_data_channel(data_channel) {
                    gst::element_error!(
                        session.pipeline,
                        gst::LibraryError::Failed,
                        ("failed to handle data channel: {:?}", err)
                    );
                }

                None
            });

        // connection state
        let session_clone = session.downgrade();
        session
            .webrtcbin
            .connect("notify::connection-state", false, move |values| {
                let _webrtc = values[0].get::<gst::Element>().expect("Invalid argument");
                let session = upgrade_weak!(session_clone, None);
                if let Err(err) = session.on_connection_state_changed() {
                    gst::element_error!(
                        session.pipeline,
                        gst::LibraryError::Failed,
                        ("failed to handle connection state change: {:?}", err)
                    );
                }

                None
            });

        // ICE connection state
        let session_clone = session.downgrade();
        session
            .webrtcbin
            .connect("notify::ice-connection-state", false, move |values| {
                let _webrtc = values[0].get::<gst::Element>().expect("Invalid argument");
                let session = upgrade_weak!(session_clone, None);
                if let Err(err) = session.on_ice_connection_state_changed() {
                    gst::element_error!(
                        session.pipeline,
                        gst::LibraryError::Failed,
                        ("failed to handle ICE connection state change: {:?}", err)
                    );
                }

                None
            });

        // signaling state
        let session_clone = session.downgrade();
        session
            .webrtcbin
            .connect("notify::signaling-state", false, move |values| {
                let _webrtc = values[0].get::<gst::Element>().expect("Invalid argument");
                let session = upgrade_weak!(session_clone, None);
                if let Err(err) = session.on_signaling_state_changed() {
                    gst::element_error!(
                        session.pipeline,
                        gst::LibraryError::Failed,
                        ("failed to handle signaling state change: {:?}", err)
                    );
                }

                None
            });

        //////////////////////////////////////////////////////////////////////

        // session.write_debug_dot_file("START");

        Ok(session)
    }

    /// Stop the session.
    pub fn stop(&self) -> Result<()> {
        let session_id = self.session_id();
        println!("[WebRTC Session {session_id}] STOP");

        // clean up
        self.0.end();

        Ok(())
    }

    fn watch_bus(&self) -> Result<()> {
        let session_id = self.session_id();

        let bus = self.pipeline.bus().unwrap();
        let session_clone = self.downgrade();
        let bus_watch = bus.add_watch(move |_bus, message| {
            // println!("[WebRTC Session {session_id}] BUS MESSAGE {:?}", message);

            use gst::MessageView;

            let view = message.view();
            match view {
                MessageView::Error(_) => {
                    let session = upgrade_weak!(session_clone, glib::ControlFlow::Break);
                    session.on_bus_error(view);

                    glib::ControlFlow::Continue
                }
                MessageView::Eos(..) => {
                    println!("[WebRTC Session {session_id}] end of bus stream");
                    glib::ControlFlow::Break
                }
                _ => glib::ControlFlow::Continue,
            }
        })?;

        let mut bw = self.bus_watch.lock().expect("failed to lock bus watch");
        *bw = Some(bus_watch);

        Ok(())
    }

    fn on_bus_error(&self, message: gst::MessageView) {
        if let gst::MessageView::Error(err) = message {
            let session_id = self.session_id();
            let src = match err.src() {
                Some(element) => element.path_string().into(),
                None => "(unknown)".to_string(),
            };
            let debug = match err.debug() {
                Some(s) => s.into(),
                None => "no debug info".to_string(),
            };
            println!(
                "[WebRTC Session {session_id}] BUS ERROR from {src} - {}\n  {debug}",
                err.error()
            );

            // stop session on any error
            // let _ = self.stop();
        }
    }

    fn on_local_answer(
        &self,
        reply: Result<Option<&gst::StructureRef>, gst::PromiseError>,
    ) -> Result<(), anyhow::Error> {
        let session_id = self.session_id();

        let reply = match reply {
            Ok(Some(reply)) => reply,
            Ok(None) => {
                bail!("local answer is empty");
            }
            Err(err) => {
                bail!("local answer error: {:?}", err);
            }
        };

        match reply.value("answer") {
            Ok(answer) => {
                let answer = answer
                    .get::<gst_webrtc::WebRTCSessionDescription>()
                    .expect("Invalid argument");

                self.webrtcbin
                    .emit_by_name::<()>("set-local-description", &[&answer, &None::<gst::Promise>]);

                let sdp = answer.sdp().as_text().expect("answer has no SDP");

                // send answer
                self.outgoing_signal_tx.send(Signal::WebRtcAnswer {
                    data: WebRtcAnswerData {
                        session_id,
                        webrtc_data: WebRtcData {
                            data_type: Some("answer".to_string()),
                            sdp,
                        },
                    },
                })?;
            }
            Err(e) => {
                // sometimes there is no answer in the answer :\
                println!("[WebRTC Session {session_id}] answer error: {e:?} reply: {reply:?}");
                bail!("no answer in local answer: {:?}", e);
            }
        }

        Ok(())
    }

    fn on_local_ice_candidate(&self, candidate: String, line_index: u32) -> Result<()> {
        let session_id = self.session_id();
        self.outgoing_signal_tx.send(Signal::IceCandidate {
            data: IceCandidateData {
                session_id,
                webrtc_data: IceCandidateWebRtcData {
                    candidate,
                    media_id: None,
                    line_index: Some(line_index),
                    username_fragment: None,
                },
            },
        })?;
        Ok(())
    }

    pub fn on_remote_offer(&mut self, offer: WebRtcData) -> Result<()> {
        let ret = gst_sdp::SDPMessage::parse_buffer(offer.sdp.as_bytes())
            .map_err(|_| anyhow!("failed to parse SDP offer"))?;

        let session_clone = self.downgrade();
        self.pipeline.call_async(move |_pipeline| {
            let session = upgrade_weak!(session_clone);

            let offer =
                gst_webrtc::WebRTCSessionDescription::new(gst_webrtc::WebRTCSDPType::Offer, ret);

            session
                .0
                .webrtcbin
                .emit_by_name::<()>("set-remote-description", &[&offer, &None::<gst::Promise>]);

            let session_clone = session.downgrade();
            let promise = gst::Promise::with_change_func(move |reply| {
                let session = upgrade_weak!(session_clone);

                if let Err(err) = session.on_local_answer(reply) {
                    gst::element_error!(
                        session.pipeline,
                        gst::LibraryError::Failed,
                        ("failed to send answer: {:?}", err)
                    );
                }
            });

            session
                .0
                .webrtcbin
                .emit_by_name::<()>("create-answer", &[&None::<gst::Structure>, &promise]);
        });

        Ok(())
    }

    pub fn on_remote_answer(&mut self, answer: WebRtcData) -> Result<()> {
        let ret = gst_sdp::SDPMessage::parse_buffer(answer.sdp.as_bytes())
            .map_err(|_| anyhow!("failed to parse SDP answer"))?;
        let answer =
            gst_webrtc::WebRTCSessionDescription::new(gst_webrtc::WebRTCSDPType::Answer, ret);

        self.webrtcbin
            .emit_by_name::<()>("set-remote-description", &[&answer, &None::<gst::Promise>]);

        Ok(())
    }

    pub fn on_remote_ice_candidate(&mut self, ice_candidate_data: IceCandidateWebRtcData) {
        let IceCandidateWebRtcData {
            candidate,
            line_index,
            ..
        } = ice_candidate_data;

        if let Some(line_index) = line_index {
            self.webrtcbin
                .emit_by_name::<()>("add-ice-candidate", &[&line_index, &candidate]);
        }
    }

    //////////////////////////////////////////////////////////////////////////

    fn on_connection_state_changed(&self) -> Result<()> {
        let connection_state = self
            .webrtcbin
            .property::<gst_webrtc::WebRTCPeerConnectionState>("connection-state");

        let session_id = self.session_id();
        println!("[WebRTC Session {session_id}] CONNECTION STATE → {connection_state:?}");

        if connection_state == gst_webrtc::WebRTCPeerConnectionState::Failed {
            println!("[WebRTC Session {session_id}] CONNECTION FAILED, ending session");
            self.stop()?;
        }

        Ok(())
    }

    fn on_ice_connection_state_changed(&self) -> Result<()> {
        let ice_connection_state = self
            .webrtcbin
            .property::<gst_webrtc::WebRTCICEConnectionState>("ice-connection-state");

        let session_id = self.session_id();
        println!("[WebRTC Session {session_id}] ICE CONNECTION STATE → {ice_connection_state:?}");

        Ok(())
    }

    fn on_signaling_state_changed(&self) -> Result<()> {
        let signaling_state = self
            .webrtcbin
            .property::<gst_webrtc::WebRTCSignalingState>("signaling-state");

        let session_id = self.session_id;
        println!("[WebRTC Session {session_id}] SIGNALING STATE → {signaling_state:?}");

        Ok(())
    }

    //////////////////////////////////////////////////////////////////////////

    fn on_data_channel(&self, data_channel: gst_webrtc::WebRTCDataChannel) -> Result<()> {
        let label = data_channel.property::<String>("label");
        println!("[WebRTC Session] DATA CHANNEL {label}");

        // // open
        // let session_clone = self.downgrade();
        // data_channel.connect("on-open", false, move |values| {
        //     let session = upgrade_weak!(session_clone, None);

        //     let data_channel = values[0]
        //         .get::<gst_webrtc::WebRTCDataChannel>()
        //         .expect("Invalid argument");
        //     if let Err(err) = session.on_data_channel_open(data_channel) {
        //         gst::element_error!(
        //             session.pipeline,
        //             gst::LibraryError::Failed,
        //             ("failed to handle data channel open: {:?}", err)
        //         );
        //     }
        //     None
        // });

        // // close
        // let session_clone = self.downgrade();
        // data_channel.connect("on-close", false, move |values| {
        //     let session = upgrade_weak!(session_clone, None);

        //     let data_channel = values[0]
        //         .get::<gst_webrtc::WebRTCDataChannel>()
        //         .expect("Invalid argument");
        //     if let Err(err) = session.on_data_channel_close(data_channel) {
        //         gst::element_error!(
        //             session.pipeline,
        //             gst::LibraryError::Failed,
        //             ("failed to handle data channel close: {:?}", err)
        //         );
        //     }
        //     None
        // });

        // // text message
        // let session_clone = self.downgrade();
        // data_channel.connect("on-message-string", false, move |values| {
        //     let session = upgrade_weak!(session_clone, None);

        //     let data_channel = values[0]
        //         .get::<gst_webrtc::WebRTCDataChannel>()
        //         .expect("Invalid argument");
        //     let message = values[1].get::<String>().expect("Invalid argument");
        //     if let Err(err) = session.on_data_channel_message_string(data_channel, message) {
        //         gst::element_error!(
        //             session.pipeline,
        //             gst::LibraryError::Failed,
        //             ("failed to handle data channel text message: {:?}", err)
        //         );
        //     }
        //     None
        // });

        // // binary (gz compressed) message
        // let session_clone = self.downgrade();
        // data_channel.connect("on-message-data", false, move |values| {
        //     let session = upgrade_weak!(session_clone, None);

        //     let data_channel = values[0]
        //         .get::<gst_webrtc::WebRTCDataChannel>()
        //         .expect("Invalid argument");
        //     let message = values[1].get::<glib::Bytes>().expect("Invalid argument");
        //     // let mut z = ZlibDecoder::new(&message[..]);
        //     // let mut s = String::new();
        //     // z.read_to_string(&mut s)?;
        //     if let Err(err) = session.on_data_channel_message_data(data_channel, message) {
        //         gst::element_error!(
        //             session.pipeline,
        //             gst::LibraryError::Failed,
        //             ("failed to handle data channel data message: {:?}", err)
        //         );
        //     }
        //     None
        // });

        // // error
        // let session_clone = self.downgrade();
        // data_channel.connect("on-error", false, move |values| {
        //     let session = upgrade_weak!(session_clone, None);

        //     let data_channel = values[0]
        //         .get::<gst_webrtc::WebRTCDataChannel>()
        //         .expect("Invalid argument");

        //     let error = values[1].get::<glib::Error>().expect("Invalid argument");

        //     if let Err(err) = session.on_data_channel_error(data_channel, error) {
        //         gst::element_error!(
        //             session.pipeline,
        //             gst::LibraryError::Failed,
        //             ("failed to handle data channel error: {:?}", err)
        //         );
        //     }
        //     None
        // });

        Ok(())
    }

    // fn on_data_channel_open(&self, data_channel: gst_webrtc::WebRTCDataChannel) -> Result<()> {
    //     if self.is_running() {
    //         let session_id = self.session_id();
    //         let label = data_channel.property::<String>("label");
    //         debug!("[WebRTC Session {session_id}] DATA CHANNEL {label} OPEN");

    //         let data_channel = DataChannel::new(&label, data_channel);
    //         let mut m = self
    //             .data_channels_by_label
    //             .write()
    //             .expect("failed to lock data channel map for write");

    //         let data_channel = Arc::new(Mutex::new(data_channel));

    //         m.insert(label.clone(), data_channel.clone());

    //         // notify listener
    //         self.session_event_tx
    //             .send(SessionEvent::DataChannelOpen {
    //                 session: self.downgrade(),
    //                 label,
    //             })
    //             .expect("failed to send data channel open event");

    //         let dc: tokio::sync::MutexGuard<'_, DataChannel> = data_channel.blocking_lock();
    //         dc.start_send_thread(data_channel.clone());
    //     }

    //     Ok(())
    // }

    // fn on_data_channel_close(&self, data_channel: gst_webrtc::WebRTCDataChannel) -> Result<()> {
    //     if self.is_running() {
    //         let session_id = self.session_id();
    //         let label = data_channel.property::<String>("label");
    //         info!("[WebRTC Session {session_id}] DATA CHANNEL {label} CLOSE");

    //         let mut m = self
    //             .data_channels_by_label
    //             .write()
    //             .expect("failed to lock data channel map for write");
    //         m.remove(&label);

    //         // notify listener
    //         self.session_event_tx
    //             .send(SessionEvent::DataChannelClosed {
    //                 session: self.downgrade(),
    //                 label,
    //             })
    //             .expect("failed to send data channel closed event");
    //     }
    //     Ok(())
    // }

    // fn on_data_channel_message_string(
    //     &self,
    //     data_channel: gst_webrtc::WebRTCDataChannel,
    //     message: String,
    // ) -> Result<()> {
    //     if self.is_running() {
    //         let session_id = self.session_id();
    //         let label = data_channel.property::<String>("label");

    //         debug!("[WebRTC Session {session_id}] DATA CHANNEL {label} RECEIVE {message}");

    //         let m = self
    //             .data_channels_by_label
    //             .read()
    //             .expect("failed to lock data channel map for read");

    //         match m.get(&label) {
    //             Some(dc) => {
    //                 self.data_channel_tx.send(DataChannelMessage {
    //                     session: self.downgrade(),
    //                     channel: Arc::downgrade(dc),
    //                     string: Some(message),
    //                 })?;
    //             }
    //             None => {
    //                 warn!(
    //                 "[WebRTC Session {session_id}] message received for unrecognised data channel {label}, dropped"
    //             );
    //             }
    //         }
    //     }
    //     Ok(())
    // }

    // fn on_data_channel_message_data(
    //     &self,
    //     data_channel: gst_webrtc::WebRTCDataChannel,
    //     message: glib::Bytes,
    // ) -> Result<()> {
    //     if self.is_running() {
    //         let session_id = self.session_id();
    //         let label = data_channel.property::<String>("label");
    //         // info!("[{label}] RECEIVE {message}");

    //         let m = self
    //             .data_channels_by_label
    //             .read()
    //             .expect("failed to lock data channel map for read");

    //         match m.get(&label) {
    //             Some(dc) => {
    //                 let mut gz = GzDecoder::new(&message[..]);
    //                 let mut s = String::new();
    //                 gz.read_to_string(&mut s)?;

    //                 debug!("[{label}] RECEIVE (gzipped) {s}");

    //                 self.data_channel_tx.send(DataChannelMessage {
    //                     session: self.downgrade(),
    //                     channel: Arc::downgrade(dc),
    //                     string: Some(s),
    //                 })?;
    //             }
    //             None => {
    //                 warn!(
    //                 "[WebRTC Session {session_id}] message received for unrecognised data channel {label}, dropped"
    //             );
    //             }
    //         }
    //     }
    //     Ok(())
    // }

    // fn on_data_channel_error(
    //     &self,
    //     data_channel: gst_webrtc::WebRTCDataChannel,
    //     error: glib::Error,
    // ) -> Result<()> {
    //     if self.is_running() {
    //         let session_id = self.session_id();
    //         let label = data_channel.property::<String>("label");
    //         error!("[WebRTC Session {session_id}] DATA CHANNEL {label} ERROR - {error}");

    //         // notify listener
    //         let error_message = format!("{error}");
    //         self.session_event_tx
    //             .send(SessionEvent::DataChannelError {
    //                 session: self.downgrade(),
    //                 label,
    //                 error_message,
    //             })
    //             .expect("failed to send data channel error event");
    //     }
    //     Ok(())
    // }

    //////////////////////////////////////////////////////////////////////////
}
