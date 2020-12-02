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

use std::io::ErrorKind;
use std::net::TcpListener;
use std::thread::{sleep, spawn};
use std::time::Duration;

use zeptohttpc::{http::Request, Error, Options, RequestBuilderExt, RequestExt};

#[test]
fn fails_due_to_timeout() {
    let listener = TcpListener::bind("localhost:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let server = spawn(move || {
        let (_stream, _peer_addr) = listener.accept().unwrap();

        sleep(Duration::from_millis(500));
    });

    let mut opts = Options::default();
    opts.timeout = Some(Duration::from_millis(100));

    let res = Request::get(format!("http://localhost:{}", port))
        .empty()
        .unwrap()
        .send_with_opts(opts);

    match res {
        Err(Error::Io(err)) => assert_eq!(ErrorKind::TimedOut, err.kind()),
        Err(err) => panic!("Unexpected error: {}", err),
        Ok(resp) => panic!("Unexpected response: {}", resp.status()),
    }

    server.join().unwrap();
}
