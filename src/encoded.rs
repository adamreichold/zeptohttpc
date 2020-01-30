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
use std::cmp::min;
use std::io::{BufRead, Read, Result as IoResult};

use encoding_rs::{CoderResult, Decoder, Encoding};

pub struct EncodedReader<R> {
    reader: R,
    decoder: Decoder,
    buf: Vec<u8>,
    pos: usize,
}

impl<R> EncodedReader<R> {
    pub fn new(reader: R, encoding: &'static Encoding) -> Self {
        Self {
            reader,
            decoder: encoding.new_decoder(),
            buf: Vec::new(),
            pos: 0,
        }
    }
}

impl<R: BufRead> BufRead for EncodedReader<R> {
    fn fill_buf(&mut self) -> IoResult<&[u8]> {
        if self.buf.len() == self.pos {
            let buf = self.reader.fill_buf()?;

            let max_buf_len = self.decoder.max_utf8_buffer_length(buf.len()).unwrap();
            self.buf.resize(max_buf_len, 0);

            let last = buf.is_empty();
            let (reason, read, written, _) = self.decoder.decode_to_utf8(buf, &mut self.buf, last);
            assert_eq!(CoderResult::InputEmpty, reason);

            self.reader.consume(read);
            self.buf.truncate(written);
            self.pos = 0;
        }

        Ok(&self.buf[self.pos..])
    }

    fn consume(&mut self, amt: usize) {
        self.pos = min(self.pos + amt, self.buf.len());
    }
}

impl<R: BufRead> Read for EncodedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        let read = self.fill_buf()?.read(buf)?;
        self.consume(read);
        Ok(read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use encoding_rs::WINDOWS_1252;

    #[test]
    fn decode_windows_1252() {
        let (buf, encoding, _) = WINDOWS_1252.encode("äé");

        let mut reader = EncodedReader::new(&*buf, encoding);

        let mut buf = String::new();
        reader.read_to_string(&mut buf).unwrap();

        assert_eq!("äé", buf);
    }
}
