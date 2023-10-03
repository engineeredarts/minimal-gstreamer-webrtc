mod signals;

fn main() {
    println!("Minimal GStreamer WebRTC - Backend");

    signals::connect("ws://127.0.0.1:10001");
}
