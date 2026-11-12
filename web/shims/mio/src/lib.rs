//! mio's polling API for the browser build. A page can't accept connections or
//! poll sockets, so [`Poll::new`] fails with `Unsupported` and the LAN host
//! reports that the world can't be opened to the network.

use std::io;
use std::ops::BitOr;
use std::time::Duration;

fn unsupported() -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, "no sockets in the browser build")
}

/// A registration's id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Token(pub usize);

/// What a registration waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interest(u8);

impl Interest {
    pub const READABLE: Interest = Interest(1);
    pub const WRITABLE: Interest = Interest(2);
    pub const fn add(self, other: Interest) -> Interest {
        Interest(self.0 | other.0)
    }
    pub const fn is_readable(self) -> bool {
        self.0 & 1 != 0
    }
    pub const fn is_writable(self) -> bool {
        self.0 & 2 != 0
    }
}

impl BitOr for Interest {
    type Output = Interest;
    fn bitor(self, other: Interest) -> Interest {
        self.add(other)
    }
}

/// A poll; none can be made here.
#[derive(Debug)]
pub struct Poll {
    registry: Registry,
}

impl Poll {
    pub fn new() -> io::Result<Poll> {
        Err(unsupported())
    }
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
    pub fn poll(&mut self, events: &mut Events, _timeout: Option<Duration>) -> io::Result<()> {
        events.clear();
        Err(unsupported())
    }
}

/// A poll's registry.
#[derive(Debug)]
pub struct Registry(());

impl Registry {
    pub fn register<S: event::Source + ?Sized>(&self, _: &mut S, _: Token, _: Interest) -> io::Result<()> {
        Err(unsupported())
    }
    pub fn reregister<S: event::Source + ?Sized>(&self, _: &mut S, _: Token, _: Interest) -> io::Result<()> {
        Err(unsupported())
    }
    pub fn deregister<S: event::Source + ?Sized>(&self, _: &mut S) -> io::Result<()> {
        Err(unsupported())
    }
}

/// Wakes a poll from another thread.
#[derive(Debug)]
pub struct Waker(());

impl Waker {
    pub fn new(_: &Registry, _: Token) -> io::Result<Waker> {
        Err(unsupported())
    }
    pub fn wake(&self) -> io::Result<()> {
        Err(unsupported())
    }
}

/// The events a poll returned: always none.
#[derive(Debug, Default)]
pub struct Events(Vec<event::Event>);

impl Events {
    pub fn with_capacity(_: usize) -> Events {
        Events(Vec::new())
    }
    pub fn iter(&self) -> std::slice::Iter<'_, event::Event> {
        self.0.iter()
    }
    pub fn is_empty(&self) -> bool {
        true
    }
    pub fn clear(&mut self) {}
}

impl<'a> IntoIterator for &'a Events {
    type Item = &'a event::Event;
    type IntoIter = std::slice::Iter<'a, event::Event>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub mod event {
    use super::Token;

    /// Something a poll can watch.
    pub trait Source {}

    /// One readiness event; none is ever made.
    #[derive(Debug)]
    pub struct Event(Token);

    impl Event {
        pub fn token(&self) -> Token {
            self.0
        }
        pub fn is_readable(&self) -> bool {
            false
        }
        pub fn is_writable(&self) -> bool {
            false
        }
        pub fn is_error(&self) -> bool {
            false
        }
        pub fn is_read_closed(&self) -> bool {
            false
        }
        pub fn is_write_closed(&self) -> bool {
            false
        }
    }
}

pub mod net {
    use std::io::{self, Read, Write};
    use std::net::{Shutdown, SocketAddr};

    /// A polled TCP stream, wrapping std's (which fails every call on wasm32).
    #[derive(Debug)]
    pub struct TcpStream(std::net::TcpStream);

    impl TcpStream {
        pub fn from_std(stream: std::net::TcpStream) -> TcpStream {
            TcpStream(stream)
        }
        pub fn peer_addr(&self) -> io::Result<SocketAddr> {
            self.0.peer_addr()
        }
        pub fn local_addr(&self) -> io::Result<SocketAddr> {
            self.0.local_addr()
        }
        pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
            self.0.shutdown(how)
        }
        pub fn set_nodelay(&self, on: bool) -> io::Result<()> {
            self.0.set_nodelay(on)
        }
    }

    impl Read for TcpStream {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            self.0.read(out)
        }
    }

    impl Write for TcpStream {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            self.0.write(data)
        }
        fn flush(&mut self) -> io::Result<()> {
            self.0.flush()
        }
    }

    impl super::event::Source for TcpStream {}
}
