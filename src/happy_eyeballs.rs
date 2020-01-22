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
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc::channel;
use std::thread::spawn;

use super::{Error, Options};

pub fn connect(host: &str, port: u16, opts: &Options) -> Result<TcpStream, Error> {
    let timeout = opts.connect_timeout;
    let delay = opts.connect_delay;

    let mut addrs: Vec<_> = (host, port)
        .to_socket_addrs()?
        .map(|addr| (0, addr))
        .collect();

    if let [(_prio, addr)] = addrs.as_slice() {
        return TcpStream::connect_timeout(addr, timeout).map_err(Error::from);
    }

    addrs
        .iter_mut()
        .filter(|(_prio, addr)| addr.is_ipv6())
        .enumerate()
        .for_each(|(idx, (prio, _addr))| *prio = 2 * idx);

    addrs
        .iter_mut()
        .filter(|(_prio, addr)| addr.is_ipv4())
        .enumerate()
        .for_each(|(idx, (prio, _addr))| *prio = 2 * idx + 1);

    addrs.sort_unstable_by_key(|(prio, _addr)| *prio);

    let mut first_err = None;

    let (tx, rx) = channel();

    for (_prio, addr) in addrs {
        let tx = tx.clone();

        spawn(move || {
            let _ = tx.send(TcpStream::connect_timeout(&addr, timeout));
        });

        if let Ok(res) = rx.recv_timeout(delay) {
            match res {
                Ok(stream) => return Ok(stream),
                Err(err) => first_err = first_err.or(Some(err)),
            }
        }
    }

    drop(tx);

    for res in rx.iter() {
        match res {
            Ok(stream) => return Ok(stream),
            Err(err) => first_err = first_err.or(Some(err)),
        }
    }

    Err(first_err.unwrap().into())
}
