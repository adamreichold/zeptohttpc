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
use rustls::{ClientConfig, ClientSession, Session, StreamOwned};
#[cfg(feature = "tls")]
use webpki::DNSNameRef;
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
    Rustls(Box<StreamOwned<ClientSession, TcpStream>>),
    #[cfg(feature = "tls")]
    RustlsWithTimeout(Box<StreamOwned<ClientSession, TcpStream>>, Timeout),
}

impl Stream {
    pub fn new(
        #[cfg(any(feature = "native-tls", feature = "tls"))] scheme: &Scheme,
        host: &str,
        port: u16,
        opts: &Options,
    ) -> Result<Self, Error> {
        let stream = connect(host, port, &opts)?;

        match opts.timeout {
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
            Some(timeout) if scheme == &Scheme::HTTPS => {
                let timeout = Timeout::start(&stream, timeout)?;
                let stream = perform_native_tls_handshake(stream, host, opts.tls_connector)?;

                Ok(Self::NativeTlsWithTimeout(stream, timeout))
            }
            #[cfg(feature = "tls")]
            Some(timeout) if scheme == &Scheme::HTTPS => {
                let timeout = Timeout::start(&stream, timeout)?;
                let stream = perform_rustls_handshake(stream, host, opts.client_config)?;

                Ok(Self::RustlsWithTimeout(Box::new(stream), timeout))
            }
            Some(timeout) => {
                let timeout = Timeout::start(&stream, timeout)?;

                Ok(Self::TcpWithTimeout(stream, timeout))
            }
        }
    }

    pub fn cork(&self, val: bool) -> IoResult<()> {
        match self {
            Self::Tcp(stream) | Self::TcpWithTimeout(stream, _) => cork(stream, val),
            #[cfg(feature = "native-tls")]
            Self::NativeTls(stream) | Self::NativeTlsWithTimeout(stream, _) => {
                cork(stream.get_ref(), val)
            }
            #[cfg(feature = "tls")]
            Self::Rustls(stream) | Self::RustlsWithTimeout(stream, _) => cork(&stream.sock, val),
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
) -> Result<StreamOwned<ClientSession, TcpStream>, Error> {
    let name = DNSNameRef::try_from_ascii_str(host)?;

    let mut session = match client_config {
        Some(client_config) => ClientSession::new(client_config, name),
        None => {
            let mut client_config = ClientConfig::new();

            client_config
                .root_store
                .add_server_trust_anchors(&TLS_SERVER_ROOTS);

            ClientSession::new(&Arc::new(client_config), name)
        }
    };

    while let Err(err) = session.complete_io(&mut stream) {
        if err.kind() != WouldBlock || !session.is_handshaking() {
            return Err(err.into());
        }
    }

    Ok(StreamOwned::new(session, stream))
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
    stream: &mut StreamOwned<ClientSession, TcpStream>,
) -> IoResult<usize> {
    match res {
        Err(err) if err.kind() == ConnectionAborted => {
            stream.sess.send_close_notify();
            stream.sess.complete_io(&mut stream.sock)?;

            Ok(0)
        }
        res => res,
    }
}

#[cfg(target_os = "linux")]
fn cork(stream: &TcpStream, val: bool) -> IoResult<()> {
    use std::io::Error as IoError;
    use std::mem::size_of;
    use std::os::unix::io::AsRawFd;

    use libc::{c_int, setsockopt, IPPROTO_TCP, TCP_CORK};

    let val = val as c_int;

    if unsafe {
        setsockopt(
            stream.as_raw_fd(),
            IPPROTO_TCP,
            TCP_CORK,
            &val as *const _ as _,
            size_of::<c_int>() as _,
        )
    } == 0
    {
        Ok(())
    } else {
        Err(IoError::last_os_error())
    }
}

#[cfg(not(target_os = "linux"))]
fn cork(_stream: &TcpStream, _val: bool) -> IoResult<()> {
    Ok(())
}
