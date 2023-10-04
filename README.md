# Minimal GStreamer WebRTC

A self contained WebRTC system that can be used to replicate GStreamer issues and attached to bug reports.

## signal_server

A very simple signal server.

- Accepts WebSocket connections on two ports, 10001 and 10002
- Forwards all messages received on one port to the other

## webrtc_backend

A basic WebRTC application using [gstreamer-webrtc](https://crates.io/crates/gstreamer-webrtc).

- Connects to the signal server on port 10001
- Waits for an offer from the remote peer and handles connection
- Logs any WebRTC-related signals

NB uses GStreamer 1.22, assumed to be built from source and installed to a custom location - see [run.sh](webrtc_backend/run.sh).

## webrtc_frontend_web

Browser UI which connects to the backend via WebRTC.

- [run_http_server.sh](webrtc_frontend_web/run_http_server.sh) serves the UI at http://localhost:1000
- Connects to the signal server on port 10002
- Open the console to see things happen
