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
use std::convert::TryInto;
use std::io::{BufRead, Error as IoError, ErrorKind::Other, Read, Result as IoResult, Write};

use httparse::{
    parse_chunk_size, InvalidChunkSize,
    Status::{Complete, Partial},
};

use super::{parse::parse, Error};

pub struct ChunkedWriter<W>(pub W);

impl<W: Write> ChunkedWriter<W> {
    pub fn close(mut self) -> IoResult<()> {
        self.0.write_all(b"0\r\n\r\n")
    }
}

impl<W: Write> Write for ChunkedWriter<W> {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        write!(self.0, "{:x}\r\n", buf.len())?;
        self.0.write_all(buf)?;
        write!(self.0, "\r\n")?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> IoResult<()> {
        self.0.flush()
    }
}

pub struct ChunkedReader<R> {
    reader: R,
    rem: usize,
    state: State,
}

#[derive(PartialEq)]
enum State {
    Init,
    Next,
    Done,
}

impl<R> ChunkedReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            rem: 0,
            state: State::Init,
        }
    }
}

impl<R: BufRead> BufRead for ChunkedReader<R> {
    fn fill_buf(&mut self) -> IoResult<&[u8]> {
        if self.rem == 0 && self.state != State::Done {
            if self.state != State::Init {
                read_line_ending(&mut self.reader)?;
            } else {
                self.state = State::Next;
            }

            self.rem = read_chunk_size(&mut self.reader)?;

            if self.rem == 0 {
                read_line_ending(&mut self.reader)?;

                self.state = State::Done;
            }
        }

        let mut buf = self.reader.fill_buf()?;

        if buf.len() > self.rem {
            buf = &buf[..self.rem];
        }

        Ok(buf)
    }

    fn consume(&mut self, mut amt: usize) {
        if amt > self.rem {
            amt = self.rem;
        }

        self.reader.consume(amt);
        self.rem -= amt;
    }
}

impl<R: BufRead> Read for ChunkedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        let read = self.fill_buf()?.read(buf)?;
        self.consume(read);
        Ok(read)
    }
}

fn read_chunk_size<R: BufRead>(reader: R) -> IoResult<usize> {
    parse(reader, |buf| match parse_chunk_size(buf) {
        Ok(Complete((parsed, chunk_size))) => {
            let chunk_size = chunk_size.try_into().unwrap();
            Ok(Complete((parsed, chunk_size)))
        }
        Ok(Partial) => Ok(Partial),
        Err(InvalidChunkSize) => Err(IoError::new(Other, Error::InvalidChunkSize)),
    })
}

fn read_line_ending<R: BufRead>(reader: R) -> IoResult<()> {
    parse(reader, |buf| {
        if buf.starts_with(b"\r\n") {
            Ok(Complete((2, ())))
        } else if buf == b"" || buf == b"\r" {
            Ok(Partial)
        } else {
            Err(IoError::new(Other, Error::InvalidLineEnding))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::ErrorKind::UnexpectedEof;

    #[test]
    fn parse_chunks() {
        let mut buf = Vec::new();
        ChunkedReader::new(&b"3\r\nfoo\r\n3\r\nbar\r\n0\r\n\r\n"[..])
            .read_to_end(&mut buf)
            .unwrap();
        assert_eq!(b"foobar", &buf[..]);
    }

    #[test]
    fn parse_empty_chunks() {
        let mut buf = Vec::new();
        ChunkedReader::new(&b"0\r\n\r\n"[..])
            .read_to_end(&mut buf)
            .unwrap();
        assert_eq!(b"", &buf[..]);
    }

    #[test]
    fn parse_missing_line_ending() {
        let mut buf = Vec::new();
        ChunkedReader::new(&b"0\r\n"[..])
            .read_to_end(&mut buf)
            .unwrap_err();
    }

    #[test]
    fn parse_line_endings() {
        read_line_ending(&b"\r\nfoo"[..]).unwrap();

        let err = read_line_ending(&b"bar"[..]).unwrap_err();
        assert_eq!(Other, err.kind());

        let err = read_line_ending(&b"\rbaz"[..]).unwrap_err();
        assert_eq!(Other, err.kind());

        let err = read_line_ending(&b""[..]).unwrap_err();
        assert_eq!(UnexpectedEof, err.kind());

        let err = read_line_ending(&b"\r"[..]).unwrap_err();
        assert_eq!(UnexpectedEof, err.kind());
    }
}
