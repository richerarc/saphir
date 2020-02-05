use regex::Regex;
use std::slice::Iter;
use http::Method;
use crate::request::Request;
use hyper::Body;
use crate::error::SaphirError;
use std::collections::HashSet;
use std::sync::atomic::AtomicU64;

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
}

impl EndpointResolver {
    pub fn new(path_str: &str, method: Method) -> Result<EndpointResolver, SaphirError> {
        let mut methods = HashSet::new();
        methods.insert(Method::OPTIONS);
        methods.insert(method);

        Ok(EndpointResolver {
            path_matcher: UriPathMatcher::new(path_str).map_err(|e| SaphirError::Other(e))?,
            methods,
            id: ENDPOINT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        })
    }

    pub fn add_method(&mut self, m: Method) {
        self.methods.insert(m);
    }

    pub fn resolve(&self, req: &mut Request<Body>) -> EndpointResolverResult {
        if req.current_path_match_all(&self.path_matcher) {
            if self.methods.contains(req.method()) {
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
    inner: Vec<UriPathSegmentMatcher>
}

impl UriPathMatcher {
    pub fn new(path_str: &str) -> Result<UriPathMatcher, String> {
        let mut uri_path_matcher = UriPathMatcher {
            inner: Vec::new(),
        };
        uri_path_matcher.append(path_str)?;
        Ok(uri_path_matcher)
    }

    pub fn append(&mut self, append: &str) -> Result<(), String> {
        let mut last_err = None;
        let path_segments: Vec<UriPathSegmentMatcher> = append
            .split('/')
            .filter_map(|ps: &str| {
                if ps.is_empty() {
                    return None;
                }

                match UriPathSegmentMatcher::new(ps) {
                    Ok(seg_matcher) => Some(seg_matcher),
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

    pub fn iter(&self) -> Iter<UriPathSegmentMatcher> {
        self.inner.iter()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }
}

#[derive(Debug)]
pub(crate) enum UriPathSegmentMatcher {
    Static { segment: String },
    Variable { name: Option<String> },
    Custom { name: Option<String>, segment: Regex },
}

impl UriPathSegmentMatcher {
    ///
    pub fn new(segment: &str) -> Result<UriPathSegmentMatcher, String> {
        if segment.contains('/') {
            return Err("A path segment should not contain any /".to_string());
        }

        if (segment.starts_with('{') && segment.ends_with('}')) || (segment.starts_with('<') && segment.ends_with('>')) {
            let s: Vec<&str> = segment[1..segment.len() - 1].splitn(2, "#r").collect();
            if s.len() < 1 {
                return Err("No name was provided for a variable segment".to_string());
            }

            let name = if s[0].len() <= 1 {
                None
            } else {
                Some(s[0].to_string())
            };

            let name_c = name.clone();

            s.get(1).map(|r| {
                let r = r.trim_start_matches('(').trim_end_matches(')');
                Regex::new(r).map_err(|e| e.to_string()).map(|r| UriPathSegmentMatcher::Custom { name, segment: r })
            }).unwrap_or_else(|| Ok(UriPathSegmentMatcher::Variable { name: name_c }))
        } else {
            Ok(UriPathSegmentMatcher::Static { segment: segment.to_string() })
        }
    }

    ///
    pub fn matches(&self, other: &str) -> bool {
        match self {
            UriPathSegmentMatcher::Static { segment: ref s } => s.eq(other),
            UriPathSegmentMatcher::Variable { name: ref _n } => true,
            UriPathSegmentMatcher::Custom { name: ref _n, segment: ref s } => s.is_match(other),
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            UriPathSegmentMatcher::Static { segment: ref _s } => None,
            UriPathSegmentMatcher::Variable { name: ref n } => n.as_ref().map(|s| s.as_str()),
            UriPathSegmentMatcher::Custom { name: ref n, segment: ref _s } => n.as_ref().map(|s| s.as_str()),
        }
    }
}