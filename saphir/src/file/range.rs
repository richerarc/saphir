// Copyright (c) 2014-2018 Sean McArthur
// Copyright (c) 2018 The hyperx Contributors
// license: https://github.com/dekellum/hyperx/blob/master/LICENSE
// source: https://github.com/dekellum/hyperx/blob/master/src/header/common/range.rs

use crate::error::SaphirError;
use std::{fmt::Display, str::FromStr};

/// `Range` header, defined in [RFC7233](https://tools.ietf.org/html/rfc7233#section-3.1)
///
/// The "Range" header field on a GET request modifies the method
/// semantics to request transfer of only one or more subranges of the
/// selected representation data, rather than the entire selected
/// representation data.
#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Range {
    /// Byte range
    Bytes(Vec<ByteRangeSpec>),
    /// Custom range, with unit not registered at IANA
    /// (`other-range-unit`: String , `other-range-set`: String)
    Unregistered(String, String),
}

/// Each `Range::Bytes` header can contain one or more `ByteRangeSpecs`.
/// Each `ByteRangeSpec` defines a range of bytes to fetch
#[derive(PartialEq, Eq, Clone, Debug)]
pub enum ByteRangeSpec {
    /// Get all bytes between x and y ("x-y")
    FromTo(u64, u64),
    /// Get all bytes starting from x ("x-")
    AllFrom(u64),
    /// Get last x bytes ("-x")
    Last(u64),
}

impl ByteRangeSpec {
    /// Given the full length of the entity, attempt to normalize the byte range
    /// into an satisfiable end-inclusive (from, to) range.
    ///
    /// The resulting range is guaranteed to be a satisfiable range within the
    /// bounds of `0 <= from <= to < full_length`.
    ///
    /// If the byte range is deemed unsatisfiable, `None` is returned.
    /// An unsatisfiable range is generally cause for a server to either reject
    /// the client request with a `416 Range Not Satisfiable` status code, or to
    /// simply ignore the range header and serve the full entity using a `200
    /// OK` status code.
    ///
    /// This function closely follows [RFC 7233][1] section 2.1.
    /// As such, it considers ranges to be satisfiable if they meet the
    /// following conditions:
    ///
    /// > If a valid byte-range-set includes at least one byte-range-spec with
    /// > a first-byte-pos that is less than the current length of the
    /// > representation, or at least one suffix-byte-range-spec with a
    /// > non-zero suffix-length, then the byte-range-set is satisfiable.
    /// > Otherwise, the byte-range-set is unsatisfiable.
    ///
    /// The function also computes remainder ranges based on the RFC:
    ///
    /// > If the last-byte-pos value is
    /// > absent, or if the value is greater than or equal to the current
    /// > length of the representation data, the byte range is interpreted as
    /// > the remainder of the representation (i.e., the server replaces the
    /// > value of last-byte-pos with a value that is one less than the current
    /// > length of the selected representation).
    ///
    /// [1]: https://tools.ietf.org/html/rfc7233
    pub fn to_satisfiable_range(&self, full_length: u64) -> Option<(u64, u64)> {
        // If the full length is zero, there is no satisfiable end-inclusive range.
        if full_length == 0 {
            return None;
        }
        match self {
            ByteRangeSpec::FromTo(from, to) => {
                if *from < full_length && *from <= *to {
                    Some((*from, ::std::cmp::min(*to, full_length - 1)))
                } else {
                    None
                }
            }
            ByteRangeSpec::AllFrom(from) => {
                if *from < full_length {
                    Some((*from, full_length - 1))
                } else {
                    None
                }
            }
            ByteRangeSpec::Last(last) => {
                if *last > 0 {
                    // From the RFC: If the selected representation is shorter
                    // than the specified suffix-length,
                    // the entire representation is used.
                    if *last > full_length {
                        Some((0, full_length - 1))
                    } else {
                        Some((full_length - *last, full_length - 1))
                    }
                } else {
                    None
                }
            }
        }
    }
}

impl Display for ByteRangeSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            ByteRangeSpec::FromTo(from, to) => write!(f, "{from}-{to}"),
            ByteRangeSpec::Last(pos) => write!(f, "-{pos}"),
            ByteRangeSpec::AllFrom(pos) => write!(f, "{pos}-"),
        }
    }
}

impl Display for Range {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Range::Bytes(ref ranges) => {
                write!(f, "bytes=")?;

                for (i, range) in ranges.iter().enumerate() {
                    if i != 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "{range}")?;
                }

                Ok(())
            }
            Range::Unregistered(ref unit, ref range_str) => write!(f, "{unit}={range_str}"),
        }
    }
}

impl FromStr for Range {
    type Err = SaphirError;

    fn from_str(s: &str) -> Result<Range, SaphirError> {
        let mut iter = s.splitn(2, '=');

        match (iter.next(), iter.next()) {
            (Some("bytes"), Some(ranges)) => {
                let ranges = from_comma_delimited(ranges);
                if ranges.is_empty() {
                    return Err(SaphirError::Other("Range is empty".to_owned()));
                }
                Ok(Range::Bytes(ranges))
            }
            (Some(unit), Some(range_str)) if !unit.is_empty() && !range_str.is_empty() => Ok(Range::Unregistered(unit.to_owned(), range_str.to_owned())),
            _ => Err(SaphirError::Other("Bad Format".to_owned())),
        }
    }
}

impl FromStr for ByteRangeSpec {
    type Err = SaphirError;

    fn from_str(s: &str) -> Result<ByteRangeSpec, SaphirError> {
        let mut parts = s.splitn(2, '-');

        match (parts.next(), parts.next()) {
            (Some(""), Some(end)) => end
                .parse()
                .map_err(|_| SaphirError::Other("Could not parse bytes".to_owned()))
                .map(ByteRangeSpec::Last),
            (Some(start), Some("")) => start
                .parse()
                .map_err(|_| SaphirError::Other("Could not parse bytes".to_owned()))
                .map(ByteRangeSpec::AllFrom),
            (Some(start), Some(end)) => match (start.parse(), end.parse()) {
                (Ok(start), Ok(end)) if start <= end => Ok(ByteRangeSpec::FromTo(start, end)),
                _ => Err(SaphirError::Other("Could not parse bytes".to_owned())),
            },
            _ => Err(SaphirError::Other("ByteRange is missing or incomplete".to_owned())),
        }
    }
}

fn from_comma_delimited<T: FromStr>(s: &str) -> Vec<T> {
    s.split(',')
        .filter_map(|x| match x.trim() {
            "" => None,
            y => Some(y),
        })
        .filter_map(|x| x.parse().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{ByteRangeSpec, Range};
    use std::str::FromStr;

    pub fn bytes(from: u64, to: u64) -> Range {
        Range::Bytes(vec![ByteRangeSpec::FromTo(from, to)])
    }

    #[test]
    fn test_parse_bytes_range_valid() {
        let r = Range::from_str("bytes=1-100").unwrap();
        let r2 = Range::from_str("bytes=1-100,-").unwrap();
        let r3 = bytes(1, 100);
        assert_eq!(r, r2);
        assert_eq!(r2, r3);

        let r = Range::from_str("bytes=1-100,200-").unwrap();
        let r2 = Range::from_str("bytes= 1-100 , 101-xxx,  200- ").unwrap();
        let r3 = Range::Bytes(vec![ByteRangeSpec::FromTo(1, 100), ByteRangeSpec::AllFrom(200)]);
        assert_eq!(r, r2);
        assert_eq!(r2, r3);

        let r = Range::from_str("bytes=1-100,-100").unwrap();
        let r2 = Range::from_str("bytes=1-100, ,,-100").unwrap();
        let r3 = Range::Bytes(vec![ByteRangeSpec::FromTo(1, 100), ByteRangeSpec::Last(100)]);
        assert_eq!(r, r2);
        assert_eq!(r2, r3);

        let r = Range::from_str("custom=1-100,-100").unwrap();
        let r2 = Range::Unregistered("custom".to_owned(), "1-100,-100".to_owned());
        assert_eq!(r, r2);
    }

    #[test]
    fn test_parse_unregistered_range_valid() {
        let r = Range::from_str("custom=1-100,-100").unwrap();
        let r2 = Range::Unregistered("custom".to_owned(), "1-100,-100".to_owned());
        assert_eq!(r, r2);

        let r = Range::from_str("custom=abcd").unwrap();
        let r2 = Range::Unregistered("custom".to_owned(), "abcd".to_owned());
        assert_eq!(r, r2);

        let r = Range::from_str("custom=xxx-yyy").unwrap();
        let r2 = Range::Unregistered("custom".to_owned(), "xxx-yyy".to_owned());
        assert_eq!(r, r2);
    }

    #[test]
    fn test_parse_invalid() {
        let r = Range::from_str("bytes=1-a,-");
        assert_eq!(r.ok(), None);

        let r = Range::from_str("bytes=1-2-3");
        assert_eq!(r.ok(), None);

        let r = Range::from_str("abc");
        assert_eq!(r.ok(), None);

        let r = Range::from_str("bytes=1-100=");
        assert_eq!(r.ok(), None);

        let r = Range::from_str("bytes=");
        assert_eq!(r.ok(), None);

        let r = Range::from_str("custom=");
        assert_eq!(r.ok(), None);

        let r = Range::from_str("=1-100");
        assert_eq!(r.ok(), None);
    }

    #[test]
    fn test_byte_range_spec_to_satisfiable_range() {
        assert_eq!(Some((0, 0)), ByteRangeSpec::FromTo(0, 0).to_satisfiable_range(3));
        assert_eq!(Some((1, 2)), ByteRangeSpec::FromTo(1, 2).to_satisfiable_range(3));
        assert_eq!(Some((1, 2)), ByteRangeSpec::FromTo(1, 5).to_satisfiable_range(3));
        assert_eq!(None, ByteRangeSpec::FromTo(3, 3).to_satisfiable_range(3));
        assert_eq!(None, ByteRangeSpec::FromTo(2, 1).to_satisfiable_range(3));
        assert_eq!(None, ByteRangeSpec::FromTo(0, 0).to_satisfiable_range(0));

        assert_eq!(Some((0, 2)), ByteRangeSpec::AllFrom(0).to_satisfiable_range(3));
        assert_eq!(Some((2, 2)), ByteRangeSpec::AllFrom(2).to_satisfiable_range(3));
        assert_eq!(None, ByteRangeSpec::AllFrom(3).to_satisfiable_range(3));
        assert_eq!(None, ByteRangeSpec::AllFrom(5).to_satisfiable_range(3));
        assert_eq!(None, ByteRangeSpec::AllFrom(0).to_satisfiable_range(0));

        assert_eq!(Some((1, 2)), ByteRangeSpec::Last(2).to_satisfiable_range(3));
        assert_eq!(Some((2, 2)), ByteRangeSpec::Last(1).to_satisfiable_range(3));
        assert_eq!(Some((0, 2)), ByteRangeSpec::Last(5).to_satisfiable_range(3));
        assert_eq!(None, ByteRangeSpec::Last(0).to_satisfiable_range(3));
        assert_eq!(None, ByteRangeSpec::Last(2).to_satisfiable_range(0));
    }
}
