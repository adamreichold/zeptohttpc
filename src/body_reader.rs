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
use std::io::{BufRead, Read, Result as IoResult};

use http::header::{HeaderMap, HeaderValue, ToStrError, TRANSFER_ENCODING};

use super::{chunked::ChunkedReader, Error};

pub struct BodyReader(Box<dyn BufRead + Send>);

impl BodyReader {
    pub(crate) fn new(
        mut reader: Box<dyn BufRead + Send>,
        headers: Option<&HeaderMap>,
    ) -> Result<Self, Error> {
        if let Some(headers) = headers {
            reader = chunked_reader(reader, headers)?;
            reader = compressed_reader(reader, headers)?;
            reader = encoded_reader(reader, headers)?;
        }

        Ok(Self(reader))
    }
}

impl BufRead for BodyReader {
    fn fill_buf(&mut self) -> IoResult<&[u8]> {
        self.0.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.0.consume(amt);
    }
}

impl Read for BodyReader {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        self.0.read(buf)
    }
}

fn chunked_reader(
    mut reader: Box<dyn BufRead + Send>,
    headers: &HeaderMap,
) -> Result<Box<dyn BufRead + Send>, Error> {
    if let Some(encodings) = headers.get(TRANSFER_ENCODING) {
        for encoding in split_encodings(encodings)? {
            if encoding == "chunked" {
                reader = Box::new(ChunkedReader::new(reader));
            }
        }
    }

    Ok(reader)
}

#[cfg(feature = "flate2")]
fn compressed_reader(
    mut reader: Box<dyn BufRead + Send>,
    headers: &HeaderMap,
) -> Result<Box<dyn BufRead + Send>, Error> {
    use std::io::BufReader;

    use flate2::bufread::{GzDecoder, ZlibDecoder};
    use http::header::CONTENT_ENCODING;

    fn deflate_reader(reader: Box<dyn BufRead + Send>) -> Box<dyn BufRead + Send> {
        Box::new(BufReader::new(ZlibDecoder::new(reader)))
    }

    fn gzip_reader(reader: Box<dyn BufRead + Send>) -> Box<dyn BufRead + Send> {
        Box::new(BufReader::new(GzDecoder::new(reader)))
    }

    if let Some(encodings) = headers.get(CONTENT_ENCODING) {
        for encoding in split_encodings(encodings)? {
            reader = match encoding.as_str() {
                "deflate" => deflate_reader(reader),
                "gzip" => gzip_reader(reader),
                _ => reader,
            };
        }
    }

    Ok(reader)
}

#[cfg(not(feature = "flate2"))]
#[allow(clippy::unnecessary_wraps)]
fn compressed_reader(
    reader: Box<dyn BufRead + Send>,
    _headers: &HeaderMap,
) -> Result<Box<dyn BufRead + Send>, Error> {
    Ok(reader)
}

#[cfg(feature = "encoding_rs")]
fn encoded_reader(
    mut reader: Box<dyn BufRead + Send>,
    headers: &HeaderMap,
) -> Result<Box<dyn BufRead + Send>, Error> {
    use encoding_rs::Encoding;
    use http::header::CONTENT_TYPE;

    use super::encoded::EncodedReader;

    if let Some(type_) = headers.get(CONTENT_TYPE) {
        #[allow(clippy::manual_split_once)]
        if let Some(charset) = type_.to_str()?.splitn(2, "charset=").nth(1) {
            if let Some(encoding) = Encoding::for_label(charset.as_bytes()) {
                reader = Box::new(EncodedReader::new(reader, encoding));
            }
        }
    }

    Ok(reader)
}

#[cfg(not(feature = "encoding_rs"))]
#[allow(clippy::unnecessary_wraps)]
fn encoded_reader(
    reader: Box<dyn BufRead + Send>,
    _headers: &HeaderMap,
) -> Result<Box<dyn BufRead + Send>, Error> {
    Ok(reader)
}

fn split_encodings(
    encodings: &HeaderValue,
) -> Result<impl Iterator<Item = String> + '_, ToStrError> {
    encodings.to_str().map(|encodings| {
        encodings
            .split(',')
            .map(str::trim)
            .map(str::to_ascii_lowercase)
    })
}
