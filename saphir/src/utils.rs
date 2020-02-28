use crate::{body::Body, error::SaphirError, request::Request};
use http::Method;
use regex::Regex;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    slice::Iter,
    str::FromStr,
    sync::atomic::AtomicU64,
};

// TODO: Add possibility to match any route like /page/<path..>/view
// this will match any route that begins with /page and ends with /view, the in between path will be saved in the capture

// TODO: Add prefix and suffix literal to match if some path segment start or end with something

static ENDPOINT_ID: AtomicU64 = AtomicU64::new(0);

pub enum EndpointResolverResult {
    InvalidPath,
    MethodNotAllowed,
    Match,
}

pub struct EndpointResolver {
    path_matcher: UriPathMatcher,
    methods: HashSet<Method>,
    id: u64,
    allow_any_method: bool,
}

impl EndpointResolver {
    pub fn new(path_str: &str, method: Method) -> Result<EndpointResolver, SaphirError> {
        let mut methods = HashSet::new();
        let allow_any_method = method.is_any();
        if !allow_any_method {
            methods.insert(method);
        }

        Ok(EndpointResolver {
            path_matcher: UriPathMatcher::new(path_str).map_err(|e| SaphirError::Other(e))?,
            methods,
            id: ENDPOINT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            allow_any_method,
        })
    }

    pub fn add_method(&mut self, m: Method) {
        if !self.allow_any_method && m.is_any() {
            self.allow_any_method = true;
        } else {
            self.methods.insert(m);
        }
    }

    pub fn resolve(&self, req: &mut Request<Body>) -> EndpointResolverResult {
        let path = req.uri().path().to_string();
        if self.path_matcher.match_all_and_capture(path, req.captures_mut()) {
            if self.allow_any_method || self.methods.contains(req.method()) {
                EndpointResolverResult::Match
            } else {
                EndpointResolverResult::MethodNotAllowed
            }
        } else {
            EndpointResolverResult::InvalidPath
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }
}

#[derive(Debug)]
pub(crate) struct UriPathMatcher {
    inner: Vec<UriPathSegmentMatcher>,
    has_multi_segment_wildcard: bool,
}

impl UriPathMatcher {
    pub fn new(path_str: &str) -> Result<UriPathMatcher, String> {
        let mut uri_path_matcher = UriPathMatcher {
            inner: Vec::new(),
            has_multi_segment_wildcard: false,
        };
        uri_path_matcher.append(path_str)?;
        Ok(uri_path_matcher)
    }

    pub fn append(&mut self, append: &str) -> Result<(), String> {
        let mut last_err = None;
        let mut multi_segment = false;
        let path_segments: Vec<UriPathSegmentMatcher> = append
            .split('/')
            .filter_map(|ps: &str| {
                if ps.is_empty() {
                    return None;
                }

                match UriPathSegmentMatcher::new(ps) {
                    Ok(seg_matcher) => {
                        if seg_matcher.is_multi_segment_wildcard() {
                            multi_segment = true
                        }

                        Some(seg_matcher)
                    }
                    Err(e) => {
                        last_err = Some(e);
                        None
                    }
                }
            })
            .collect();

        if let Some(e) = last_err {
            return Err(e);
        }

        self.inner.extend(path_segments);

        if multi_segment {
            self.has_multi_segment_wildcard = true
        }

        Ok(())
    }

    pub fn match_start(&self, path: &str) -> bool {
        let mut path_split = path.trim_start_matches('/').split('/');

        for segment in &self.inner {
            if let Some(ref s) = path_split.next() {
                if !segment.matches(s) {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }

    pub fn match_all_and_capture(&self, path: String, captures: &mut HashMap<String, String>) -> bool {
        let mut splitted_path = path.split('/').collect::<VecDeque<_>>();
        splitted_path.pop_front();
        if splitted_path.back().map(|s| s.len()).unwrap_or(0) < 1 {
            splitted_path.pop_back();
        }

        if self.len() != splitted_path.len() && !self.has_multi_segment_wildcard {
            return false;
        }

        {
            let mut splitted_path = splitted_path.iter();
            // validate path
            for seg in self.iter() {
                if let Some(&current) = splitted_path.next() {
                    if !seg.matches(current) {
                        return false;
                    } else if seg.is_multi_segment_wildcard() {
                        break;
                    }
                } else {
                    return false;
                }
            }
        }

        // Alter current path and capture path variable
        {
            for seg in self.iter() {
                if let Some(current) = splitted_path.pop_front() {
                    if let Some(name) = seg.name() {
                        captures.insert(name.to_string(), current.to_string());
                    }
                }
            }
        }

        true
    }

    #[inline]
    pub fn iter(&self) -> Iter<UriPathSegmentMatcher> {
        self.inner.iter()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }
}

#[derive(Debug)]
pub(crate) enum UriPathSegmentMatcher {
    Static { segment: String },
    Variable { name: Option<String> },
    Custom { name: Option<String>, segment: Regex },
    Wildcard { segment_only: bool },
}

impl UriPathSegmentMatcher {
    ///
    pub fn new(segment: &str) -> Result<UriPathSegmentMatcher, String> {
        if segment.contains('/') {
            return Err("A path segment should not contain any /".to_string());
        }

        if segment.starts_with('*') && segment.len() <= 2 {
            Ok(UriPathSegmentMatcher::Wildcard { segment_only: segment == "**" })
        } else if (segment.starts_with('{') && segment.ends_with('}')) || (segment.starts_with('<') && segment.ends_with('>')) {
            let s: Vec<&str> = segment[1..segment.len() - 1].splitn(2, "#r").collect();
            if s.len() < 1 {
                return Err("No name was provided for a variable segment".to_string());
            }

            let name = if s[0].starts_with('_') { None } else { Some(s[0].to_string()) };

            let name_c = name.clone();

            s.get(1)
                .map(|r| {
                    let r = r.trim_start_matches('(').trim_end_matches(')');
                    Regex::new(r)
                        .map_err(|e| e.to_string())
                        .map(|r| UriPathSegmentMatcher::Custom { name, segment: r })
                })
                .unwrap_or_else(|| Ok(UriPathSegmentMatcher::Variable { name: name_c }))
        } else {
            Ok(UriPathSegmentMatcher::Static { segment: segment.to_string() })
        }
    }

    #[inline]
    pub fn matches(&self, other: &str) -> bool {
        match self {
            UriPathSegmentMatcher::Static { segment: ref s } => s.eq(other),
            UriPathSegmentMatcher::Variable { name: ref _n } => true,
            UriPathSegmentMatcher::Custom { name: ref _n, segment: ref s } => s.is_match(other),
            UriPathSegmentMatcher::Wildcard { segment_only: _ } => true,
        }
    }

    #[inline]
    pub fn name(&self) -> Option<&str> {
        match self {
            UriPathSegmentMatcher::Static { segment: ref _s } => None,
            UriPathSegmentMatcher::Variable { name: ref n } => n.as_ref().map(|s| s.as_str()),
            UriPathSegmentMatcher::Custom { name: ref n, segment: ref _s } => n.as_ref().map(|s| s.as_str()),
            UriPathSegmentMatcher::Wildcard { segment_only: _ } => None,
        }
    }

    #[inline]
    pub fn is_multi_segment_wildcard(&self) -> bool {
        match self {
            UriPathSegmentMatcher::Wildcard { segment_only } => *segment_only,
            _ => false,
        }
    }
}

pub trait MethodExtension {
    fn any() -> Self;
    fn is_any(&self) -> bool;
}

impl MethodExtension for Method {
    /// Represent a method for which any Http method will be accepted
    #[inline]
    fn any() -> Self {
        Method::from_str("ANY").expect("This is a valid method str")
    }

    fn is_any(&self) -> bool {
        self.as_str() == "ANY"
    }
}
