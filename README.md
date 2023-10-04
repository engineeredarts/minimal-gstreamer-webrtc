# Minimal GStreamer WebRTC

A self contained WebRTC system that can be used to replicate GStreamer issues and attached to bug reports.

## signal_server

A very simple signal server.

- Accepts WebSocket connections on two ports, 10001 and 10002
- Forwards all messages received on one port to the other
