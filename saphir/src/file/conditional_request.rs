// Copyright (c) 2018 Weihang Lo
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::{
    error::SaphirError,
    file::etag::{EntityTag, SystemTimeExt},
    request::Request,
};
//use chrono::{DateTime, FixedOffset, Utc};
use hyper::Method;
use time::{OffsetDateTime, format_description::{well_known::Rfc2822, FormatItem}, macros::format_description};
use std::time::SystemTime;

const DEPRECATED_HEADER_DATE_FORMAT: &[FormatItem<'static>] = format_description!("[weekday], [day]-[month repr:short]-[year repr:last_two] [hour]:[minute]:[second] [offset_hour][offset_minute]");
const DEPRECATED_HEADER_DATE_FORMAT2: &[FormatItem<'static>] = format_description!("[weekday repr:short] [month repr:short] [day] [hour]:[minute]:[second] [year]");

/// Validate precondition of `If-Match` header.
///
/// Note that an origin server MUST use the strong comparison function when
/// comparing entity-tags for `If-Match`.
///
/// [RFC7232: If-Match](https://tools.ietf.org/html/rfc7232#section-3.1)
fn check_if_match(etag: &EntityTag, if_match: &str) -> bool {
    if_match.trim() == "*" || if_match.split(',').any(|string| etag.strong_eq(EntityTag::parse(string.trim())))
}

/// Validate precondition of `If-None-Match` header.
///
/// Note that a recipient MUST use the weak comparison function when comparing
/// entity-tags for `If-None-Match`.
///
/// [RFC7232: If-None-Match](https://tools.ietf.org/html/rfc7232#section-3.2)
fn check_if_none_match(etag: &EntityTag, if_none_match: &str) -> bool {
    if_none_match.trim() != "*" && if_none_match.split(',').all(|string| !etag.weak_eq(EntityTag::parse(string.trim())))
}

/// Validate precondition of `If-Unmodified-Since` header.
fn check_if_unmodified_since(last_modified: &SystemTime, if_unmodified_since: &SystemTime) -> bool {
    last_modified.timestamp() <= if_unmodified_since.timestamp()
}

/// Validate precondition of `If-Modified-Since` header.
fn check_if_modified_since(last_modified: &SystemTime, if_modified_since: &SystemTime) -> bool {
    !check_if_unmodified_since(last_modified, if_modified_since)
}

fn is_method_get_head(method: &Method) -> bool {
    match *method {
        Method::GET | Method::HEAD => true,
        _ => false,
    }
}

/// Indicates that conditions given in the request header evaluted to false.
/// Return true if any preconditions fail.
///
/// Note that this method is only implemented partial precedence of
/// conditions defined in [RFC7232][1] which is only related to precondition
/// (Status Code 412) but not caching response (Status Code 304). Caller must
/// handle caching responses by themselves.
///
/// [1]: https://tools.ietf.org/html/rfc7232#section-6
pub fn is_precondition_failed(req: &Request, etag: &EntityTag, last_modified: &SystemTime) -> bool {
    // 1. Evaluate If-Match
    if let Some(if_match) = req.headers().get(http::header::IF_MATCH) {
        if check_if_match(etag, if_match.to_str().unwrap_or_default()) {
            // 3. Evaluate If-None-Match
            if req.headers().get(http::header::IF_NONE_MATCH).is_some() && !is_method_get_head(req.method()) {
                return true;
            }
        } else {
            return true;
        }
    }

    // 2. Evaluate If-Unmodified-Since
    if let Some(if_unmodified_since) = req
        .headers()
        .get(http::header::IF_UNMODIFIED_SINCE)
        .and_then(|header| header.to_str().ok())
        .and_then(|s| date_from_http_str(s).ok())
        .map(|time| time.into())
    {
        if check_if_unmodified_since(last_modified, &if_unmodified_since) {
            // 3. Evaluate If-None-Match
            if req.headers().get(http::header::IF_NONE_MATCH).is_some() && !is_method_get_head(req.method()) {
                return true;
            }
        } else {
            return true;
        }
    }

    // 3. Evaluate If-None-Match
    if req.headers().get(http::header::IF_NONE_MATCH).is_some() && !is_method_get_head(req.method()) {
        return true;
    }

    false
}

/// Determine freshness of requested resource by validate `If-None-Match`
/// and `If-Modified-Since` precondition header fields containing validators.
///
/// See more on [RFC7234, 4.3.2. Handling a Received Validation Request][1].
///
/// [1]: https://tools.ietf.org/html/rfc7234#section-4.3.2
pub fn is_fresh(req: &Request, etag: &EntityTag, last_modified: &SystemTime) -> bool {
    // `If-None-Match` takes presedence over `If-Modified-Since`.
    if let Some(Ok(if_none_match)) = req.headers().get(http::header::IF_NONE_MATCH).map(|header| header.to_str()) {
        !check_if_none_match(etag, if_none_match)
    } else if let Some(since) = req
        .headers()
        .get(http::header::IF_UNMODIFIED_SINCE)
        .and_then(|header| header.to_str().ok())
        .and_then(|s| date_from_http_str(s).ok())
        .map(|time| time.into())
    {
        !check_if_modified_since(last_modified, &since)
    } else {
        false
    }
}

pub fn format_systemtime(time: SystemTime) -> String {
    OffsetDateTime::from(time).format(&Rfc2822).unwrap_or_default()
}

pub fn date_from_http_str(http: &str) -> Result<OffsetDateTime, SaphirError> {
    match OffsetDateTime::parse(http, &Rfc2822).or_else(|_| OffsetDateTime::parse(http, &DEPRECATED_HEADER_DATE_FORMAT)).or_else(|_| OffsetDateTime::parse(http, &DEPRECATED_HEADER_DATE_FORMAT2))
    {
        Ok(t) => Ok(t),
        Err(_) => Err(SaphirError::Other("Cannot parse date from header".to_owned())),
    }
}

#[cfg(test)]
mod t {
    use super::*;
    use crate::{file::etag::EntityTag, prelude::Body};
    use http::request::Builder;
    use std::time::Duration;

    mod match_none_match {
        use super::*;

        #[test]
        fn any() {
            let etag = EntityTag::Strong("".to_owned());
            assert!(check_if_match(&etag, "*"));
            assert!(!check_if_none_match(&etag, "*"));
        }

        #[test]
        fn one() {
            let etag = EntityTag::Strong("2".to_owned());
            let tags = format!(
                "{},{},{}",
                EntityTag::Strong("0".to_owned()).get_tag(),
                EntityTag::Strong("1".to_owned()).get_tag(),
                EntityTag::Strong("2".to_owned()).get_tag(),
            );
            assert!(check_if_match(&etag, &tags));
            assert!(!check_if_none_match(&etag, &tags));
        }

        #[test]
        fn none() {
            let etag = EntityTag::Strong("0".to_owned());
            let tags = EntityTag::Strong("1".to_owned()).get_tag();
            assert!(!check_if_match(&etag, &tags));
            assert!(check_if_none_match(&etag, &tags));
        }
    }

    mod modified_unmodified_since {
        use super::*;

        fn init_since() -> (SystemTime, SystemTime) {
            let now = SystemTime::now();
            (now, now)
        }

        #[test]
        fn now() {
            let (now, last_modified) = init_since();
            assert!(!check_if_modified_since(&last_modified, &now));
            assert!(check_if_unmodified_since(&last_modified, &now));
        }

        #[test]
        fn after_one_sec() {
            let (now, last_modified) = init_since();
            let modified = now + Duration::from_secs(1);
            assert!(!check_if_modified_since(&last_modified, &modified));
            assert!(check_if_unmodified_since(&last_modified, &modified));
        }

        #[test]
        fn one_sec_ago() {
            let (now, last_modified) = init_since();
            let modified = now - Duration::from_secs(1);
            assert!(check_if_modified_since(&last_modified, &modified));
            assert!(!check_if_unmodified_since(&last_modified, &modified));
        }
    }

    fn init_request() -> (Builder, EntityTag, SystemTime) {
        (
            http::request::Request::builder().method("GET"),
            EntityTag::Strong("hello".to_owned()),
            SystemTime::now(),
        )
    }

    mod fresh {
        use super::*;

        #[test]
        fn no_precondition_header_fields() {
            let (req, etag, date) = init_request();
            let req = Request::new(req.body(Body::empty()).unwrap(), None);
            assert!(!is_fresh(&req, &etag, &date));
        }

        #[test]
        fn if_none_match_precedes_if_modified_since() {
            let (req, etag, date) = init_request();
            let if_none_match = etag.get_tag();
            let if_modified_since = format_systemtime(date + Duration::from_secs(1));
            let req = Request::new(
                req.header(http::header::IF_NONE_MATCH, if_none_match)
                    .header(http::header::IF_MODIFIED_SINCE, if_modified_since)
                    .body(Body::empty())
                    .unwrap(),
                None,
            );
            assert!(is_fresh(&req, &etag, &date));
        }
    }

    mod precondition {
        use super::*;

        #[test]
        fn ok_without_any_precondition() {
            let (req, etag, date) = init_request();
            let req = Request::new(req.body(Body::empty()).unwrap(), None);
            assert!(!is_precondition_failed(&req, &etag, &date));
        }

        #[test]
        fn failed_with_if_match_not_passes() {
            let (req, etag, date) = init_request();
            let if_match = EntityTag::Strong("".to_owned()).get_tag();
            let req = Request::new(req.header(http::header::IF_MATCH, if_match).body(Body::empty()).unwrap(), None);
            assert!(is_precondition_failed(&req, &etag, &date));
        }

        #[test]
        fn with_if_match_passes_get() {
            let (req, etag, date) = init_request();
            let if_match = EntityTag::Strong("hello".to_owned()).get_tag();
            let if_none_match = EntityTag::Strong("world".to_owned()).get_tag();
            let req = Request::new(
                req.header(http::header::IF_MATCH, if_match)
                    .header(http::header::IF_NONE_MATCH, if_none_match)
                    .body(Body::empty())
                    .unwrap(),
                None,
            );
            assert!(!is_precondition_failed(&req, &etag, &date));
        }

        #[test]
        fn with_if_match_fails_post() {
            let (req, etag, date) = init_request();
            let if_match = EntityTag::Strong("hello".to_owned()).get_tag();
            let if_none_match = EntityTag::Strong("world".to_owned()).get_tag();
            let req = Request::new(
                req.method(Method::POST)
                    .header(http::header::IF_MATCH, if_match)
                    .header(http::header::IF_NONE_MATCH, if_none_match)
                    .body(Body::empty())
                    .unwrap(),
                None,
            );
            assert!(is_precondition_failed(&req, &etag, &date));
        }

        #[test]
        fn failed_with_if_unmodified_since_not_passes() {
            let (req, etag, date) = init_request();
            let if_unmodified_since = date - Duration::from_secs(1);
            let req = Request::new(
                req.header(http::header::IF_UNMODIFIED_SINCE, self::format_systemtime(if_unmodified_since))
                    .body(Body::empty())
                    .unwrap(),
                None,
            );
            assert!(is_precondition_failed(&req, &etag, &date));
        }

        #[test]
        fn with_if_unmodified_since_passes_get() {
            let (req, etag, if_unmodified_since) = init_request();
            let if_none_match = EntityTag::Strong("nonematch".to_owned()).get_tag();
            let req = Request::new(
                req.header(http::header::IF_UNMODIFIED_SINCE, self::format_systemtime(if_unmodified_since))
                    .header(http::header::IF_NONE_MATCH, if_none_match)
                    .body(Body::empty())
                    .unwrap(),
                None,
            );
            assert!(!is_precondition_failed(&req, &etag, &if_unmodified_since));
        }

        #[test]
        fn with_if_unmodified_since_fails_post() {
            let (req, etag, if_unmodified_since) = init_request();
            let if_none_match = EntityTag::Strong("nonematch".to_owned()).get_tag();
            let req = Request::new(
                req.method(Method::POST)
                    .header(http::header::IF_UNMODIFIED_SINCE, self::format_systemtime(if_unmodified_since))
                    .header(http::header::IF_NONE_MATCH, if_none_match)
                    .body(Body::empty())
                    .unwrap(),
                None,
            );
            assert!(is_precondition_failed(&req, &etag, &if_unmodified_since));
        }
    }
}
