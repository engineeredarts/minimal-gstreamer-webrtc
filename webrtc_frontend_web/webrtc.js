var signals_ws;
var peer = null;
var connected = false;
var data_channel = null;

function on_load() {
  document.getElementById("connect_button").onclick = async () => {
    await connect_webrtc();
  };

  document.getElementById("disconnect_button").onclick = () => {
    disconnect_webrtc();
  };

  init_webrtc();
}

function init_webrtc() {
  console.info("initializing WebRTC...");

  signals_ws = new WebSocket("ws://localhost:10002");

  signals_ws.addEventListener("open", (event) => {
    console.info("[Signals] OPEN");
  });

  signals_ws.addEventListener("message", (event) => {
    const msg = JSON.parse(event.data);
    const { type, data } = msg;
    on_signal(type, data);
  });
}

async function connect_webrtc() {
  console.info("connecting...");
  connected = false;

  peer = new RTCPeerConnection();

  ////////////////////////////////////////////////////////////////////////////

  peer.onnegotiationneeded = (event) => {
    make_offer();
  };

  peer.onicecandidate = (event) => {
    console.info("local ICE candidate", event);
    const candidate = event.candidate;

    if (candidate && candidate.candidate) {
      // wtf?
      send_signal("ice_candidate", {
        session_id: 0,
        webrtc_data: {
          candidate: candidate.candidate,
          sdpMLineIndex: candidate.sdpMLineIndex,
        },
      });
    }
  };

  peer.oniceconnectionstatechange = (event) => {
    console.info("ICE connection state:", event.target.iceConnectionState);
  };

  peer.onconnectionstatechange = (event) => {
    console.info("connection state:", event.target.connectionState);
    check_connected();
  };

  peer.onsignalingstatechange = (event) => {
    console.info("signaling state:", event.target.signalingState);
    check_connected();
  };

  peer.ondatachannel = (event) => {
    const { channel } = event;
    console.info("[Data Channel] data channel", channel);

    channel.onopen = (event) => {
      console.info("[Data Channel] OPEN");
    };

    channel.onclose = (event) => {
      console.info("[Data Channel] CLOSE");
    };

    channel.onmessage = (event) => {
      console.info("[Data Channel] MESSAGE", event);
    };
  };

  ////////////////////////////////////////////////////////////////////////////

  await make_offer();

  open_data_channel();
}

async function make_offer() {
  const offer = await peer.createOffer();
  peer.setLocalDescription(offer);

  send_signal("webrtc_offer", {
    session_id: 0,
    webrtc_data: { type: "offer", sdp: offer.sdp },
  });
}

function send_signal(type, data) {
  const msg = JSON.stringify({
    type,
    data,
  });

  console.info("[Signals] SEND", msg);
  signals_ws.send(msg);
}

function on_signal(type, data) {
  console.info("[Signals] RECEIVED", type, data);

  const { webrtc_data } = data;

  switch (type) {
    case "webrtc_answer":
      on_answer(webrtc_data);
      break;

    case "ice_candidate":
      on_ice_candidate(webrtc_data);
      break;
  }
}

function on_answer(data) {
  console.info("[Signals] REMOTE ANSWER", data);
  peer.setRemoteDescription(new RTCSessionDescription(data));
}

function on_ice_candidate(data) {
  console.info("[Signals] REMOTE ICE CANDIDATE", data);
  peer.addIceCandidate(new RTCIceCandidate(data));
}

function check_connected() {
  if (!peer) return;

  console.info("connection state:", peer.connectionState);
  if (!connected) {
    if (peer.connectionState == "connected") {
      console.info("CONNECTED");
      connected = true;
    }
  } else {
    if (peer.connectionState != "connected") {
      console.info("NOT CONNECTED");
      connected = false;
    }
  }
}

function open_data_channel() {
  console.info("opening data channel...");
  data_channel = peer.createDataChannel("test data channel", { ordered: true });
}

function disconnect_webrtc() {
  console.info("disconnecting...");
  if (data_channel) {
    data_channel.close();
    data_channel = null;
  }
  if (peer) {
    peer.close();
    peer = null;
  }
  connected = false;
}
