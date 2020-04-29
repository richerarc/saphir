// Copyright (c) 2018 Weihang Lo
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::{
    file::{
        content_range::ContentRange,
        etag::{EntityTag, SystemTimeExt},
        range::Range,
    },
    request::Request,
};
use chrono::{DateTime, Utc};
use std::time::SystemTime;

/// Check if given value from `If-Range` header field is fresh.
///
/// According to RFC7232, to validate `If-Range` header, the implementation
/// must use a strong comparison.
pub fn is_range_fresh(req: &Request, etag: &EntityTag, last_modified: &SystemTime) -> bool {
    // Ignore `If-Range` if `Range` header is not present.
    if !req.headers().contains_key(http::header::RANGE) {
        return false;
    }
    if let Some(if_range) = req.headers().get(http::header::IF_RANGE).and_then(|header| header.to_str().ok()) {
        if if_range.starts_with('"') || if_range.starts_with("W/\"") {
            return etag.strong_eq(EntityTag::parse(if_range));
        }

        if let Ok(date) = DateTime::parse_from_rfc2822(if_range).map(DateTime::<Utc>::from) {
            return last_modified.timestamp() == SystemTime::from(date).timestamp();
        }
    }
    // Always be fresh if there is no validators
    true
}

/// Convert `Range` header field in incoming request to `Content-Range` header
/// field for response.
///
/// Here are all situations mapped to returning `Option`:
///
/// - None byte-range -> None
/// - One satisfiable byte-range -> Some
/// - One not satisfiable byte-range -> None
/// - Two or more byte-ranges -> None
///
/// Note that invalid and multiple byte-range are treaded as an unsatisfiable
/// range.
pub fn is_satisfiable_range(range: &Range, instance_length: u64) -> Option<ContentRange> {
    match *range {
        // Try to extract byte range specs from range-unit.
        Range::Bytes(ref byte_range_specs) => Some(byte_range_specs),
        _ => None,
    }
    .and_then(|specs| if specs.len() == 1 { Some(specs[0].to_owned()) } else { None })
    .and_then(|spec| spec.to_satisfiable_range(instance_length))
    .map(|range| ContentRange::Bytes {
        range: Some(range),
        instance_length: Some(instance_length),
    })
}

/// Extract range from `ContentRange` header field.
pub fn extract_range(content_range: &ContentRange) -> Option<(u64, u64)> {
    match *content_range {
        ContentRange::Bytes { range, .. } => range,
        _ => None,
    }
}

#[cfg(test)]
mod t_range {
    use super::*;
    use crate::{
        file::{conditional_request::format_systemtime, etag::EntityTag},
        prelude::Body,
    };
    use http::{request::Builder, Method};
    use std::time::{Duration, SystemTime};

    #[test]
    fn no_range_header() {
        // Ignore range freshness validation. Return ture.
        let req = init_request();
        let last_modified = SystemTime::now();
        let etag = EntityTag::Strong("".to_owned());

        assert!(!is_range_fresh(
            &Request::new(req.header(http::header::IF_RANGE, etag.get_tag()).body(Body::empty()).unwrap(), None),
            &etag,
            &last_modified
        ));
    }

    #[test]
    fn no_if_range_header() {
        // Ignore if-range freshness validation. Return true.
        let req = init_request();
        let range = Range::Bytes(vec![]);
        let last_modified = SystemTime::now();
        let etag = EntityTag::Strong("".to_owned());
        // Always be fresh if there is no validators
        assert!(is_range_fresh(
            &Request::new(req.header(http::header::RANGE, range.to_string()).body(Body::empty()).unwrap(), None),
            &etag,
            &last_modified
        ));
    }

    #[test]
    fn weak_validator_as_falsy() {
        let req = init_request();
        let range = Range::Bytes(vec![]);

        let last_modified = SystemTime::now();
        let etag = EntityTag::Weak("im_weak".to_owned());
        assert!(!is_range_fresh(
            &Request::new(
                req.header(http::header::IF_RANGE, etag.get_tag())
                    .header(http::header::RANGE, range.to_string())
                    .body(Body::empty())
                    .unwrap(),
                None
            ),
            &etag,
            &last_modified
        ));
    }

    #[test]
    fn only_accept_exact_match_mtime() {
        let mut req = init_request();
        let etag = EntityTag::Strong("".to_owned());
        let date = SystemTime::now();

        req = req.header(http::header::RANGE, Range::Bytes(vec![]).to_string());

        // Same date.
        assert!(is_range_fresh(
            &Request::new(req.header(http::header::IF_RANGE, format_systemtime(date)).body(Body::empty()).unwrap(), None),
            &etag,
            &date
        ));

        req = init_request();
        req = req.header(http::header::RANGE, Range::Bytes(vec![]).to_string());

        // Before 10 sec.
        let past = date - Duration::from_secs(10);
        assert!(!is_range_fresh(
            &Request::new(req.header(http::header::IF_RANGE, format_systemtime(past)).body(Body::empty()).unwrap(), None),
            &etag,
            &date
        ));

        req = init_request();
        req = req.header(http::header::RANGE, Range::Bytes(vec![]).to_string());

        // After 10 sec.
        let future = date + Duration::from_secs(10);
        assert!(!is_range_fresh(
            &Request::new(req.header(http::header::IF_RANGE, format_systemtime(future)).body(Body::empty()).unwrap(), None),
            &etag,
            &date
        ));
    }

    #[test]
    fn strong_validator() {
        let mut req = init_request();
        req = req.header(http::header::RANGE, Range::Bytes(vec![]).to_string());

        let last_modified = SystemTime::now();
        let etag = EntityTag::Strong("im_strong".to_owned());
        req = req.header(http::header::IF_RANGE, etag.get_tag());
        let req = Request::new(req.body(Body::empty()).unwrap(), None);
        assert!(is_range_fresh(&req, &etag, &last_modified));
    }

    fn init_request() -> Builder {
        Builder::new().method(Method::GET)
    }
}

#[cfg(test)]
mod t_satisfiable {
    use super::*;

    #[test]
    fn zero_byte_range() {
        let range = &Range::Unregistered("".to_owned(), "".to_owned());
        assert!(is_satisfiable_range(range, 10).is_none());
    }

    #[test]
    fn one_satisfiable_byte_range() {
        let range = &Range::bytes(0, 10);
        assert!(is_satisfiable_range(range, 10).is_some());
    }

    #[test]
    fn one_unsatisfiable_byte_range() {
        let range = &Range::bytes(20, 10);
        assert!(is_satisfiable_range(range, 10).is_none());
    }

    #[test]
    fn multiple_byte_ranges() {
        let range = &Range::bytes_multi(vec![(0, 5), (5, 6)]);
        assert!(is_satisfiable_range(range, 10).is_none());
    }
}
