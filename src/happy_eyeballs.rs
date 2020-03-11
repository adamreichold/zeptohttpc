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
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::str::FromStr;
use std::sync::mpsc::channel;
use std::thread::spawn;

use super::{Error, Options};

pub fn connect(host: &str, port: u16, opts: &Options) -> Result<TcpStream, Error> {
    let timeout = opts.connect_timeout;
    let delay = opts.connect_delay;

    let mut addrs = resolve_addrs(host, port)?;

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

fn resolve_addrs(host: &str, port: u16) -> Result<Vec<(usize, SocketAddr)>, Error> {
    if host.starts_with('[') && host.ends_with(']') {
        if let Ok(addr) = IpAddr::from_str(&host[1..host.len() - 1]) {
            return Ok(vec![(0, SocketAddr::new(addr, port))]);
        }
    }

    Ok((host, port)
        .to_socket_addrs()?
        .map(|addr| (0, addr))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn resolve_domain() {
        let addrs = resolve_addrs("localhost", 80).unwrap();

        for (_prio, addr) in addrs {
            assert!(addr.ip().is_loopback());
            assert_eq!(addr.port(), 80);
        }
    }

    #[test]
    fn resolve_ipv4_address() {
        let addrs = resolve_addrs("127.0.0.1", 80).unwrap();

        assert_eq!(
            addrs,
            vec![(0, SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 80))]
        );
    }

    #[test]
    fn resolve_ipv6_address() {
        let addrs = resolve_addrs("[::1]", 80).unwrap();

        assert_eq!(
            addrs,
            vec![(
                0,
                SocketAddr::new(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1).into(), 80)
            )]
        );
    }
}
