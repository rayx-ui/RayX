//! A test hub for the dev tools proofs: listens on loopback, takes an app's connection and its
//! `hello`, and asks it requests. Plain JSON: `rayx-cli` links no RayX crate, the protocol's
//! included.

#![allow(dead_code)]

use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tungstenite::{Message, WebSocket};

/// The hub's default port, where an app's agent dials unless told otherwise.
pub const DEFAULT_PORT: u16 = 9240;

pub struct Hub {
    listener: TcpListener,
    socket: Option<WebSocket<TcpStream>>,
    next: u64,
    /// The `hello`'s parameters.
    pub hello: Option<Value>,
}

impl Hub {
    /// Listens on `port` (the agent's default unless the app is told another).
    pub fn listen(port: u16) -> Hub {
        let listener = TcpListener::bind(("127.0.0.1", port))
            .unwrap_or_else(|e| panic!("port {port} is taken (a hub runs already?): {e}"));
        listener.set_nonblocking(true).expect("nonblocking");
        Hub {
            listener,
            socket: None,
            next: 1,
            hello: None,
        }
    }

    /// Takes the first connection and its `hello`; `alive` says whether the app still runs.
    pub fn accept(&mut self, within: Duration, mut alive: impl FnMut() -> bool) {
        let deadline = Instant::now() + within;
        let stream = loop {
            match self.listener.accept() {
                Ok((stream, _)) => break stream,
                Err(_) if Instant::now() < deadline && alive() => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => panic!("the app's agent did not connect: {e}"),
            }
        };
        stream.set_nonblocking(false).expect("blocking");
        stream
            .set_read_timeout(Some(Duration::from_secs(60)))
            .expect("timeout");
        let mut socket = tungstenite::accept(stream).expect("handshake");
        let Ok(Message::Text(text)) = socket.read() else {
            panic!("no hello");
        };
        let hello: Value = serde_json::from_str(&text).expect("JSON");
        assert_eq!(hello["method"], "hello", "{hello}");
        self.hello = Some(hello["params"].clone());
        self.socket = Some(socket);
    }

    /// Asks `method`; its result, or its error object.
    pub fn ask(&mut self, method: &str, params: Value) -> Result<Value, Value> {
        let id = self.next;
        self.next += 1;
        let socket = self.socket.as_mut().expect("connected");
        let frame = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        socket.send(Message::text(frame.to_string())).expect("send");
        loop {
            let Ok(Message::Text(text)) = socket.read() else {
                panic!("no answer to {method}");
            };
            let message: Value = serde_json::from_str(&text).expect("JSON");
            if message["id"] == id && message.get("method").is_none() {
                return match message.get("error") {
                    Some(error) => Err(error.clone()),
                    None => Ok(message["result"].clone()),
                };
            }
        }
    }
}
