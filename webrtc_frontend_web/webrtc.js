var signals_ws;
var peer;

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

  document.getElementById("connect_button").onclick = async () => {
    await connect_webrtc();
  };
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
}

async function connect_webrtc() {
  console.info("connecting...");

  peer = new RTCPeerConnection();

  const offer = await peer.createOffer();
  peer.setLocalDescription(offer);

  send_signal("webrtc_offer", { sdp: offer.sdp });
}
