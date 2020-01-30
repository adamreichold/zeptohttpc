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
use std::io::{ErrorKind::TimedOut, Read, Result as IoResult};
use std::net::{Shutdown, TcpStream};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::thread::spawn;
use std::time::Duration;

use super::Error;

pub struct Timeout(Sender<()>);

impl Timeout {
    pub fn start(stream: &TcpStream, duration: Duration) -> Result<Self, Error> {
        let stream = stream.try_clone()?;
        let (tx, rx) = channel();

        spawn(move || {
            if let Err(RecvTimeoutError::Timeout) = rx.recv_timeout(duration) {
                drop(rx);

                let _ = stream.shutdown(Shutdown::Both);
            }
        });

        Ok(Self(tx))
    }

    pub fn read<R: Read>(&self, reader: &mut R, buf: &mut [u8]) -> IoResult<usize> {
        let read = reader.read(buf)?;

        if read == 0 && !buf.is_empty() && self.0.send(()).is_err() {
            return Err(TimedOut.into());
        }

        Ok(read)
    }
}
