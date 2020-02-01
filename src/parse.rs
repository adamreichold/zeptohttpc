use std::io::{
    BufRead, Error as IoError,
    ErrorKind::{Other, UnexpectedEof},
};

use httparse::Status::{self, Complete, Partial};

use super::MAX_PARSE_BUF_LEN;

pub fn parse<R, P, T, E>(mut reader: R, parser: P) -> Result<T, E>
where
    R: BufRead,
    P: Fn(&[u8]) -> Result<Status<(usize, T)>, E>,
    E: From<IoError>,
{
    let buf = reader.fill_buf()?;
    if let Complete((parsed, val)) = parser(buf)? {
        reader.consume(parsed);
        return Ok(val);
    }

    parse_buffered(reader, parser)
}

#[cold]
fn parse_buffered<R, P, T, E>(mut reader: R, parser: P) -> Result<T, E>
where
    R: BufRead,
    P: Fn(&[u8]) -> Result<Status<(usize, T)>, E>,
    E: From<IoError>,
{
    let mut buf1 = Vec::new();
    loop {
        let buf = reader.fill_buf()?;
        if buf.is_empty() {
            return Err(IoError::from(UnexpectedEof).into());
        }
        if buf1.len() + buf.len() > MAX_PARSE_BUF_LEN {
            return Err(IoError::new(Other, "Maximum parse buffer length reached").into());
        }
        buf1.extend_from_slice(buf);

        match parser(&buf1)? {
            Complete((parsed, val)) => {
                let amt = parsed - (buf1.len() - buf.len());
                reader.consume(amt);

                return Ok(val);
            }
            Partial => {
                let amt = buf.len();
                reader.consume(amt);
            }
        }
    }
}
