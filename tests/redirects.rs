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
#![allow(clippy::field_reassign_with_default)]

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::thread::{spawn, JoinHandle};

use zeptohttpc::{http::Request, Error, Options, RequestBuilderExt, RequestExt, ResponseExt};

#[test]
fn redirects_for_moved_permanently() {
    let mock = MockServer::start(vec![
        "HTTP/1.0 301 Moved Permanently\r\nLocation: {uri}\r\nContent-Length: 8\r\n\r\nnot here",
        "HTTP/1.0 200 Ok\r\nContent-Length: 10\r\n\r\nredirected",
    ]);

    let resp = Request::get(mock.uri()).empty().unwrap().send().unwrap();

    let body = resp.into_string().unwrap();
    assert_eq!("redirected", body);
}

#[test]
fn does_not_redirect_for_not_modified() {
    let mock = MockServer::start(vec![
        "HTTP/1.0 304 Not Modified\r\nLocation: {uri}\r\nContent-Length: 9\r\n\r\nunchanged",
    ]);

    let resp = Request::get(mock.uri()).empty().unwrap().send().unwrap();

    let body = resp.into_string().unwrap();
    assert_eq!("unchanged", body);
}

#[test]
fn does_not_redirect_if_explictly_disabled() {
    let mock = MockServer::start(vec![
        "HTTP/1.0 301 Moved Permanently\r\nLocation: {uri}\r\nContent-Length: 8\r\n\r\nnot here",
    ]);

    let mut opts = Options::default();
    opts.follow_redirects = None;

    let resp = Request::get(mock.uri())
        .empty()
        .unwrap()
        .send_with_opts(opts)
        .unwrap();

    let body = resp.into_string().unwrap();
    assert_eq!("not here", body);
}

#[test]
fn fails_due_to_too_many_redirects() {
    let mock = MockServer::start(vec![
        "HTTP/1.0 301 Moved Permanently\r\nLocation: {uri}\r\nContent-Length: 8\r\n\r\nnot here",
        "HTTP/1.0 301 Moved Permanently\r\nLocation: {uri}\r\nContent-Length: 8\r\n\r\nnot here",
        "HTTP/1.0 301 Moved Permanently\r\nLocation: {uri}\r\nContent-Length: 8\r\n\r\nnot here",
        "HTTP/1.0 301 Moved Permanently\r\nLocation: {uri}\r\nContent-Length: 8\r\n\r\nnot here",
    ]);

    let mut opts = Options::default();
    opts.follow_redirects = Some(3);

    let res = Request::get(mock.uri())
        .empty()
        .unwrap()
        .send_with_opts(opts);

    match res {
        Err(Error::TooManyRedirects) => (),
        Err(err) => panic!("Unexpected error: {err}"),
        Ok(resp) => panic!("Unexpected response: {}", resp.status()),
    }
}

#[test]
fn location_is_recommended_but_not_required() {
    let mock = MockServer::start(vec![
        "HTTP/1.0 301 Moved Permanently\r\nContent-Length: 8\r\n\r\nnot here",
    ]);

    let res = Request::get(mock.uri()).empty().unwrap().send();

    match res {
        Ok(resp) => assert_eq!(resp.status().as_u16(), 301),
        Err(err) => panic!("Unexpected error: {err}"),
    }
}

struct MockServer {
    port: u16,
    server: Option<JoinHandle<()>>,
}

impl MockServer {
    fn start(resps: Vec<&'static str>) -> Self {
        let listener = TcpListener::bind("localhost:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let server = spawn(move || {
            let port = port.to_string();

            for resp in resps {
                let resp = resp.replace("{uri}", &format!("http://localhost:{port}"));

                let (mut stream, _peer_addr) = listener.accept().unwrap();

                stream.write_all(resp.as_bytes()).unwrap();
                stream.shutdown(Shutdown::Write).unwrap();

                let mut buf = Vec::new();
                stream.read_to_end(&mut buf).unwrap();
            }
        });

        Self {
            port,
            server: Some(server),
        }
    }

    fn uri(&self) -> String {
        format!("http://localhost:{}", self.port)
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.server.take().unwrap().join().unwrap();
    }
}
