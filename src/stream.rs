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
use std::io::{Read, Result as IoResult, Write};
use std::net::TcpStream;

#[cfg(feature = "native-tls")]
use http::uri::Scheme;
#[cfg(feature = "native-tls")]
use native_tls::{HandshakeError, TlsConnector, TlsStream};

use super::{happy_eyeballs::connect, timeout::Timeout, Error, Options};

pub enum Stream {
    Tcp(TcpStream),
    TcpWithTimeout(TcpStream, Timeout),
    #[cfg(feature = "native-tls")]
    Tls(TlsStream<TcpStream>),
    #[cfg(feature = "native-tls")]
    TlsWithTimeout(TlsStream<TcpStream>, Timeout),
}

impl Stream {
    pub fn new(
        #[cfg(feature = "native-tls")] scheme: &Scheme,
        host: &str,
        port: u16,
        opts: &Options,
    ) -> Result<Self, Error> {
        let stream = connect(host, port, &opts)?;

        match opts.timeout {
            #[cfg(feature = "native-tls")]
            None if scheme == &Scheme::HTTPS => {
                let stream = perform_handshake(stream, host, &opts.tls_connector)?;

                Ok(Self::Tls(stream))
            }
            None => Ok(Self::Tcp(stream)),
            #[cfg(feature = "native-tls")]
            Some(timeout) if scheme == &Scheme::HTTPS => {
                let timeout = Timeout::start(&stream, timeout)?;
                let stream = perform_handshake(stream, host, &opts.tls_connector)?;

                Ok(Self::TlsWithTimeout(stream, timeout))
            }
            Some(timeout) => {
                let timeout = Timeout::start(&stream, timeout)?;

                Ok(Self::TcpWithTimeout(stream, timeout))
            }
        }
    }
}

#[cfg(feature = "native-tls")]
fn perform_handshake(
    stream: TcpStream,
    host: &str,
    connector: &Option<TlsConnector>,
) -> Result<TlsStream<TcpStream>, Error> {
    let handshake = match connector {
        Some(connector) => connector.connect(host, stream),
        None => TlsConnector::new()?.connect(host, stream),
    };

    match handshake {
        Ok(stream) => Ok(stream),
        Err(HandshakeError::Failure(err)) => Err(err.into()),
        Err(HandshakeError::WouldBlock(mut stream)) => loop {
            match stream.handshake() {
                Ok(stream) => return Ok(stream),
                Err(HandshakeError::Failure(err)) => return Err(err.into()),
                Err(HandshakeError::WouldBlock(stream1)) => stream = stream1,
            }
        },
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        match self {
            Self::Tcp(stream) => stream.read(buf),
            Self::TcpWithTimeout(stream, timeout) => timeout.read(stream, buf),
            #[cfg(feature = "native-tls")]
            Self::Tls(stream) => stream.read(buf),
            #[cfg(feature = "native-tls")]
            Self::TlsWithTimeout(stream, timeout) => timeout.read(stream, buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        match self {
            Self::Tcp(stream) | Self::TcpWithTimeout(stream, _) => stream.write(buf),
            #[cfg(feature = "native-tls")]
            Self::Tls(stream) | Self::TlsWithTimeout(stream, _) => stream.write(buf),
        }
    }

    fn flush(&mut self) -> IoResult<()> {
        match self {
            Self::Tcp(stream) | Self::TcpWithTimeout(stream, _) => stream.flush(),
            #[cfg(feature = "native-tls")]
            Self::Tls(stream) | Self::TlsWithTimeout(stream, _) => stream.flush(),
        }
    }
}
