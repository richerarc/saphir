use crate::{body::Body, error::SaphirError, request::Request};
use http::Method;
use regex::Regex;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    iter::FromIterator,
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
            path_matcher: UriPathMatcher::new(path_str).map_err(SaphirError::Other)?,
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
pub(crate) enum UriPathMatcher {
    Simple {
        inner: Vec<UriPathSegmentMatcher>,
    },
    Wildcard {
        start: Vec<UriPathSegmentMatcher>,
        end: VecDeque<UriPathSegmentMatcher>,
        wildcard_capture_name: Option<String>,
    },
}

impl UriPathMatcher {
    pub fn new(path_str: &str) -> Result<UriPathMatcher, String> {
        let uri_path_matcher = if path_str.contains("**") || path_str.contains("..") {
            let segments = path_str.split('/').collect::<Vec<_>>();
            let mut wildcard_capture_name = None;
            let split_at = segments
                .iter()
                .position(|seg| {
                    if seg.contains("**") || seg.contains("..") {
                        let trimmed = seg.trim_start_matches("**").trim_start_matches("..");
                        if !trimmed.is_empty() {
                            wildcard_capture_name = Some(trimmed.to_string());
                        }
                        return true;
                    }

                    false
                })
                .ok_or_else(|| "Unable to locate wildcard".to_string())?;

            let (s1, s2) = segments.split_at(split_at);

            let s2 = &s2[1..s2.len()];

            UriPathMatcher::Wildcard {
                start: Self::parse_segments(s1.iter())?,
                end: Self::parse_segments(s2.iter())?,
                wildcard_capture_name,
            }
        } else {
            UriPathMatcher::Simple {
                inner: Self::parse_segments(path_str.split('/'))?,
            }
        };

        Ok(uri_path_matcher)
    }

    fn parse_segments<C, I, A>(segments: I) -> Result<C, String>
    where
        I: Iterator<Item = A>,
        A: AsRef<str>,
        C: FromIterator<UriPathSegmentMatcher>,
    {
        let mut last_err = None;
        let inner = segments
            .filter_map(|ps| {
                if ps.as_ref().is_empty() {
                    return None;
                }

                match UriPathSegmentMatcher::new(ps.as_ref()) {
                    Ok(seg_matcher) => Some(seg_matcher),
                    Err(e) => {
                        last_err = Some(e);
                        None
                    }
                }
            })
            .collect::<C>();

        if let Some(e) = last_err {
            return Err(e);
        }

        Ok(inner)
    }

    pub fn match_non_exhaustive(&self, path: &str) -> bool {
        let mut path_split = path.trim_start_matches('/').split('/').collect();

        match self {
            UriPathMatcher::Simple { inner } => Self::match_start(inner, &mut path_split),
            UriPathMatcher::Wildcard { start, end, .. } => Self::match_start(start, &mut path_split) && Self::match_end(end, &mut path_split),
        }
    }

    fn match_start(semgents_matcher: &[UriPathSegmentMatcher], splitted_path: &mut VecDeque<&str>) -> bool {
        for segment in semgents_matcher {
            if let Some(ref s) = splitted_path.pop_front() {
                if !segment.matches(s) {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }

    fn match_end(semgents_matcher: &VecDeque<UriPathSegmentMatcher>, splitted_path: &mut VecDeque<&str>) -> bool {
        let mut s_iter = semgents_matcher.iter();
        while let Some(segment) = s_iter.next_back() {
            if let Some(ref s) = splitted_path.pop_back() {
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

        match self {
            UriPathMatcher::Simple { inner } => {
                if inner.len() != splitted_path.len() {
                    return false;
                }

                {
                    let mut splitted_path = splitted_path.iter();
                    // validate path
                    for seg in inner.iter() {
                        if let Some(&current) = splitted_path.next() {
                            if !seg.matches(current) {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    }
                }

                // Alter current path and capture path variable
                {
                    for seg in inner {
                        if let Some(current) = splitted_path.pop_front() {
                            if let Some(name) = seg.name() {
                                captures.insert(name.to_string(), current.to_string());
                            }
                        }
                    }
                }

                true
            }
            UriPathMatcher::Wildcard {
                start,
                end,
                wildcard_capture_name,
            } => {
                let mut splitted = splitted_path.clone();
                if Self::match_start(start, &mut splitted) && Self::match_end(end, &mut splitted) {
                    if let Some(name) = wildcard_capture_name {
                        let value = splitted.iter().map(|&s| format!("/{}", s)).collect();
                        captures.insert(name.clone(), value);
                    }
                } else {
                    return false;
                }

                // Alter current path and capture path variable
                {
                    for seg in start {
                        if let Some(current) = splitted_path.pop_front() {
                            if let Some(name) = seg.name() {
                                captures.insert(name.to_string(), current.to_string());
                            }
                        }
                    }

                    let mut end_iter = end.iter();
                    while let Some(seg) = end_iter.next_back() {
                        if let Some(current) = splitted_path.pop_back() {
                            if let Some(name) = seg.name() {
                                captures.insert(name.to_string(), current.to_string());
                            }
                        }
                    }
                }

                true
            }
        }
    }
}

#[derive(Debug)]
pub(crate) enum UriPathSegmentMatcher {
    Static { segment: String },
    Variable { name: Option<String> },
    Custom { name: Option<String>, segment: Regex },
    Wildcard { prefix: Option<String>, suffix: Option<String> },
}

impl UriPathSegmentMatcher {
    const SEGMENT_VARIABLE_OPENING_CHARS: &'static [char] = &['{', '<'];
    const SEGMENT_VARIABLE_CLOSING_CHARS: &'static [char] = &['}', '>'];

    ///
    pub fn new(segment: &str) -> Result<UriPathSegmentMatcher, String> {
        if segment.contains('/') {
            return Err("A path segment should not contain any /".to_string());
        }

        if segment.contains('*') {
            let mut segment_split = segment.splitn(2, '*');
            Ok(UriPathSegmentMatcher::Wildcard {
                prefix: segment_split.next().filter(|s| !s.is_empty()).map(|s| s.to_string()),
                suffix: segment_split.next().filter(|s| !s.is_empty()).map(|s| s.to_string()),
            })
        } else if segment.starts_with(Self::SEGMENT_VARIABLE_OPENING_CHARS) && segment.ends_with(Self::SEGMENT_VARIABLE_CLOSING_CHARS) {
            let s: Vec<&str> = segment[1..segment.len() - 1].splitn(2, "#r").collect();
            if s.is_empty() {
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
            UriPathSegmentMatcher::Variable { .. } => true,
            UriPathSegmentMatcher::Custom { segment: ref s, .. } => s.is_match(other),
            UriPathSegmentMatcher::Wildcard { prefix, suffix } => {
                prefix.as_ref().filter(|prefix| !other.starts_with(prefix.as_str())).is_none()
                    && suffix.as_ref().filter(|suffix| !other.ends_with(suffix.as_str())).is_none()
            }
        }
    }

    #[inline]
    pub fn name(&self) -> Option<&str> {
        match self {
            UriPathSegmentMatcher::Static { .. } => None,
            UriPathSegmentMatcher::Variable { name: ref n } => n.as_ref().map(|s| s.as_str()),
            UriPathSegmentMatcher::Custom { name: ref n, .. } => n.as_ref().map(|s| s.as_str()),
            UriPathSegmentMatcher::Wildcard { .. } => None,
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
