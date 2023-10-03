use tungstenite::Message;
use url::Url;

pub fn connect(url: &str) {
    println!("[Signals] connecting to signal server {url}");

    let url = Url::parse(url).unwrap();
    let (mut socket, _response) = tungstenite::connect(url).unwrap();

    println!("[Signals] CONNECTED");

    socket.send(Message::Text("hello".into())).unwrap();

    loop {
        let msg = socket.read().unwrap();
        println!("[Signals] RECEIVED {msg:?}");
    }
}
