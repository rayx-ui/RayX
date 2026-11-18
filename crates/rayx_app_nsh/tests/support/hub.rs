//! A test hub for the dev tools' agent: listens on a loopback port, takes the agent's
//! connection and its `hello`, asks requests and collects notifications.

#![allow(dead_code)]

use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use rayx_devtools_protocol::{self as proto, Id, Route, RpcError};
use serde_json::Value;
use tungstenite::{Message, WebSocket};

/// A hub listening on a port of its own.
pub struct Hub {
    listener: TcpListener,
    socket: Option<WebSocket<TcpStream>>,
    next: u64,
    /// The `hello` the agent opened with.
    pub hello: Option<proto::Hello>,
    /// The notifications heard so far: method and parameters.
    pub notifications: Vec<(String, Value)>,
}

impl Hub {
    pub fn listen() -> Hub {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        Hub {
            listener,
            socket: None,
            next: 1,
            hello: None,
            notifications: Vec::new(),
        }
    }

    pub fn url(&self) -> String {
        format!("ws://{}", self.listener.local_addr().unwrap())
    }

    /// Takes the agent's connection and its `hello`.
    pub fn accept(&mut self, within: Duration) {
        let deadline = Instant::now() + within;
        let stream = loop {
            match self.listener.accept() {
                Ok((stream, _)) => break stream,
                Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(error) => panic!("the agent did not connect: {error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        let hello = loop {
            match socket.read() {
                Ok(Message::Text(text)) => break text.to_string(),
                Ok(_) => {}
                Err(_) if Instant::now() < deadline => {}
                Err(error) => panic!("no hello: {error}"),
            }
        };
        let Route::Request { params, .. } = proto::route(&hello) else {
            panic!("hello is a request: {hello}");
        };
        self.hello = Some(params.decode().unwrap());
        self.socket = Some(socket);
    }

    /// Asks `method` with `params`; its result, or its error.
    pub fn ask(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        let id = self.next;
        self.next += 1;
        let socket = self.socket.as_mut().expect("connected");
        socket
            .send(Message::text(proto::encode_request(
                &Id::from(id),
                method,
                &params,
            )))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            assert!(Instant::now() < deadline, "no answer to {method}");
            let text = match socket.read() {
                Ok(Message::Text(text)) => text.to_string(),
                _ => continue,
            };
            match proto::route(&text) {
                Route::Response {
                    id: answered,
                    outcome,
                } if answered == Id::from(id) => {
                    return outcome.map(|result| serde_json::from_str(result.raw()).unwrap());
                }
                Route::Notification { method, params } => self.notifications.push((
                    method.to_owned(),
                    serde_json::from_str(params.raw()).unwrap(),
                )),
                _ => {}
            }
        }
    }

    /// Reads what arrives for `time`, keeping the notifications.
    pub fn listen_for(&mut self, time: Duration) {
        let deadline = Instant::now() + time;
        let socket = self.socket.as_mut().expect("connected");
        while Instant::now() < deadline {
            if let Ok(Message::Text(text)) = socket.read()
                && let Route::Notification { method, params } = proto::route(&text)
            {
                self.notifications.push((
                    method.to_owned(),
                    serde_json::from_str(params.raw()).unwrap(),
                ));
            }
        }
    }
}
