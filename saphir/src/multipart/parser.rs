use futures::StreamExt;
use mime::Mime;
use nom::{
    alt, call, char, cond, delimited, do_parse, error::ErrorKind, map_res, named, opt, parse_to, preceded, tag, tag_no_case, take_until, IResult, Needed,
};

use crate::multipart::{Field, FieldStream, MultipartError, ParseStream};

const BASE_BUFFER_SIZE: usize = 2048;

#[derive(Debug)]
pub enum ParseFieldError {
    Finished,
    NomError(ErrorKind),
    MissingData(usize),
    Io(std::io::Error),
    Other(String),
}

enum FieldHeader<'a> {
    Disposition((&'a str, Option<&'a str>)),
    Type(Mime),
    TransferEncoding(&'a str),
}

pub struct FieldHeaders<'a> {
    content_disposition_name: &'a str,
    content_disposition_filename: Option<&'a str>,
    content_type: Option<Mime>,
    content_transfer_encoding: Option<&'a str>,
}

impl From<std::io::Error> for ParseFieldError {
    fn from(e: std::io::Error) -> Self {
        ParseFieldError::Io(e)
    }
}

named!(mime_parser<&str, Mime>, parse_to!(Mime));
named!(until_line_end, take_until!("\r\n"));
named!(line_ending, alt!(tag!("\r\n") | tag!("\n")));

pub fn tag_boundary<'a>(tag: &'a str) -> impl Fn(&'a [u8]) -> IResult<&'a [u8], bool> {
    move |i: &'a [u8]| {
        do_parse!(
            i,
            tag!("--") >> tag!(tag) >> ending: map_res!(call!(until_line_end), std::str::from_utf8) >> line_ending >> (ending == "--")
        )
    }
}

fn parse_content_type(input: &[u8]) -> IResult<&[u8], FieldHeader> {
    do_parse!(
        input,
        mime: map_res!(map_res!(until_line_end, std::str::from_utf8), mime_parser) >> (FieldHeader::Type(mime.1))
    )
}

fn parse_content_disposition(input: &[u8]) -> IResult<&[u8], FieldHeader> {
    do_parse!(
        input,
        tag_no_case!("form-data; ")
            >> tag!("name=")
            >> name: map_res!(delimited!(char!('"'), take_until!("\""), char!('"')), std::str::from_utf8)
            >> file: opt!(tag!("; filename="))
            >> filename:
                cond!(
                    file.is_some(),
                    map_res!(delimited!(char!('"'), take_until!("\""), char!('"')), std::str::from_utf8)
                )
            >> (FieldHeader::Disposition((name, filename)))
    )
}

fn parse_transfer_encoding(input: &[u8]) -> IResult<&[u8], FieldHeader> {
    do_parse!(
        input,
        value: map_res!(call!(until_line_end), std::str::from_utf8) >> (FieldHeader::TransferEncoding(value))
    )
}

fn header(input: &[u8]) -> IResult<&[u8], Option<FieldHeader>> {
    if input.is_empty() {
        return Ok((input, None));
    }

    let (_, out) = until_line_end(input)?;

    if out.len() <= 1 {
        return Ok((input, None));
    }

    do_parse!(
        input,
        tag_no_case!("content-")
            >> field:
                alt!(
                    preceded!(tag_no_case!("type: "), call!(parse_content_type))
                        | preceded!(tag_no_case!("disposition: "), call!(parse_content_disposition))
                        | preceded!(tag_no_case!("transfer-encoding: "), call!(parse_transfer_encoding))
                )
            >> line_ending
            >> (Some(field))
    )
}

fn headers(input: &[u8]) -> IResult<&[u8], FieldHeaders> {
    let mut content_disposition = None;
    let mut content_type = None;
    let mut content_transfer_encoding = None;
    let mut input = input;
    loop {
        match header(input) {
            Ok((i, header)) => {
                input = i;
                match header {
                    Some(FieldHeader::Disposition(name)) => content_disposition = Some(name),
                    Some(FieldHeader::Type(mime)) => content_type = Some(mime),
                    Some(FieldHeader::TransferEncoding(enc)) => content_transfer_encoding = Some(enc),
                    None => {
                        input = line_ending(input)?.0;
                        break;
                    }
                }
            }
            Err(nom::Err::Error(nom::error::Error {
                input: _,
                code: ErrorKind::Tag,
            })) => {
                let (i, _) = do_parse!(input, call!(until_line_end) >> line_ending >> ())?;
                input = i;
                continue;
            }
            Err(e) => return Err(e),
        }
    }

    let (content_disposition_name, content_disposition_filename) = content_disposition.ok_or(nom::Err::Error(nom::error::Error {
        input,
        code: nom::error::ErrorKind::MapOpt,
    }))?;

    Ok((
        input,
        FieldHeaders {
            content_disposition_name,
            content_disposition_filename,
            content_type,
            content_transfer_encoding,
        },
    ))
}

fn field<'a>(input: &'a [u8], bound: &'a str) -> Result<(&'a [u8], FieldHeaders<'a>), ParseFieldError> {
    let res = do_parse!(input, finished: call!(tag_boundary(bound)) >> f: cond!(!finished, call!(headers)) >> (f));

    match res {
        Ok((i, Some(f))) => Ok((i, f)),
        Ok((_i, None)) => Err(ParseFieldError::Finished),
        Err(nom::Err::Incomplete(Needed::Size(size))) => Err(ParseFieldError::MissingData(size.get())),
        Err(nom::Err::Incomplete(Needed::Unknown)) => Err(ParseFieldError::MissingData(1024)),
        Err(nom::Err::Error(nom::error::Error { input: _, code: k })) | Err(nom::Err::Failure(nom::error::Error { input: _, code: k })) => {
            Err(ParseFieldError::NomError(k))
        }
    }
}

async fn buf_data(parse_ctx: &mut ParseStream, additional_size: usize) -> Result<bool, MultipartError> {
    if parse_ctx.exhausted {
        return if parse_ctx.buf.is_empty() { Err(MultipartError::Finished) } else { Ok(false) };
    }

    loop {
        match parse_ctx.stream.next().await.transpose()? {
            None => {
                if !parse_ctx.buf.is_empty() {
                    parse_ctx.exhausted = true;
                    break Ok(false);
                } else {
                    break Err(MultipartError::Finished);
                }
            }
            Some(b) => {
                parse_ctx.buf.extend_from_slice(b.as_ref());
            }
        }

        if parse_ctx.buf.len() >= BASE_BUFFER_SIZE + additional_size {
            break Ok(true);
        }
    }
}

async fn drain_current(stream: &mut FieldStream, boundary: &str) -> Result<(), MultipartError> {
    while !parse_next_field_chunk(stream, boundary).await?.is_empty() {}

    Ok(())
}

pub async fn parse_field(mut stream: FieldStream, boundary: &str) -> Result<Field, MultipartError> {
    drain_current(&mut stream, boundary).await?;

    let parse_ctx = stream.stream();
    let mut additional_size = 0usize;

    loop {
        if buf_data(parse_ctx, additional_size).await? {
            additional_size = 0;
        }

        let buf = &mut parse_ctx.buf;

        match field(buf.as_slice(), boundary) {
            Ok((i, f)) => {
                let name = f.content_disposition_name.to_string();
                let filename = f.content_disposition_filename.map(|s| s.to_string());
                let content_type = f.content_type.unwrap_or_else(|| mime::TEXT_PLAIN.clone());
                let content_transfer_encoding = f.content_transfer_encoding.map(|s| s.to_string());
                *buf = i.to_vec();
                return Ok(Field {
                    name,
                    filename,
                    content_type,
                    content_transfer_encoding,
                    boundary: boundary.to_string(),
                    stream: Some(stream),
                });
            }
            Err(ParseFieldError::MissingData(size)) if !parse_ctx.exhausted => {
                additional_size += size;
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    }
}

#[allow(clippy::iter_with_drain)]
pub async fn parse_next_field_chunk(stream: &mut FieldStream, boundary: &str) -> Result<Vec<u8>, MultipartError> {
    let data;
    let parse_ctx = stream.stream();
    let mut boundary = boundary.to_string();

    boundary.insert_str(0, "--");

    let boundary_len = boundary.len();

    buf_data(parse_ctx, 0).await?;

    let buf = &mut parse_ctx.buf;
    let res: IResult<&[u8], &[u8]> = take_until!(buf.as_slice(), boundary.as_str());
    match res {
        Ok((input, taken)) => {
            data = taken.to_vec();
            *buf = input.to_vec();
        }
        Err(_) => {
            if parse_ctx.exhausted {
                // FIXME: False-positive clippy; cannot into_iter() on a &mut ref.
                //        Remove #[allow(clippy::iter_with_drain)] once fixed
                data = buf.drain(0..buf.len()).collect();
            } else {
                data = buf[0..(buf.len() - boundary_len)].to_vec();
                *buf = buf[(buf.len() - boundary_len - 1)..buf.len()].to_vec();
            }
        }
    }

    Ok(data)
}

pub async fn parse_field_data(mut stream: FieldStream, boundary: &str) -> Result<Vec<u8>, MultipartError> {
    let parse_ctx = stream.stream();
    let mut additional_size = 0usize;

    let mut data = Vec::new();

    let mut boundary = boundary.to_string();

    boundary.insert_str(0, "--");

    let boundary_len = boundary.len();

    loop {
        if buf_data(parse_ctx, additional_size).await? {
            additional_size = 0;
        }

        let buf = &mut parse_ctx.buf;
        let res: IResult<&[u8], &[u8]> = take_until!(buf.as_slice(), boundary.as_str());
        match res {
            Ok((input, taken)) => {
                data.extend_from_slice(taken);
                *buf = input.to_vec();
                return Ok(data);
            }
            Err(_) => {
                if parse_ctx.exhausted {
                    data.extend_from_slice(buf.as_slice());
                    return Ok(data);
                } else {
                    data.extend_from_slice(&buf[0..(buf.len() - boundary_len)]);
                    *buf = buf[(buf.len() - boundary_len - 1)..buf.len()].to_vec();
                    additional_size += 1024
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use nom::Needed;
    use std::{num::NonZeroUsize, str::FromStr};

    use super::*;

    #[test]
    fn test_tag_boundary() {
        let boundary = "----AaB03x";
        let boundary_no_ending = &b"------AaB03x\r\nContent-Type: text/plain\r\n"[..];
        let boundary_ending = &b"------AaB03x--\r\n"[..];
        let boundary_partial = &b"------AaB"[..];
        let empty = &b""[..];

        assert_eq!(
            tag_boundary(boundary)(boundary_partial),
            Err(nom::Err::Incomplete(Needed::Size(NonZeroUsize::new(3).unwrap())))
        );
        assert_eq!(tag_boundary(boundary)(boundary_no_ending), Ok((&b"Content-Type: text/plain\r\n"[..], false)));
        assert_eq!(tag_boundary(boundary)(boundary_ending), Ok((empty, true)));
    }

    #[test]
    fn test_headers_correct() {
        let headers_dada = &b"\
        content-disposition: form-data; name=\"field1\"; filename=\"file.txt\"\r\n\
        content-type: text/plain;charset=UTF-8\r\n\
        content-transfer-encoding: quoted-printable\r\n\
        \r\n\
        this is plain text data"[..];

        let (input, field_headers) = headers(headers_dada).unwrap();

        assert_eq!(field_headers.content_transfer_encoding, Some("quoted-printable"));
        assert_eq!(field_headers.content_type, Some(Mime::from_str("text/plain;charset=UTF-8").unwrap()));
        assert_eq!(field_headers.content_disposition_name, "field1");
        assert_eq!(field_headers.content_disposition_filename, Some("file.txt"));
        assert_eq!(input, &b"this is plain text data"[..]);
    }

    #[test]
    fn test_headers_correct_with_ignored() {
        let headers_dada = &b"\
        accept-encoding: UTF-8\r\n\
        content-disposition: form-data; name=\"field1\"\r\n\
        content-type: text/plain;charset=UTF-8\r\n\
        authorization: bearer asdjasdoijeferor39tj4efsuigfe\r\n\
        content-transfer-encoding: quoted-printable\r\n\
        \r\n\
        this is plain text data"[..];

        let (_input, field_headers) = headers(headers_dada).unwrap();

        assert_eq!(field_headers.content_transfer_encoding, Some("quoted-printable"));
        assert_eq!(field_headers.content_type, Some(Mime::from_str("text/plain;charset=UTF-8").unwrap()));
        assert_eq!(field_headers.content_disposition_name, "field1");
    }

    #[test]
    fn test_partial_field() {
        let data = &b"\
        --AaB03x\r\n\
        content-disposition: form-data; name=\"empty-field\"\r\n\
        content-type: text/plain\r\n\
        \r\n\
        --AaB03x--\r\n"[..];

        let (out, field_h) = field(data, "AaB03x").unwrap();
        assert_eq!(field_h.content_disposition_name, "empty-field");
        assert_eq!(field_h.content_type, Some(Mime::from_str("text/plain").unwrap()));

        if let Err(ParseFieldError::Finished) = field(out, "AaB03x") {
        } else {
            unreachable!("This should be unreachable");
        }
    }
}
