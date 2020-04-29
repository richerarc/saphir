// Copyright (c) 2014-2018 Sean McArthur
// Copyright (c) 2018 The hyperx Contributors
// license: https://github.com/dekellum/hyperx/blob/master/LICENSE
// source: https://github.com/dekellum/hyperx/blob/master/src/header/common/content_range.rs

use crate::error::SaphirError;
use std::str::FromStr;

/// Content-Range, described in [RFC7233](https://tools.ietf.org/html/rfc7233#section-4.2)
///
/// # ABNF
///
/// ```text
/// Content-Range       = byte-content-range
///                     / other-content-range
///
/// byte-content-range  = bytes-unit SP
///                       ( byte-range-resp / unsatisfied-range )
///
/// byte-range-resp     = byte-range "/" ( complete-length / "*" )
/// byte-range          = first-byte-pos "-" last-byte-pos
/// unsatisfied-range   = "*/" complete-length
///
/// complete-length     = 1*DIGIT
///
/// other-content-range = other-range-unit SP other-range-resp
/// other-range-resp    = *CHAR
/// ```
#[derive(PartialEq, Clone, Debug)]
pub enum ContentRange {
    /// Byte range
    Bytes {
        /// First and last bytes of the range, omitted if request could not be
        /// satisfied
        range: Option<(u64, u64)>,

        /// Total length of the instance, can be omitted if unknown
        instance_length: Option<u64>,
    },

    /// Custom range, with unit not registered at IANA
    Unregistered {
        /// other-range-unit
        unit: String,

        /// other-range-resp
        resp: String,
    },
}

fn split_in_two(s: &str, separator: char) -> Option<(&str, &str)> {
    let mut iter = s.splitn(2, separator);
    match (iter.next(), iter.next()) {
        (Some(a), Some(b)) => Some((a, b)),
        _ => None,
    }
}

impl FromStr for ContentRange {
    type Err = SaphirError;

    fn from_str(s: &str) -> Result<Self, SaphirError> {
        let res = match split_in_two(s, ' ') {
            Some(("bytes", resp)) => {
                let (range, instance_length) = split_in_two(resp, '/').ok_or(SaphirError::Other("Could not parse Content-Range".to_owned()))?;

                let instance_length = if instance_length == "*" {
                    None
                } else {
                    Some(
                        instance_length
                            .parse()
                            .map_err(|_| SaphirError::Other("Could not parse Content-Range".to_owned()))?,
                    )
                };

                let range = if range == "*" {
                    None
                } else {
                    let (first_byte, last_byte) = split_in_two(range, '-').ok_or(SaphirError::Other("Could not parse bytes in range".to_owned()))?;
                    let first_byte = first_byte.parse().map_err(|_| SaphirError::Other("Could not parse byte in range".to_owned()))?;
                    let last_byte = last_byte.parse().map_err(|_| SaphirError::Other("Could not parse byte in range".to_owned()))?;
                    if last_byte < first_byte {
                        return Err(SaphirError::Other("Byte order incorrect".to_owned()));
                    }
                    Some((first_byte, last_byte))
                };

                ContentRange::Bytes {
                    range,
                    instance_length,
                }
            }
            Some((unit, resp)) => ContentRange::Unregistered {
                unit: unit.to_owned(),
                resp: resp.to_owned(),
            },
            _ => return Err(SaphirError::Other("Range missing or incomplete".to_owned())),
        };
        Ok(res)
    }
}

impl ToString for ContentRange {
    fn to_string(&self) -> String {
        match *self {
            ContentRange::Bytes { range, instance_length } => {
                let mut string = "bytes ".to_owned();
                match range {
                    Some((first_byte, last_byte)) => {
                        string.push_str(format!("{}-{}", first_byte, last_byte).as_str());
                    }
                    None => {
                        string.push('*');
                    }
                };
                string.push('/');
                if let Some(v) = instance_length {
                    string.push_str(v.to_string().as_str());
                } else {
                    string.push('*')
                }

                string
            }
            ContentRange::Unregistered { ref unit, ref resp } => format!("{} {}", unit, resp),
        }
    }
}
