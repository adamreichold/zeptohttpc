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
use std::io::{copy, Read, Result as IoResult, Seek, SeekFrom, Write};

#[derive(Debug, Clone, Copy)]
pub enum BodyKind {
    Empty,
    KnownLength(u64),
    Chunked,
}

pub trait BodyWriter {
    fn kind(&mut self) -> IoResult<BodyKind>;
    fn write<W: Write>(&mut self, writer: W) -> IoResult<()>;
}

#[derive(Debug, Clone, Copy)]
pub struct EmptyBody;

impl BodyWriter for EmptyBody {
    fn kind(&mut self) -> IoResult<BodyKind> {
        Ok(BodyKind::Empty)
    }

    fn write<W: Write>(&mut self, _writer: W) -> IoResult<()> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct MemBody<B>(pub B);

impl<B: AsRef<[u8]>> BodyWriter for MemBody<B> {
    fn kind(&mut self) -> IoResult<BodyKind> {
        let len = self.0.as_ref().len().try_into().unwrap();
        Ok(BodyKind::KnownLength(len))
    }

    fn write<W: Write>(&mut self, mut writer: W) -> IoResult<()> {
        writer.write_all(self.0.as_ref())?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct IoBody<B>(pub B);

impl<B: Seek + Read> BodyWriter for IoBody<B> {
    fn kind(&mut self) -> IoResult<BodyKind> {
        let len = self.0.seek(SeekFrom::End(0))?;
        Ok(BodyKind::KnownLength(len))
    }

    fn write<W: Write>(&mut self, mut writer: W) -> IoResult<()> {
        self.0.seek(SeekFrom::Start(0))?;
        copy(&mut self.0, &mut writer)?;
        Ok(())
    }
}

#[cfg(feature = "flate2")]
pub mod compressed_body {
    use super::*;

    use flate2::write::GzEncoder;

    #[derive(Debug, Clone)]
    pub struct CompressedBody<B>(pub B);

    impl<B: BodyWriter> BodyWriter for CompressedBody<B> {
        fn kind(&mut self) -> IoResult<BodyKind> {
            Ok(BodyKind::Chunked)
        }

        fn write<W: Write>(&mut self, writer: W) -> IoResult<()> {
            let mut writer = GzEncoder::new(writer, Default::default());
            self.0.write(&mut writer)?;
            writer.finish()?;
            Ok(())
        }
    }
}

#[cfg(feature = "json")]
pub mod json_body {
    use super::*;

    use std::io::BufWriter;

    use serde::ser::Serialize;
    use serde_json::ser::to_writer;

    #[derive(Debug, Clone)]
    pub struct JsonBody<B>(pub B);

    impl<B: Serialize> BodyWriter for JsonBody<B> {
        fn kind(&mut self) -> IoResult<BodyKind> {
            Ok(BodyKind::Chunked)
        }

        fn write<W: Write>(&mut self, writer: W) -> IoResult<()> {
            let mut writer = BufWriter::new(writer);
            to_writer(&mut writer, &self.0)?;
            writer.flush()?;
            Ok(())
        }
    }
}
