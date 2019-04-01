#![allow(dead_code)]

use regex::Regex;
use std::slice::Iter;

#[derive(Debug)]
pub(crate) struct UriPathMatcher {
    inner: Vec<UriPathSegmentMatcher>
}

impl UriPathMatcher {
    pub fn new(path_str: &str) -> Result<UriPathMatcher, String> {
        let path_segment_result = path_str.split('/').filter_map(|ps: &str| {
            if ps.len() > 0 {
                Some(UriPathSegmentMatcher::new(ps))
            } else {
                None
            }
        });

        let (ok, mut err): (Vec<Result<UriPathSegmentMatcher, String>>, Vec<Result<UriPathSegmentMatcher, String>>) = path_segment_result.partition(|res| {
            res.is_ok()
        });

        if err.len() > 0 {
            return Err(err.remove(0).err().expect("This is never gonna happens"));
        }

        let inner = ok.into_iter().map(|res| res.unwrap()).collect();

        Ok(UriPathMatcher {
            inner
        })
    }

    pub fn append(&mut self, append: &str) -> Result<(), String> {
        let path_segment_result = append.split('/').filter_map(|ps: &str| {
            if ps.len() > 0 {
                Some(UriPathSegmentMatcher::new(ps))
            } else {
                None
            }
        });

        let (ok, mut err): (Vec<Result<UriPathSegmentMatcher, String>>, Vec<Result<UriPathSegmentMatcher, String>>) = path_segment_result.partition(|res| {
            res.is_ok()
        });

        if err.len() > 0 {
            return Err(err.remove(0).err().expect("This is never gonna happens"));
        }

        self.inner.extend(ok.into_iter().map(|res| res.unwrap()));

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

        if segment.starts_with('<') {
            if segment.ends_with('>') {
                let s: Vec<&str> = segment.trim_start_matches('<').trim_end_matches('>').splitn(2, "#r").collect();
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
                Err("A variable path segment should start with < & end with >".to_string())
            }
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

    pub fn is_static(&self) -> bool {
        match self {
            UriPathSegmentMatcher::Static {segment: ref _s} => true,
            _ => false
        }
    }
}

/// Enum representing whether or not a request should continue to be processed be the server
pub enum RequestContinuation {
    /// Next
    Continue,
    /// None
    Stop,
}

/// Trait to convert string type to regular expressions
pub trait ToRegex {
    ///
    fn to_regex(&self) -> Result<::regex::Regex, ::regex::Error>;
    ///
    fn as_str(&self) -> &str;
}

impl<'a> ToRegex for &'a str {
    fn to_regex(&self) -> Result<::regex::Regex, ::regex::Error> {
        ::regex::Regex::new(self)
    }

    fn as_str(&self) -> &str {
        self
    }
}

impl ToRegex for String {
    fn to_regex(&self) -> Result<::regex::Regex, ::regex::Error> {
        ::regex::Regex::new(self.as_str())
    }

    fn as_str(&self) -> &str {
        &self
    }
}

impl ToRegex for ::regex::Regex {
    fn to_regex(&self) -> Result<::regex::Regex, ::regex::Error> {
        Ok(self.clone())
    }

    fn as_str(&self) -> &str {
        self.as_str()
    }
}

#[macro_export]
/// Convert a str to a regex
macro_rules! reg {
    ($str_regex:expr) => {
        $str_regex.to_regex().expect("the parameter passed to reg macro is not a legitimate regex")
    };

}