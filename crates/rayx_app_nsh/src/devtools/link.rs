//! The agent's connection to the hub: one `connect(url, hello, on_frame)`, with a native half and
//! a web half.
//!
//! - **Native.** A WebSocket client (`tokio-tungstenite`, the version NSH compiles) on a thread of
//!   its own with a single-threaded runtime. It dials, sends `hello` on every connection, hands
//!   every text frame to `on_frame` on that thread and sends what [`Link::send`] queues; when
//!   the connection fails or ends it dials again after 250 ms, doubling up to 5 s. Frames queued
//!   while it is down are dropped: the hub asks again.
//! - **Web.** NSH's `System.websocket` transport (the page's WebSocket), whose events arrive
//!   on the engine's thread; the same `hello`, frames and backoff, the waits on NSH's timer.

use std::time::Duration;

/// The first wait before dialling again.
pub(crate) const FIRST_BACKOFF: Duration = Duration::from_millis(250);
/// The longest wait before dialling again.
pub(crate) const LAST_BACKOFF: Duration = Duration::from_secs(5);

/// What the link hands the agent.
#[derive(Debug)]
pub(crate) enum Event {
    /// A text frame from the hub.
    Frame(String),
    /// The connection ended (the hub's state of this app is gone).
    Closed,
}

/// The waits between dials: 250 ms, doubling, at most 5 s.
pub(crate) fn backoff(attempt: u32) -> Duration {
    FIRST_BACKOFF
        .saturating_mul(2u32.saturating_pow(attempt.min(16)))
        .min(LAST_BACKOFF)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::Link;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use futures::{SinkExt as _, StreamExt as _};
    use tokio::sync::mpsc;
    use tokio_tungstenite::tungstenite::Message;

    use super::{Event, backoff};

    /// The link: what the agent sends goes to the link's thread.
    pub(crate) struct Link {
        outgoing: mpsc::UnboundedSender<String>,
    }

    impl Link {
        /// Dials `url` on a thread of its own; `hello` opens every connection.
        pub(crate) fn connect(
            url: String,
            hello: String,
            on_event: impl Fn(Event) + Send + 'static,
            _transport: Option<std::sync::Arc<dyn nsh_core::websocket::WebSocketTransport>>,
        ) -> Link {
            let (outgoing, frames) = mpsc::unbounded_channel();
            let started = std::thread::Builder::new()
                .name("rayx-devtools".into())
                .spawn(move || {
                    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    else {
                        return;
                    };
                    runtime.block_on(run(url, hello, frames, on_event));
                });
            drop(started);
            Link { outgoing }
        }

        /// Queues `frame` for the hub; dropped while the link is down.
        pub(crate) fn send(&self, frame: String) {
            let _ = self.outgoing.send(frame);
        }
    }

    async fn run(
        url: String,
        hello: String,
        mut frames: mpsc::UnboundedReceiver<String>,
        on_event: impl Fn(Event),
    ) {
        let mut attempt = 0;
        loop {
            if let Ok((socket, _)) = tokio_tungstenite::connect_async(url.as_str()).await {
                attempt = 0;
                // What was queued while down answered a connection that is gone.
                while frames.try_recv().is_ok() {}
                let (mut sink, mut stream) = socket.split();
                if sink.send(Message::text(hello.clone())).await.is_ok() {
                    loop {
                        tokio::select! {
                            frame = stream.next() => match frame {
                                Some(Ok(Message::Text(text))) => on_event(Event::Frame(text.to_string())),
                                Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                                Some(Ok(_)) => {}
                            },
                            frame = frames.recv() => match frame {
                                Some(frame) => {
                                    if sink.send(Message::text(frame)).await.is_err() {
                                        break;
                                    }
                                }
                                // The agent is gone: so is the link.
                                None => return,
                            },
                        }
                    }
                }
                on_event(Event::Closed);
            }
            if frames.is_closed() {
                return;
            }
            tokio::time::sleep(backoff(attempt)).await;
            attempt += 1;
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) use web::Link;

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::sync::Arc;
    use std::time::Duration;

    use futures::StreamExt as _;
    use futures::channel::mpsc::{UnboundedSender, unbounded};
    use nsh_core::prelude::*;
    use nsh_core::websocket::{
        WebSocketData, WebSocketEvent, WebSocketRequest, WebSocketTask, WebSocketTransport,
    };

    use super::{Event, backoff};

    /// The link on the web: a socket of the page's, through NSH's transport, on the engine's
    /// thread.
    pub(crate) struct Link {
        state: Option<Rc<State>>,
    }

    struct State {
        url: String,
        hello: String,
        transport: Arc<dyn WebSocketTransport>,
        on_event: Box<dyn Fn(Event)>,
        task: RefCell<Option<Box<dyn WebSocketTask>>>,
        attempt: Cell<u32>,
        /// The waits before the next dials, for the task that redials: the socket's events
        /// arrive outside the reactive runtime, where nothing can be spawned.
        redial: UnboundedSender<Duration>,
    }

    impl Link {
        /// Dials `url` through `transport`; without one (a host that supplies none) the link
        /// stays down.
        pub(crate) fn connect(
            url: String,
            hello: String,
            on_event: impl Fn(Event) + Send + 'static,
            transport: Option<Arc<dyn WebSocketTransport>>,
        ) -> Link {
            let Some(transport) = transport else {
                return Link { state: None };
            };
            let (redial, mut waits) = unbounded();
            let state = Rc::new(State {
                url,
                hello,
                transport,
                on_event: Box::new(on_event),
                task: RefCell::new(None),
                attempt: Cell::new(0),
                redial,
            });
            // Spawned here, in the runtime the agent starts in; it ends with the link.
            let weak = Rc::downgrade(&state);
            drop(spawn(async move {
                while let Some(wait) = waits.next().await {
                    timer(wait).await;
                    let Some(state) = weak.upgrade() else {
                        break;
                    };
                    dial(&state);
                }
            }));
            dial(&state);
            Link { state: Some(state) }
        }

        /// Sends `frame` to the hub; dropped while the link is down.
        pub(crate) fn send(&self, frame: String) {
            if let Some(state) = &self.state
                && let Some(task) = state.task.borrow_mut().as_mut()
            {
                let _ = task.send(WebSocketData::Text(frame));
            }
        }
    }

    fn dial(state: &Rc<State>) {
        let request = WebSocketRequest {
            url: state.url.clone(),
            protocols: Vec::new(),
            headers: Vec::new(),
        };
        let weak = Rc::downgrade(state);
        let events = Box::new(move |event: WebSocketEvent| {
            let Some(state) = weak.upgrade() else {
                return;
            };
            match event {
                WebSocketEvent::Open { .. } => {
                    state.attempt.set(0);
                    if let Some(task) = state.task.borrow_mut().as_mut() {
                        let _ = task.send(WebSocketData::Text(state.hello.clone()));
                    }
                }
                WebSocketEvent::Message(WebSocketData::Text(text)) => {
                    (state.on_event)(Event::Frame(text));
                }
                WebSocketEvent::Message(WebSocketData::Bytes(_)) | WebSocketEvent::Error(_) => {}
                WebSocketEvent::Close { .. } => {
                    drop(state.task.borrow_mut().take());
                    (state.on_event)(Event::Closed);
                    again(&state);
                }
            }
        });
        match state.transport.connect(request, events) {
            Ok(task) => *state.task.borrow_mut() = Some(task),
            Err(_) => again(state),
        }
    }

    /// Dials again after the backoff, through the redialling task.
    fn again(state: &Rc<State>) {
        let attempt = state.attempt.get();
        state.attempt.set(attempt + 1);
        let _ = state.redial.unbounded_send(backoff(attempt));
    }
}
