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
#[cfg(feature = "rustls")]
use std::convert::TryInto;
#[cfg(feature = "rustls")]
use std::io::ErrorKind::{ConnectionAborted, WouldBlock};
use std::io::{Read, Result as IoResult, Write};
#[cfg(any(feature = "native-tls", feature = "rustls"))]
use std::net::TcpStream;
#[cfg(feature = "rustls")]
use std::sync::Arc;

#[cfg(any(feature = "native-tls", feature = "rustls"))]
use http::uri::Scheme;
#[cfg(feature = "native-tls")]
use native_tls::{HandshakeError, TlsConnector, TlsStream};
#[cfg(any(feature = "webpki-roots", feature = "rustls-native-certs"))]
use once_cell::sync::Lazy;
#[cfg(feature = "rustls-native-certs")]
use rustls::Certificate;
#[cfg(feature = "webpki-roots")]
use rustls::OwnedTrustAnchor;
#[cfg(any(feature = "webpki-roots", feature = "rustls-native-certs"))]
use rustls::RootCertStore;
#[cfg(feature = "rustls")]
use rustls::{ClientConfig, ClientConnection, StreamOwned};
#[cfg(feature = "rustls-native-certs")]
use rustls_native_certs::load_native_certs;
#[cfg(feature = "webpki-roots")]
use webpki_roots::TLS_SERVER_ROOTS;

use super::{happy_eyeballs::connect, timeout::Timeout, Error, Options};

pub struct Stream(Box<dyn Inner>);

trait Inner: Read + Write + Send {}

impl<S> Inner for S where S: Read + Write + Send {}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        self.0.read(buf)
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> IoResult<()> {
        self.0.flush()
    }
}

impl Stream {
    pub fn new(
        #[cfg(any(feature = "native-tls", feature = "rustls"))] scheme: &Scheme,
        host: &str,
        port: u16,
        opts: &Options,
    ) -> Result<Self, Error> {
        let stream = connect(host, port, opts)?;

        let inner: Box<dyn Inner> = match opts.deadline {
            #[cfg(feature = "native-tls")]
            None if scheme == &Scheme::HTTPS => {
                let stream = perform_native_tls_handshake(stream, host, opts.tls_connector)?;

                Box::new(stream)
            }
            #[cfg(feature = "rustls")]
            None if scheme == &Scheme::HTTPS => {
                let stream = perform_rustls_handshake(stream, host, opts.client_config)?;

                Box::new(HandleCloseNotify(stream))
            }
            None => Box::new(stream),
            #[cfg(feature = "native-tls")]
            Some(deadline) if scheme == &Scheme::HTTPS => {
                let timeout = Timeout::start(&stream, deadline)?;
                let stream = perform_native_tls_handshake(stream, host, opts.tls_connector)?;

                Box::new(WithTimeout(stream, timeout))
            }
            #[cfg(feature = "rustls")]
            Some(deadline) if scheme == &Scheme::HTTPS => {
                let timeout = Timeout::start(&stream, deadline)?;
                let stream = perform_rustls_handshake(stream, host, opts.client_config)?;

                Box::new(WithTimeout(HandleCloseNotify(stream), timeout))
            }
            Some(deadline) => {
                let timeout = Timeout::start(&stream, deadline)?;

                Box::new(WithTimeout(stream, timeout))
            }
        };

        Ok(Self(inner))
    }
}

struct WithTimeout<S>(S, Timeout);

impl<S> Read for WithTimeout<S>
where
    S: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        self.1.read(&mut self.0, buf)
    }
}

impl<S> Write for WithTimeout<S>
where
    S: Write,
{
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> IoResult<()> {
        self.0.flush()
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

#[cfg(feature = "rustls")]
fn perform_rustls_handshake(
    mut stream: TcpStream,
    host: &str,
    client_config: Option<&Arc<ClientConfig>>,
) -> Result<StreamOwned<ClientConnection, TcpStream>, Error> {
    let name = host
        .try_into()
        .map_err(|_| Error::InvalidServerName(host.to_owned()))?;

    let mut conn = match client_config {
        Some(client_config) => ClientConnection::new(client_config.clone(), name)?,
        #[cfg(any(feature = "webpki-roots", feature = "rustls-native-certs"))]
        None => {
            static CLIENT_CONFIG: Lazy<Arc<ClientConfig>> = Lazy::new(|| {
                let mut root_store = RootCertStore::empty();

                #[cfg(feature = "webpki-roots")]
                root_store.add_trust_anchors(TLS_SERVER_ROOTS.iter().map(|root| {
                    OwnedTrustAnchor::from_subject_spki_name_constraints(
                        &*root.subject,
                        &*root.subject_public_key_info,
                        root.name_constraints.as_deref(),
                    )
                }));

                #[cfg(feature = "rustls-native-certs")]
                for cert in load_native_certs().expect("Failed to load native roots") {
                    root_store.add(&Certificate(cert.0)).unwrap();
                }

                let client_config = ClientConfig::builder()
                    .with_safe_defaults()
                    .with_root_certificates(root_store)
                    .with_no_client_auth();

                Arc::new(client_config)
            });

            ClientConnection::new(CLIENT_CONFIG.clone(), name)?
        }
        #[cfg(not(any(feature = "webpki-roots", feature = "rustls-native-certs")))]
        None => return Err(Error::MissingTlsRoots),
    };

    while let Err(err) = conn.complete_io(&mut stream) {
        if err.kind() != WouldBlock || !conn.is_handshaking() {
            return Err(err.into());
        }
    }

    Ok(StreamOwned::new(conn, stream))
}

#[cfg(feature = "rustls")]
struct HandleCloseNotify(StreamOwned<ClientConnection, TcpStream>);

#[cfg(feature = "rustls")]
impl Read for HandleCloseNotify {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        let res = self.0.read(buf);

        match res {
            Err(err) if err.kind() == ConnectionAborted => {
                self.0.conn.send_close_notify();
                self.0.conn.complete_io(&mut self.0.sock)?;

                Ok(0)
            }
            res => res,
        }
    }
}

#[cfg(feature = "rustls")]
impl Write for HandleCloseNotify {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> IoResult<()> {
        self.0.flush()
    }
}
