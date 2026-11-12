//! arboard's clipboard for the browser build. The page can't read the system
//! clipboard without a permission prompt, so `Clipboard::new` fails and the game
//! keeps text it copied itself, as it does on a desktop with no clipboard.

use std::fmt;

/// arboard's error type.
#[derive(Debug)]
pub enum Error {
    ContentNotAvailable,
    ClipboardNotSupported,
    ClipboardOccupied,
    ConversionFailure,
    Unknown { description: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("no system clipboard in the browser build")
    }
}

impl std::error::Error for Error {}

/// A system clipboard; none can be opened here.
#[derive(Debug)]
pub struct Clipboard(());

impl Clipboard {
    pub fn new() -> Result<Self, Error> {
        Err(Error::ClipboardNotSupported)
    }
    pub fn get_text(&mut self) -> Result<String, Error> {
        Err(Error::ClipboardNotSupported)
    }
    pub fn set_text<'a>(&mut self, _text: impl Into<std::borrow::Cow<'a, str>>) -> Result<(), Error> {
        Err(Error::ClipboardNotSupported)
    }
}
