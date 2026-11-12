//! ureq 2's API for the browser build. A page can't open raw sockets and the
//! runner's sandbox blocks most cross-origin requests, so every request fails
//! with a transport error, the same way the game sees a machine with no network.
//! Skins, Realms, the mod registry and the like then fall back as they do offline.

use std::fmt;
use std::io::{self, Read};
use std::time::Duration;

/// Starts a GET request.
pub fn get(url: &str) -> Request {
    Request::new("GET", url)
}
/// Starts a POST request.
pub fn post(url: &str) -> Request {
    Request::new("POST", url)
}
/// Starts a PUT request.
pub fn put(url: &str) -> Request {
    Request::new("PUT", url)
}
/// Starts a DELETE request.
pub fn delete(url: &str) -> Request {
    Request::new("DELETE", url)
}
/// Starts a HEAD request.
pub fn head(url: &str) -> Request {
    Request::new("HEAD", url)
}
/// Starts a request with any method.
pub fn request(method: &str, url: &str) -> Request {
    Request::new(method, url)
}
/// A default agent.
pub fn agent() -> Agent {
    Agent::new()
}

/// Builds an [`Agent`]; every setting is accepted and has no effect.
#[derive(Clone, Debug, Default)]
pub struct AgentBuilder;

impl AgentBuilder {
    pub fn new() -> Self {
        Self
    }
    pub fn build(self) -> Agent {
        Agent
    }
    pub fn timeout(self, _: Duration) -> Self {
        self
    }
    pub fn timeout_connect(self, _: Duration) -> Self {
        self
    }
    pub fn timeout_read(self, _: Duration) -> Self {
        self
    }
    pub fn timeout_write(self, _: Duration) -> Self {
        self
    }
    pub fn user_agent(self, _: &str) -> Self {
        self
    }
    pub fn redirects(self, _: u32) -> Self {
        self
    }
    pub fn https_only(self, _: bool) -> Self {
        self
    }
    pub fn no_delay(self, _: bool) -> Self {
        self
    }
    pub fn max_idle_connections(self, _: usize) -> Self {
        self
    }
    pub fn max_idle_connections_per_host(self, _: usize) -> Self {
        self
    }
    pub fn try_proxy_from_env(self, _: bool) -> Self {
        self
    }
    pub fn proxy(self, _: Proxy) -> Self {
        self
    }
}

/// A proxy setting, accepted and ignored.
#[derive(Clone, Debug)]
pub struct Proxy;

impl Proxy {
    pub fn new(_: impl AsRef<str>) -> Result<Self, Error> {
        Ok(Self)
    }
}

/// An HTTP agent.
#[derive(Clone, Debug, Default)]
pub struct Agent;

impl Agent {
    pub fn new() -> Self {
        Self
    }
    pub fn request(&self, method: &str, url: &str) -> Request {
        Request::new(method, url)
    }
    pub fn get(&self, url: &str) -> Request {
        Request::new("GET", url)
    }
    pub fn post(&self, url: &str) -> Request {
        Request::new("POST", url)
    }
    pub fn put(&self, url: &str) -> Request {
        Request::new("PUT", url)
    }
    pub fn delete(&self, url: &str) -> Request {
        Request::new("DELETE", url)
    }
    pub fn head(&self, url: &str) -> Request {
        Request::new("HEAD", url)
    }
}

/// A request that is never sent.
#[derive(Clone, Debug)]
pub struct Request {
    method: String,
    url: String,
}

impl Request {
    fn new(method: &str, url: &str) -> Self {
        Self {
            method: method.to_owned(),
            url: url.to_owned(),
        }
    }
    pub fn method(&self) -> &str {
        &self.method
    }
    pub fn url(&self) -> &str {
        &self.url
    }
    pub fn set(self, _: &str, _: &str) -> Self {
        self
    }
    pub fn query(self, _: &str, _: &str) -> Self {
        self
    }
    pub fn query_pairs<'a>(self, _: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        self
    }
    pub fn timeout(self, _: Duration) -> Self {
        self
    }
    pub fn call(self) -> Result<Response, Error> {
        Err(self.offline())
    }
    pub fn send(self, _: impl Read) -> Result<Response, Error> {
        Err(self.offline())
    }
    pub fn send_bytes(self, _: &[u8]) -> Result<Response, Error> {
        Err(self.offline())
    }
    pub fn send_string(self, _: &str) -> Result<Response, Error> {
        Err(self.offline())
    }
    pub fn send_form(self, _: &[(&str, &str)]) -> Result<Response, Error> {
        Err(self.offline())
    }
    fn offline(&self) -> Error {
        Error::Transport(Transport {
            url: self.url.clone(),
        })
    }
}

/// A response. None is ever produced, but the type is named in error matches.
#[derive(Debug)]
pub struct Response {
    status: u16,
}

impl Response {
    pub fn status(&self) -> u16 {
        self.status
    }
    pub fn status_text(&self) -> &str {
        ""
    }
    pub fn get_url(&self) -> &str {
        ""
    }
    pub fn header(&self, _: &str) -> Option<&str> {
        None
    }
    pub fn headers_names(&self) -> Vec<String> {
        Vec::new()
    }
    pub fn content_type(&self) -> &str {
        ""
    }
    pub fn charset(&self) -> &str {
        ""
    }
    pub fn into_reader(self) -> Box<dyn Read + Send + Sync + 'static> {
        Box::new(io::empty())
    }
    pub fn into_string(self) -> io::Result<String> {
        Ok(String::new())
    }
}

/// Why a request failed: always [`Error::Transport`] here.
#[derive(Debug)]
pub enum Error {
    Status(u16, Response),
    Transport(Transport),
}

impl Error {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Error::Status(..) => ErrorKind::HTTP,
            Error::Transport(t) => t.kind(),
        }
    }
    pub fn into_response(self) -> Option<Response> {
        match self {
            Error::Status(_, r) => Some(r),
            Error::Transport(_) => None,
        }
    }
    pub fn into_transport(self) -> Option<Transport> {
        match self {
            Error::Status(..) => None,
            Error::Transport(t) => Some(t),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Status(code, _) => write!(f, "status code {code}"),
            Error::Transport(t) => t.fmt(f),
        }
    }
}

impl std::error::Error for Error {}

impl From<Error> for io::Error {
    fn from(e: Error) -> Self {
        io::Error::other(e.to_string())
    }
}

/// A failure below HTTP: here, always "no network in the browser build".
#[derive(Debug)]
pub struct Transport {
    url: String,
}

impl Transport {
    pub fn kind(&self) -> ErrorKind {
        ErrorKind::ConnectionFailed
    }
    pub fn message(&self) -> Option<&str> {
        Some("no network in the browser build")
    }
    pub fn url(&self) -> Option<&str> {
        Some(&self.url)
    }
}

impl fmt::Display for Transport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: no network in the browser build", self.url)
    }
}

impl std::error::Error for Transport {}

/// ureq's error kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
pub enum ErrorKind {
    InvalidUrl,
    UnknownScheme,
    Dns,
    InsecureRequestHttpsOnly,
    ConnectionFailed,
    TooManyRedirects,
    BadStatus,
    BadHeader,
    Io,
    InvalidProxyUrl,
    ProxyConnect,
    ProxyUnauthorized,
    HTTP,
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
