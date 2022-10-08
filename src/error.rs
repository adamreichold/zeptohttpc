// Copyright 2020 Adam Reichold
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
use std::error::Error as StdError;
use std::fmt;
use std::io;

#[derive(Debug)]
pub enum Error {
    MissingScheme,
    MissingAuthority,
    MissingStatus,
    UnsupportedProtocol,
    TooManyRedirects,
    InvalidChunkSize,
    InvalidLineEnding,
    Io(io::Error),
    Http(http::Error),
    HttpInvalidUri(http::uri::InvalidUri),
    HttpInvalidUriParts(http::uri::InvalidUriParts),
    HttpHeaderInvalidValue(http::header::InvalidHeaderValue),
    HttpHeaderToStr(http::header::ToStrError),
    Httparse(httparse::Error),
    #[cfg(feature = "native-tls")]
    NativeTls(native_tls::Error),
    #[cfg(feature = "rustls")]
    Tls(rustls::Error),
    #[cfg(feature = "rustls")]
    InvalidDnsName(String),
    #[cfg(feature = "json")]
    Json(serde_json::Error),
}

impl StdError for Error {
    fn cause(&self) -> Option<&dyn StdError> {
        match self {
            Self::Io(err) => Some(err),
            Self::Http(err) => Some(err),
            Self::HttpInvalidUri(err) => Some(err),
            Self::HttpInvalidUriParts(err) => Some(err),
            Self::HttpHeaderInvalidValue(err) => Some(err),
            Self::HttpHeaderToStr(err) => Some(err),
            Self::Httparse(err) => Some(err),
            #[cfg(feature = "native-tls")]
            Self::NativeTls(err) => Some(err),
            #[cfg(feature = "rustls")]
            Self::Tls(err) => Some(err),
            #[cfg(feature = "json")]
            Self::Json(err) => Some(err),
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingScheme => write!(fmt, "Missing scheme"),
            Self::MissingAuthority => write!(fmt, "Missing authority"),
            Self::MissingStatus => write!(fmt, "Missing status"),
            Self::UnsupportedProtocol => write!(fmt, "Unsupported protocol"),
            Self::TooManyRedirects => write!(fmt, "Too many redirects"),
            Self::InvalidChunkSize => write!(fmt, "Invalid chunk size"),
            Self::InvalidLineEnding => write!(fmt, "Invalid line ending"),
            Self::Io(err) => write!(fmt, "I/O error: {}", err),
            Self::Http(err) => write!(fmt, "HTTP error: {}", err),
            Self::HttpInvalidUri(err) => write!(fmt, "HTTP invalid URI: {}", err),
            Self::HttpInvalidUriParts(err) => write!(fmt, "HTTP invalid URI parts: {}", err),
            Self::HttpHeaderInvalidValue(err) => write!(fmt, "HTTP header invalid value: {}", err),
            Self::HttpHeaderToStr(err) => write!(fmt, "HTTP header to string: {}", err),
            Self::Httparse(err) => write!(fmt, "HTTP parser error: {}", err),
            #[cfg(feature = "native-tls")]
            Self::NativeTls(err) => write!(fmt, "TLS error: {}", err),
            #[cfg(feature = "rustls")]
            Self::Tls(err) => write!(fmt, "TLS error: {}", err),
            #[cfg(feature = "rustls")]
            Self::InvalidDnsName(name) => write!(fmt, "Invalid DNS name: {}", name),
            #[cfg(feature = "json")]
            Self::Json(err) => write!(fmt, "JSON error: {}", err),
        }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<io::ErrorKind> for Error {
    fn from(err: io::ErrorKind) -> Self {
        Self::Io(err.into())
    }
}

impl From<http::Error> for Error {
    fn from(err: http::Error) -> Self {
        Self::Http(err)
    }
}

impl From<http::uri::InvalidUri> for Error {
    fn from(err: http::uri::InvalidUri) -> Self {
        Self::HttpInvalidUri(err)
    }
}

impl From<http::uri::InvalidUriParts> for Error {
    fn from(err: http::uri::InvalidUriParts) -> Self {
        Self::HttpInvalidUriParts(err)
    }
}

impl From<http::header::InvalidHeaderValue> for Error {
    fn from(err: http::header::InvalidHeaderValue) -> Self {
        Self::HttpHeaderInvalidValue(err)
    }
}

impl From<http::header::ToStrError> for Error {
    fn from(err: http::header::ToStrError) -> Self {
        Self::HttpHeaderToStr(err)
    }
}

impl From<httparse::Error> for Error {
    fn from(err: httparse::Error) -> Self {
        Self::Httparse(err)
    }
}

#[cfg(feature = "native-tls")]
impl From<native_tls::Error> for Error {
    fn from(err: native_tls::Error) -> Self {
        Self::NativeTls(err)
    }
}

#[cfg(feature = "rustls")]
impl From<rustls::Error> for Error {
    fn from(err: rustls::Error) -> Self {
        Self::Tls(err)
    }
}

#[cfg(feature = "json")]
impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}
