use crate::{
    body::Body,
    error::SaphirError,
    http_context::{HandlerMetadata, RouteId},
    request::Request,
};
use http::Method;
use regex::Regex;
use std::{
    cmp::{min, Ordering},
    collections::{HashMap, VecDeque},
    fmt::Write,
    iter::FromIterator,
    str::FromStr,
    sync::atomic::AtomicU64,
};

// TODO: Add possibility to match any route like /page/<path..>/view
// this will match any route that begins with /page and ends with /view, the in
// between path will be saved in the capture

// TODO: Add prefix and suffix literal to match if some path segment start or
// end with something

static ENDPOINT_ID: AtomicU64 = AtomicU64::new(0);

pub enum EndpointResolverResult<'a> {
    InvalidPath,
    MethodNotAllowed,
    Match(&'a HandlerMetadata),
}

#[derive(Debug, Eq, PartialEq)]
pub enum EndpointResolverMethods {
    Specific(HashMap<Method, HandlerMetadata>),
    Any(HandlerMetadata),
}

#[derive(Debug, Eq)]
pub struct EndpointResolver {
    id: u64,
    path_matcher: UriPathMatcher,
    methods: EndpointResolverMethods,
}

impl Ord for EndpointResolver {
    fn cmp(&self, other: &Self) -> Ordering {
        self.path_matcher.cmp(&other.path_matcher)
    }
}

impl PartialOrd for EndpointResolver {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for EndpointResolver {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl EndpointResolver {
    pub fn new(path_str: &str, method: Method) -> Result<EndpointResolver, SaphirError> {
        let id = ENDPOINT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let meta = HandlerMetadata {
            route_id: RouteId::new(id),
            name: None,
        };
        let methods = if method.is_any() {
            EndpointResolverMethods::Any(meta)
        } else {
            let mut methods = HashMap::new();
            methods.insert(method, meta);
            EndpointResolverMethods::Specific(methods)
        };

        Ok(EndpointResolver {
            path_matcher: UriPathMatcher::new(path_str).map_err(SaphirError::Other)?,
            methods,
            id,
        })
    }

    pub fn new_with_metadata<I: Into<Option<HandlerMetadata>>>(path_str: &str, method: Method, meta: I) -> Result<EndpointResolver, SaphirError> {
        let id = ENDPOINT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut meta = meta.into().unwrap_or_default();
        meta.route_id = RouteId::new(id);
        let methods = if method.is_any() {
            EndpointResolverMethods::Any(meta)
        } else {
            let mut methods = HashMap::new();
            methods.insert(method, meta);
            EndpointResolverMethods::Specific(methods)
        };

        Ok(EndpointResolver {
            path_matcher: UriPathMatcher::new(path_str).map_err(SaphirError::Other)?,
            methods,
            id,
        })
    }

    pub fn add_method(&mut self, m: Method) {
        match &mut self.methods {
            EndpointResolverMethods::Specific(inner) => {
                if m.is_any() {
                    panic!("Adding ANY method but an Handler already defines specific methods, This is fatal")
                }
                let meta = HandlerMetadata {
                    route_id: RouteId::new(self.id),
                    name: None,
                };
                inner.insert(m, meta);
            }
            EndpointResolverMethods::Any(_) => panic!("Adding a specific endpoint method but an Handler already defines ANY method, This is fatal"),
        }
    }

    pub fn add_method_with_metadata<I: Into<Option<HandlerMetadata>>>(&mut self, m: Method, meta: I) {
        match &mut self.methods {
            EndpointResolverMethods::Specific(inner) => {
                if m.is_any() {
                    panic!("Adding ANY method but an Handler already defines specific methods, This is fatal")
                }
                let mut meta = meta.into().unwrap_or_default();
                meta.route_id = RouteId::new(self.id);
                inner.insert(m, meta);
            }
            EndpointResolverMethods::Any(_) => panic!("Adding a specific endpoint method but an Handler already defines ANY method, This is fatal"),
        }
    }

    pub fn resolve(&self, req: &mut Request<Body>) -> EndpointResolverResult {
        let path = req.uri().path().to_string();
        if self.path_matcher.match_all_and_capture(path, req.captures_mut()) {
            match &self.methods {
                EndpointResolverMethods::Specific(methods) => {
                    if let Some(meta) = methods.get(req.method()) {
                        EndpointResolverResult::Match(meta)
                    } else {
                        EndpointResolverResult::MethodNotAllowed
                    }
                }
                EndpointResolverMethods::Any(meta) => EndpointResolverResult::Match(meta),
            }
        } else {
            EndpointResolverResult::InvalidPath
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }
}

#[derive(Debug, Eq)]
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

impl Ord for UriPathMatcher {
    fn cmp(&self, other: &Self) -> Ordering {
        let (start_self, end_self, simple_self) = match self {
            UriPathMatcher::Simple { inner } => (inner, None, true),
            UriPathMatcher::Wildcard { start, end, .. } => (start, Some(end), false),
        };
        let (start_other, end_other, simple_other) = match other {
            UriPathMatcher::Simple { inner } => (inner, None, true),
            UriPathMatcher::Wildcard { start, end, .. } => (start, Some(end), false),
        };

        let i_self = start_self.len();
        let i_other = start_other.len();
        let min_len = min(i_self, i_other);
        for i in 0..min_len {
            let cmp = start_self[i].cmp(&start_other[i]);
            if cmp != Ordering::Equal {
                return cmp;
            }
        }

        if i_self > i_other {
            for start in start_self.iter().take(i_self).skip(min_len) {
                if let UriPathSegmentMatcher::Static { .. } = start {
                    return Ordering::Less;
                }
            }
        }
        if i_other > i_self {
            for start in start_other.iter().take(i_other).skip(min_len) {
                if let UriPathSegmentMatcher::Static { .. } = start {
                    return Ordering::Greater;
                }
            }
        }

        match (end_self, end_other) {
            (Some(end_self), Some(end_other)) => {
                let j_self = end_self.len();
                let j_other = end_other.len();
                let min_len = min(j_self, j_other);
                for j in 0..min_len {
                    let cmp = end_self[j].cmp(&end_other[j]);
                    if cmp != Ordering::Equal {
                        return cmp;
                    }
                }
                j_other.cmp(&j_self)
            }
            (Some(e), None) if !e.is_empty() => Ordering::Less,
            (None, Some(e)) if !e.is_empty() => Ordering::Greater,
            _ => match (simple_self, simple_other) {
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                _ => i_other.cmp(&i_self),
            },
        }
    }
}

impl PartialOrd for UriPathMatcher {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for UriPathMatcher {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
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

    fn match_start(semgents_matcher: &[UriPathSegmentMatcher], path_segments: &mut VecDeque<&str>) -> bool {
        for segment in semgents_matcher {
            if let Some(s) = path_segments.pop_front() {
                if !segment.matches(s) {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }

    fn match_end(semgents_matcher: &VecDeque<UriPathSegmentMatcher>, path_segments: &mut VecDeque<&str>) -> bool {
        let mut s_iter = semgents_matcher.iter();
        while let Some(segment) = s_iter.next_back() {
            if let Some(s) = path_segments.pop_back() {
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
        let mut path_segments = path.split('/').collect::<VecDeque<_>>();
        path_segments.pop_front();
        if path_segments.back().map(|s| s.len()).unwrap_or(0) < 1 {
            path_segments.pop_back();
        }

        match self {
            UriPathMatcher::Simple { inner } => {
                if inner.len() != path_segments.len() {
                    return false;
                }

                {
                    let mut path_segments = path_segments.iter();
                    // validate path
                    for seg in inner.iter() {
                        if let Some(&current) = path_segments.next() {
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
                        if let Some(current) = path_segments.pop_front() {
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
                let mut segments = path_segments.clone();
                if Self::match_start(start, &mut segments) && Self::match_end(end, &mut segments) {
                    if let Some(name) = wildcard_capture_name {
                        let value = segments.iter().fold(String::new(), |mut o, &s| {
                            let _ = write!(o, "/{s}");
                            o
                        });
                        captures.insert(name.clone(), value);
                    }
                } else {
                    return false;
                }

                // Alter current path and capture path variable
                {
                    for seg in start {
                        if let Some(current) = path_segments.pop_front() {
                            if let Some(name) = seg.name() {
                                captures.insert(name.to_string(), current.to_string());
                            }
                        }
                    }

                    let mut end_iter = end.iter();
                    while let Some(seg) = end_iter.next_back() {
                        if let Some(current) = path_segments.pop_back() {
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
#[allow(clippy::large_enum_variant)]
pub(crate) enum UriPathSegmentMatcher {
    Static { segment: String },
    Variable { name: Option<String> },
    Custom { name: Option<String>, segment: Regex },
    Wildcard { prefix: Option<String>, suffix: Option<String> },
}
impl Eq for UriPathSegmentMatcher {}

impl Ord for UriPathSegmentMatcher {
    fn cmp(&self, other: &Self) -> Ordering {
        self.ord_index().cmp(&other.ord_index())
    }
}

impl PartialOrd for UriPathSegmentMatcher {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for UriPathSegmentMatcher {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl UriPathSegmentMatcher {
    const SEGMENT_VARIABLE_CLOSING_CHARS: &'static [char] = &['}', '>'];
    const SEGMENT_VARIABLE_OPENING_CHARS: &'static [char] = &['{', '<'];

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

    #[inline]
    fn ord_index(&self) -> u16 {
        match self {
            UriPathSegmentMatcher::Static { .. } => 1,
            UriPathSegmentMatcher::Variable { .. } => 3,
            UriPathSegmentMatcher::Custom { .. } => 2,
            UriPathSegmentMatcher::Wildcard { .. } => 3,
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

#[cfg(feature = "form")]
#[cfg_attr(docsrs, doc(cfg(feature = "form")))]
pub fn read_query_string_to_hashmap(query_str: &str) -> Result<HashMap<String, String>, serde_urlencoded::de::Error> {
    serde_urlencoded::from_str::<HashMap<String, String>>(query_str)
}

#[cfg(feature = "form")]
#[cfg_attr(docsrs, doc(cfg(feature = "form")))]
pub fn read_query_string_to_type<T>(query_str: &str) -> Result<T, serde_urlencoded::de::Error>
where
    T: for<'a> serde::Deserialize<'a>,
{
    serde_urlencoded::from_str::<T>(query_str)
}

#[cfg(test)]
mod tests {
    use super::{EndpointResolver, Method};
    use std::{collections::HashMap, str::FromStr};

    #[test]
    fn test_simple_endpoint_resolver_ordering() {
        let paths = vec![
            "/api/v1/users",
            "/api/v1/users/keys",
            "/api/v1/users/keys/<id>",
            "/api/v1/users/keys/first",
            "/api/v1/users/keys/**",
            "/api/v1/users/keys/**/delete",
            "/api/v1/users/**/delete",
            "/api/v1/users/<user_id>/keys",
            "/api/v1/users/<user_id>/<key_id>",
            "/api/v1/users/<user_id>",
        ];
        let mut resolvers = HashMap::new();
        let mut ids = HashMap::new();
        for path in &paths {
            let resolver = EndpointResolver::new(path, Method::from_str("GET").unwrap()).unwrap();
            ids.insert(path, resolver.id());
            resolvers.insert(path, resolver);
        }

        assert!(resolvers.get(&"/api/v1/users/keys/first") < resolvers.get(&"/api/v1/users/keys/<id>"));
        assert!(resolvers.get(&"/api/v1/users/keys/<id>") < resolvers.get(&"/api/v1/users/keys/**"));

        let mut resolvers_vec: Vec<_> = resolvers.into_values().collect();
        resolvers_vec.sort_unstable();

        assert_eq!(&resolvers_vec[0].id(), ids.get(&"/api/v1/users/keys/first").unwrap());
        assert_eq!(&resolvers_vec[1].id(), ids.get(&"/api/v1/users/keys/**/delete").unwrap());
        assert_eq!(&resolvers_vec[2].id(), ids.get(&"/api/v1/users/keys/<id>").unwrap());
        assert_eq!(&resolvers_vec[3].id(), ids.get(&"/api/v1/users/keys").unwrap());
        assert_eq!(&resolvers_vec[4].id(), ids.get(&"/api/v1/users/keys/**").unwrap());
        assert_eq!(&resolvers_vec[5].id(), ids.get(&"/api/v1/users/<user_id>/keys").unwrap());
        assert_eq!(&resolvers_vec[6].id(), ids.get(&"/api/v1/users/**/delete").unwrap());
        assert_eq!(&resolvers_vec[7].id(), ids.get(&"/api/v1/users/<user_id>/<key_id>").unwrap());
        assert_eq!(&resolvers_vec[8].id(), ids.get(&"/api/v1/users/<user_id>").unwrap());
        assert_eq!(&resolvers_vec[9].id(), ids.get(&"/api/v1/users").unwrap());
    }
}
