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
#[cfg(feature = "tls")]
use std::convert::TryFrom;
#[cfg(feature = "tls")]
use std::io::ErrorKind::{ConnectionAborted, WouldBlock};
use std::io::{Read, Result as IoResult, Write};
use std::net::TcpStream;
#[cfg(feature = "tls")]
use std::sync::Arc;

#[cfg(any(feature = "native-tls", feature = "tls"))]
use http::uri::Scheme;
#[cfg(feature = "native-tls")]
use native_tls::{HandshakeError, TlsConnector, TlsStream};
#[cfg(feature = "tls")]
use rustls::{
    ClientConfig, ClientConnection, ConfigBuilder, Connection, RootCertStore, ServerName,
    StreamOwned,
};
#[cfg(feature = "tls")]
use webpki_roots::TLS_SERVER_ROOTS;

use super::{happy_eyeballs::connect, timeout::Timeout, Error, Options};

pub enum Stream {
    Tcp(TcpStream),
    TcpWithTimeout(TcpStream, Timeout),
    #[cfg(feature = "native-tls")]
    NativeTls(TlsStream<TcpStream>),
    #[cfg(feature = "native-tls")]
    NativeTlsWithTimeout(TlsStream<TcpStream>, Timeout),
    #[cfg(feature = "tls")]
    Rustls(Box<StreamOwned<ClientConnection, TcpStream>>),
    #[cfg(feature = "tls")]
    RustlsWithTimeout(Box<StreamOwned<ClientConnection, TcpStream>>, Timeout),
}

impl Stream {
    pub fn new(
        #[cfg(any(feature = "native-tls", feature = "tls"))] scheme: &Scheme,
        host: &str,
        port: u16,
        opts: &Options,
    ) -> Result<Self, Error> {
        let stream = connect(host, port, opts)?;

        match opts.deadline {
            #[cfg(feature = "native-tls")]
            None if scheme == &Scheme::HTTPS => {
                let stream = perform_native_tls_handshake(stream, host, opts.tls_connector)?;

                Ok(Self::NativeTls(stream))
            }
            #[cfg(feature = "tls")]
            None if scheme == &Scheme::HTTPS => {
                let stream = perform_rustls_handshake(stream, host, opts.client_config)?;

                Ok(Self::Rustls(Box::new(stream)))
            }
            None => Ok(Self::Tcp(stream)),
            #[cfg(feature = "native-tls")]
            Some(deadline) if scheme == &Scheme::HTTPS => {
                let timeout = Timeout::start(&stream, deadline)?;
                let stream = perform_native_tls_handshake(stream, host, opts.tls_connector)?;

                Ok(Self::NativeTlsWithTimeout(stream, timeout))
            }
            #[cfg(feature = "tls")]
            Some(deadline) if scheme == &Scheme::HTTPS => {
                let timeout = Timeout::start(&stream, deadline)?;
                let stream = perform_rustls_handshake(stream, host, opts.client_config)?;

                Ok(Self::RustlsWithTimeout(Box::new(stream), timeout))
            }
            Some(deadline) => {
                let timeout = Timeout::start(&stream, deadline)?;

                Ok(Self::TcpWithTimeout(stream, timeout))
            }
        }
    }
}

#[cfg(feature = "native-tls")]
fn perform_native_tls_handshake(
    stream: TcpStream,
    host: &str,
    tls_connector: Option<&TlsConnector>,
) -> Result<TlsStream<TcpStream>, Error> {
    let handshake = match tls_connector {
        Some(tls_connector) => tls_connector.connect(host, stream),
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

#[cfg(feature = "tls")]
fn perform_rustls_handshake(
    mut stream: TcpStream,
    host: &str,
    client_config: Option<&Arc<ClientConfig>>,
) -> Result<StreamOwned<ClientConnection, TcpStream>, Error> {
    let name = ServerName::try_from(host)?;

    let mut conn = match client_config {
        Some(client_config) => ClientConnection::new(client_config.clone(), name)?,
        None => {
            let mut root_store = RootCertStore::empty();
            root_store.add_server_trust_anchors(&TLS_SERVER_ROOTS);

            let client_config = ConfigBuilder::with_safe_defaults()
                .for_client()?
                .with_root_certificates(root_store, &[])
                .with_no_client_auth();

            ClientConnection::new(Arc::new(client_config), name)?
        }
    };

    while let Err(err) = conn.complete_io(&mut stream) {
        if err.kind() != WouldBlock || !conn.is_handshaking() {
            return Err(err.into());
        }
    }

    Ok(StreamOwned::new(conn, stream))
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        match self {
            Self::Tcp(stream) => stream.read(buf),
            Self::TcpWithTimeout(stream, timeout) => timeout.read(stream, buf),
            #[cfg(feature = "native-tls")]
            Self::NativeTls(stream) => stream.read(buf),
            #[cfg(feature = "native-tls")]
            Self::NativeTlsWithTimeout(stream, timeout) => timeout.read(stream, buf),
            #[cfg(feature = "tls")]
            Self::Rustls(stream) => {
                let res = stream.read(buf);
                handle_close_notify(res, stream)
            }
            #[cfg(feature = "tls")]
            Self::RustlsWithTimeout(stream, timeout) => {
                let res = timeout.read(stream, buf);
                handle_close_notify(res, stream)
            }
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        match self {
            Self::Tcp(stream) | Self::TcpWithTimeout(stream, _) => stream.write(buf),
            #[cfg(feature = "native-tls")]
            Self::NativeTls(stream) | Self::NativeTlsWithTimeout(stream, _) => stream.write(buf),
            #[cfg(feature = "tls")]
            Self::Rustls(stream) | Self::RustlsWithTimeout(stream, _) => stream.write(buf),
        }
    }

    fn flush(&mut self) -> IoResult<()> {
        match self {
            Self::Tcp(stream) | Self::TcpWithTimeout(stream, _) => stream.flush(),
            #[cfg(feature = "native-tls")]
            Self::NativeTls(stream) | Self::NativeTlsWithTimeout(stream, _) => stream.flush(),
            #[cfg(feature = "tls")]
            Self::Rustls(stream) | Self::RustlsWithTimeout(stream, _) => stream.flush(),
        }
    }
}

#[cfg(feature = "tls")]
fn handle_close_notify(
    res: IoResult<usize>,
    stream: &mut StreamOwned<ClientConnection, TcpStream>,
) -> IoResult<usize> {
    match res {
        Err(err) if err.kind() == ConnectionAborted => {
            stream.conn.send_close_notify();
            stream.conn.complete_io(&mut stream.sock)?;

            Ok(0)
        }
        res => res,
    }
}
