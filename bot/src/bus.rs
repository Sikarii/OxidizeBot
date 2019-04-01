use crate::{player, spotify};
use failure::format_err;
use futures::{future, Async, Future, Poll, Stream};
use hashbrown::HashMap;
use parking_lot::Mutex;
use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
};
use tokio::{
    io::{self, AsyncRead, WriteHalf},
    net::{TcpListener, TcpStream},
};

pub type Reader<T> = tokio_bus::BusReader<T>;

struct Inner {
    bus: tokio_bus::Bus<Message>,
    /// Latest instance of all messages.
    latest: HashMap<&'static str, Message>,
}

/// Bus system.
pub struct Bus {
    bus: Mutex<Inner>,
    address: SocketAddr,
}

impl Bus {
    /// Create a new notifier.
    pub fn new() -> Self {
        Bus {
            bus: Mutex::new(Inner {
                bus: tokio_bus::Bus::new(1024),
                latest: HashMap::new(),
            }),
            address: SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 4444),
        }
    }

    /// Send a message to the bus.
    pub fn send(&self, m: Message) {
        let mut inner = self.bus.lock();

        if let Some(key) = m.cache() {
            inner.latest.insert(key, m.clone());
        }

        if let Err(_) = inner.bus.try_broadcast(m) {
            log::error!("failed to send notification: bus is full");
        }
    }

    /// Get the latest messages received.
    pub fn latest(&self) -> Vec<Message> {
        let inner = self.bus.lock();
        inner.latest.values().cloned().collect()
    }

    /// Create a receiver of the bus.
    pub fn add_rx(self: Arc<Self>) -> Reader<Message> {
        self.bus.lock().bus.add_rx()
    }

    /// Listen for incoming connections and hand serialized bus messages to connected sockets.
    pub fn listen(self: Arc<Self>) -> impl Future<Item = (), Error = failure::Error> {
        let listener = future::result(TcpListener::bind(&self.address));

        listener.from_err::<failure::Error>().and_then(|listener| {
            listener
                .incoming()
                .from_err::<failure::Error>()
                .and_then(move |s| {
                    let (_, writer) = s.split();
                    let rx = self.bus.lock().bus.add_rx();

                    let handler = BusHandler::new(writer, rx)
                        .map_err(|e| {
                            log::error!("failed to process outgoing message: {}", e);
                        })
                        .for_each(|_| Ok(()));

                    tokio::spawn(handler);
                    Ok(())
                })
                .for_each(|_| Ok(()))
        })
    }
}

enum BusHandlerState {
    Receiving,
    Serialize(Message),
    Send(io::WriteAll<WriteHalf<TcpStream>, String>),
}

/// Handles reading messages of a buss and writing them to a TcpStream.
struct BusHandler {
    writer: Option<WriteHalf<TcpStream>>,
    rx: tokio_bus::BusReader<Message>,
    state: BusHandlerState,
}

impl BusHandler {
    pub fn new(writer: WriteHalf<TcpStream>, rx: tokio_bus::BusReader<Message>) -> Self {
        Self {
            writer: Some(writer),
            rx,
            state: BusHandlerState::Receiving,
        }
    }
}

impl Stream for BusHandler {
    type Item = ();
    type Error = failure::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        use self::BusHandlerState::*;

        loop {
            self.state = match self.state {
                Receiving => match self.rx.poll() {
                    Ok(Async::Ready(Some(m))) => Serialize(m),
                    Ok(Async::Ready(None)) => return Ok(Async::Ready(None)),
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(e) => return Err(failure::Error::from(e)),
                },
                Serialize(ref m) => match (serde_json::to_string(m), self.writer.take()) {
                    (Ok(json), Some(writer)) => Send(io::write_all(writer, format!("{}\n", json))),
                    (_, None) => return Err(format_err!("writer not available")),
                    (Err(e), _) => return Err(failure::Error::from(e)),
                },
                Send(ref mut f) => match f.poll() {
                    Ok(Async::Ready((writer, _))) => {
                        self.writer = Some(writer);
                        self.state = Receiving;
                        continue;
                    }
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(e) => return Err(failure::Error::from(e)),
                },
            }
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(tag = "type")]
pub enum Message {
    #[serde(rename = "firework")]
    Firework,
    #[serde(rename = "ping")]
    Ping,
    /// Progress of current song.
    #[serde(rename = "song/progress")]
    SongProgress { elapsed: u64, duration: u64 },
    #[serde(rename = "song/current")]
    SongCurrent {
        track: Option<spotify::FullTrack>,
        user: Option<String>,
        paused: bool,
    },
}

impl Message {
    /// Construct a message about song progress.
    pub fn song_progress(song: Option<&player::Song>) -> Self {
        let song = match song {
            Some(song) => song,
            None => {
                return Message::SongProgress {
                    elapsed: 0,
                    duration: 0,
                }
            }
        };

        Message::SongProgress {
            elapsed: song.elapsed().as_secs(),
            duration: song.duration().as_secs(),
        }
    }

    /// Message indicating that no song is playing.
    pub fn from_song(song: Option<&player::Song>, paused: bool) -> Self {
        let song = match song {
            Some(song) => song,
            None => {
                return Message::SongCurrent {
                    track: None,
                    user: None,
                    paused,
                }
            }
        };

        Message::SongCurrent {
            track: Some(song.item.track.clone()),
            user: song.item.user.clone(),
            paused,
        }
    }

    /// Whether a message should be cached or not and under what key.
    pub fn cache(&self) -> Option<&'static str> {
        use self::Message::*;

        match *self {
            SongProgress { .. } => Some("song/progress"),
            SongCurrent { .. } => Some("song/current"),
            _ => None,
        }
    }
}