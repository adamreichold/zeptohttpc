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
#![forbid(unsafe_code)]

//! This crate aims to be the smallest possible yet practically useful HTTP client built on top of the `http` and `httparse` crates.
//!
//! ```
//! # use std::{error::Error, time::Duration};
//! use zeptohttpc::{http::Request, Options, RequestBuilderExt, RequestExt, ResponseExt};
//!
//! # fn main() -> Result<(), Box<dyn Error>> {
//! let req = Request::get("http://httpbin.org/base64/emVwdG9odHRwYw%3D%3D").empty().unwrap();
//!
//! let mut opts = Options::default();
//! opts.timeout = Some(Duration::from_secs(10));
//!
//! let resp = req.send_with_opts(opts).unwrap();
//!
//! let body = resp.into_string().unwrap();
//! assert_eq!("zeptohttpc", body);
//! #
//! # Ok(())
//! # }
//! ```

mod body_reader;
mod body_writer;
mod chunked;
#[cfg(feature = "encoding_rs")]
mod encoded;
mod error;
mod happy_eyeballs;
mod parse;
mod stream;
mod timeout;

pub use http;
pub use httparse;
#[cfg(feature = "native-tls")]
pub use native_tls;
#[cfg(feature = "tls")]
pub use rustls;
#[cfg(feature = "json")]
pub use serde;
#[cfg(feature = "json")]
pub use serde_json;
#[cfg(feature = "tls")]
pub use webpki;

pub use body_reader::BodyReader;
pub use body_writer::{BodyKind, BodyWriter};
pub use error::Error;

use std::convert::TryInto;
use std::io::{BufReader, BufWriter, Read, Result as IoResult, Seek, Write};
use std::marker::PhantomData;
#[cfg(feature = "tls")]
use std::sync::Arc;
use std::time::{Duration, Instant};

use http::{
    header::{
        Entry, HeaderValue, ACCEPT_ENCODING, CONNECTION, CONTENT_LENGTH, HOST, LOCATION,
        TRANSFER_ENCODING, USER_AGENT,
    },
    request::{Builder as RequestBuilder, Parts as RequestParts, Request},
    response::Response,
    uri::{PathAndQuery, Scheme, Uri},
    Error as HttpError, Version,
};
use httparse::{
    Response as ResponseParser,
    Status::{Complete, Partial},
    EMPTY_HEADER,
};
#[cfg(feature = "native-tls")]
use native_tls::TlsConnector;
#[cfg(feature = "tls")]
use rustls::ClientConfig;
#[cfg(feature = "json")]
use serde::{de::DeserializeOwned, ser::Serialize};

#[cfg(feature = "flate2")]
use body_writer::compressed_body::CompressedBody;
#[cfg(feature = "json")]
use body_writer::json_body::JsonBody;
use body_writer::{EmptyBody, IoBody, MemBody};
use chunked::ChunkedWriter;
use parse::parse;
use stream::Stream;

pub trait RequestBuilderExt {
    fn empty(self) -> Result<Request<EmptyBody>, HttpError>;
    #[allow(clippy::wrong_self_convention)]
    fn from_mem<B: AsRef<[u8]>>(self, body: B) -> Result<Request<MemBody<B>>, HttpError>;
    #[allow(clippy::wrong_self_convention)]
    fn from_io<B: Seek + Read>(self, body: B) -> Result<Request<IoBody<B>>, HttpError>;
    #[cfg(feature = "json")]
    fn json<B: Serialize>(self, body: B) -> Result<Request<JsonBody<B>>, HttpError>;
    #[cfg(feature = "json")]
    fn json_buffered<B: Serialize>(self, body: &B) -> Result<Request<MemBody<Vec<u8>>>, Error>;
}

impl RequestBuilderExt for RequestBuilder {
    fn empty(self) -> Result<Request<EmptyBody>, HttpError> {
        self.body(EmptyBody)
    }

    fn from_mem<B: AsRef<[u8]>>(self, body: B) -> Result<Request<MemBody<B>>, HttpError> {
        self.body(MemBody(body))
    }

    fn from_io<B: Seek + Read>(self, body: B) -> Result<Request<IoBody<B>>, HttpError> {
        self.body(IoBody(body))
    }

    #[cfg(feature = "json")]
    fn json<B: Serialize>(self, body: B) -> Result<Request<JsonBody<B>>, HttpError> {
        use http::header::CONTENT_TYPE;

        self.header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
            .body(JsonBody(body))
    }

    #[cfg(feature = "json")]
    fn json_buffered<B: Serialize>(self, body: &B) -> Result<Request<MemBody<Vec<u8>>>, Error> {
        use http::header::CONTENT_TYPE;
        use serde_json::ser::to_vec;

        self.header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
            .from_mem(to_vec(body)?)
            .map_err(Error::from)
    }
}

#[derive(Clone, Copy)]
pub struct Options<'a> {
    pub connect_timeout: Duration,
    pub connect_delay: Duration,
    pub timeout: Option<Duration>,
    pub follow_redirects: Option<usize>,
    #[cfg(feature = "native-tls")]
    pub tls_connector: Option<&'a TlsConnector>,
    #[cfg(feature = "tls")]
    pub client_config: Option<&'a Arc<ClientConfig>>,
    _private: PhantomData<&'a ()>,
}

impl Default for Options<'_> {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            connect_delay: Duration::from_millis(500),
            timeout: None,
            follow_redirects: Some(5),
            #[cfg(feature = "native-tls")]
            tls_connector: None,
            #[cfg(feature = "tls")]
            client_config: None,
            _private: PhantomData,
        }
    }
}

pub trait RequestExt {
    type Body;

    #[cfg(feature = "flate2")]
    fn compressed(self) -> Result<Request<CompressedBody<Self::Body>>, Error>;

    fn send(self) -> Result<Response<BodyReader>, Error>;
    fn send_with_opts(self, opts: Options<'_>) -> Result<Response<BodyReader>, Error>;
}

impl<B: BodyWriter> RequestExt for Request<B> {
    type Body = B;

    #[cfg(feature = "flate2")]
    fn compressed(mut self) -> Result<Request<CompressedBody<B>>, Error> {
        append_enconding(self.headers_mut().entry(TRANSFER_ENCODING), "gzip")?;

        Ok(self.map(CompressedBody))
    }

    fn send(self) -> Result<Response<BodyReader>, Error> {
        self.send_with_opts(Default::default())
    }

    fn send_with_opts(self, mut opts: Options<'_>) -> Result<Response<BodyReader>, Error> {
        let (mut parts, mut body) = self.into_parts();

        parts
            .headers
            .insert(CONNECTION, HeaderValue::from_static("close"));

        parts
            .headers
            .entry(USER_AGENT)
            .or_insert_with(|| HeaderValue::from_static(DEF_USER_AGENT));

        if cfg!(feature = "flate2") {
            parts
                .headers
                .insert(ACCEPT_ENCODING, HeaderValue::from_static("deflate, gzip"));
        }

        let chunked = match body.kind()? {
            BodyKind::Empty => false,
            BodyKind::KnownLength(len) => {
                parts.headers.insert(CONTENT_LENGTH, len.into());

                false
            }
            BodyKind::Chunked => {
                append_enconding(parts.headers.entry(TRANSFER_ENCODING), "chunked")?;

                true
            }
        };

        let mut start = Instant::now();

        loop {
            let scheme = parts.uri.scheme().ok_or(Error::MissingScheme)?;
            let authority = parts.uri.authority().ok_or(Error::MissingAuthority)?;

            let host = authority.host();
            parts.headers.insert(HOST, host.try_into()?);

            let port = match authority.port_u16() {
                Some(port) => port,
                None if scheme == &Scheme::HTTP => 80,
                #[cfg(any(feature = "native-tls", feature = "tls"))]
                None if scheme == &Scheme::HTTPS => 443,
                _ => return Err(Error::UnsupportedProtocol),
            };

            let mut stream = Stream::new(
                #[cfg(any(feature = "native-tls", feature = "tls"))]
                scheme,
                host,
                port,
                &opts,
            )?;

            write_request(&mut stream, &parts, &mut body, chunked)?;
            let resp = read_response(stream)?;

            let now = Instant::now();
            let elapsed = now.duration_since(start);
            start = now;

            if let Some(location) = handle_redirects(&resp, &mut opts, elapsed)? {
                let uri = parts.uri.into_parts();

                let mut location = location.into_parts();
                location.scheme = location.scheme.or(uri.scheme);
                location.authority = location.authority.or(uri.authority);

                parts.uri = location.try_into()?;
                continue;
            }

            return Ok(resp);
        }
    }
}

pub trait ResponseExt {
    fn into_vec(self) -> IoResult<Vec<u8>>;
    fn into_string(self) -> IoResult<String>;
    #[cfg(feature = "json")]
    fn json<T: DeserializeOwned>(self) -> IoResult<T>;
}

impl ResponseExt for Response<BodyReader> {
    fn into_vec(self) -> IoResult<Vec<u8>> {
        let mut buf = Vec::new();
        self.into_body().read_to_end(&mut buf)?;
        Ok(buf)
    }

    fn into_string(self) -> IoResult<String> {
        let mut buf = String::new();
        self.into_body().read_to_string(&mut buf)?;
        Ok(buf)
    }

    #[cfg(feature = "json")]
    fn json<T: DeserializeOwned>(self) -> IoResult<T> {
        use serde_json::de::from_reader;

        from_reader(self.into_body()).map_err(Into::into)
    }
}

fn append_enconding(
    encodings: Entry<'_, HeaderValue>,
    encoding: &'static str,
) -> Result<(), Error> {
    match encodings {
        Entry::Vacant(encodings) => {
            encodings.insert(HeaderValue::from_static(encoding));
        }
        Entry::Occupied(mut encodings) => {
            let mut encodings1 = encodings.get().to_str()?.to_owned();
            encodings1.push(',');
            encodings1.push_str(encoding);
            *encodings.get_mut() = HeaderValue::from_str(&encodings1)?;
        }
    }

    Ok(())
}

fn write_request<B: BodyWriter>(
    stream: &mut Stream,
    parts: &RequestParts,
    body: &mut B,
    chunked: bool,
) -> Result<(), Error> {
    let mut writer = BufWriter::new(stream);

    write!(
        writer,
        "{} {} {:?}\r\n",
        parts.method,
        parts.uri.path_and_query().map_or("/", PathAndQuery::as_str),
        parts.version
    )?;

    for (key, value) in &parts.headers {
        writer.write_all(key.as_ref())?;
        writer.write_all(b": ")?;
        writer.write_all(value.as_bytes())?;
        writer.write_all(b"\r\n")?;
    }

    writer.write_all(b"\r\n")?;

    if chunked {
        let mut writer = ChunkedWriter(&mut writer);
        body.write(&mut writer)?;
        writer.close()?;
    } else {
        body.write(&mut writer)?;
    }

    writer.flush()?;

    Ok(())
}

fn read_response(stream: Stream) -> Result<Response<BodyReader>, Error> {
    let mut reader = BufReader::new(stream);

    let resp = parse(&mut reader, |buf| -> Result<_, Error> {
        let mut headers = [EMPTY_HEADER; MAX_HEADERS];
        let mut parser = ResponseParser::new(&mut headers);

        match parser.parse(&buf)? {
            Complete(parsed) => {
                let mut resp = Response::builder();

                resp = resp.status(parser.code.ok_or(Error::MissingStatus)?);

                resp = match parser.version {
                    Some(0) => resp.version(Version::HTTP_10),
                    Some(1) => resp.version(Version::HTTP_11),
                    _ => resp,
                };

                for header in parser.headers {
                    resp = resp.header(header.name, header.value);
                }

                Ok(Complete((parsed, resp)))
            }
            Partial => Ok(Partial),
        }
    })?;

    let body = BodyReader::new(Box::new(reader), resp.headers_ref())?;

    resp.body(body).map_err(Error::from)
}

fn handle_redirects(
    resp: &Response<BodyReader>,
    opts: &mut Options,
    elapsed: Duration,
) -> Result<Option<Uri>, Error> {
    if let Some(redirects) = &mut opts.follow_redirects {
        match resp.status().as_u16() {
            301 | 302 | 303 | 307 | 308 => {
                if *redirects == 0 {
                    return Err(Error::TooManyRedirects);
                }

                *redirects -= 1;

                if let Some(timeout) = &mut opts.timeout {
                    if *timeout <= elapsed {
                        return Err(Error::TooManyRedirects);
                    }

                    *timeout -= elapsed;
                }

                let location: Uri = resp
                    .headers()
                    .get(LOCATION)
                    .ok_or(Error::MissingLocation)?
                    .to_str()?
                    .parse()?;

                return Ok(Some(location));
            }
            _ => (),
        }
    }

    Ok(None)
}

const DEF_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

const MAX_HEADERS: usize = 128;
const MAX_PARSE_BUF_LEN: usize = MAX_HEADERS * 1024;
