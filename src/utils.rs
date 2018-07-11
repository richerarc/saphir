#![allow(dead_code)]
use std::any::Any;

/// A convenience class to contains RequestParams
pub struct RequestParamCollection {
    inner: Vec<RequestParam>
}

impl RequestParamCollection {
    ///
    pub fn new() -> Self {
        RequestParamCollection {
            inner: Vec::new(),
        }
    }

    /// Retrieve a Ref of a param by its name
    pub fn get(&self, name: &str) -> Option<&RequestParam> {
        self.inner.iter().find(|p| p.name.eq(name))
    }

    /// Retrieve a RefMut of a param by its name
    pub fn get_mut(&mut self, name: &str) -> Option<&mut RequestParam> {
        self.inner.as_mut_slice().iter_mut().find(|p| p.name.eq(name))
    }

    /// Add a `RequestParam` to the collection
    pub fn add(&mut self, p: RequestParam) {
        self.inner.push(p);
    }

    /// Remove a `RequestParam` from the collection
    pub fn remove(&mut self, name: &str) {
        if let Some((index, _)) = self.inner.iter().enumerate().find(|t| t.1.name.eq(name)) {
            self.inner.remove(index);
        }
    }
}

use std::ops::{Index, IndexMut};

impl Index<usize> for RequestParamCollection  {
    type Output = RequestParam;

    fn index(&self, index: usize) -> &RequestParam {
        &self.inner[index]
    }
}

impl IndexMut<usize> for RequestParamCollection {
    fn index_mut(&mut self, index: usize) -> &mut RequestParam {
        &mut self.inner[index]
    }
}

impl<'a> IntoIterator for &'a mut RequestParamCollection {
    type Item = &'a mut RequestParam;
    type IntoIter = ::std::slice::IterMut<'a, RequestParam>;

    fn into_iter(self) -> <Self as IntoIterator>::IntoIter {
        let inner = &mut self.inner;
        inner.into_iter()
    }
}

impl IntoIterator for RequestParamCollection {
    type Item = RequestParam;
    type IntoIter = ::std::vec::IntoIter<RequestParam>;

    fn into_iter(self) -> <Self as IntoIterator>::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a> IntoIterator for &'a RequestParamCollection {
    type Item = &'a RequestParam;
    type IntoIter = ::std::slice::Iter<'a, RequestParam>;

    fn into_iter(self) -> <Self as IntoIterator>::IntoIter {
        let inner = &self.inner;
        inner.into_iter()
    }
}

///
pub struct RequestParam {
    ///
    name: String,
    ///
    data: Box<Any>
}

impl RequestParam {
    /// Create a new RequestParam
    pub fn new<T>(name: String, data: T) -> Self where T: 'static + Any {
        RequestParam {
            name,
            data: Box::new(data),
        }
    }

    /// Check if data is of type T
    pub fn is<T: 'static + Any>(&self) -> bool {
        self.data.is::<T>()
    }

    /// Retrieve RequestParam as Ref of type T, or none if the conversion failed
    pub fn borrow_as<T: 'static + Any>(&self) -> Option<&T> {
        self.data.downcast_ref::<T>()
    }

    /// Retrieve RequestParam as RefMut of type T, or none if the conversion failed
    pub fn borrow_mut_as<T: 'static + Any>(&mut self) -> Option<&mut T> {
        self.data.downcast_mut::<T>()
    }

    /// Get the name of the request param
    pub fn name(&self) -> &str {
        self.name.as_ref()
    }
}

impl<S: ToString, T: 'static + Any> From<(S, T)> for RequestParam {
    fn from(tup: (S, T)) -> Self {
        let (name, data) = tup;
        RequestParam {
            name: name.to_string(),
            data: Box::new(data),
        }
    }
}

/// Enum representing whether or not a request should continue to be processed be the server
pub enum RequestContinuation {
    /// Next
    Continue(Option<RequestParam>),
    /// None
    Stop,
}

/// Trait to convert string type to regular expressions
pub trait ToRegex {
    ///
    fn to_regex(&self) -> Result<::regex::Regex, ::regex::Error>;
}

impl<'a> ToRegex for &'a str {
    fn to_regex(&self) -> Result<::regex::Regex, ::regex::Error> {
        ::regex::Regex::new(self)
    }
}

impl ToRegex for String {
    fn to_regex(&self) -> Result<::regex::Regex, ::regex::Error> {
        ::regex::Regex::new(self.as_str())
    }
}

impl ToRegex for ::regex::Regex {
    fn to_regex(&self) -> Result<::regex::Regex, ::regex::Error> {
        Ok(self.clone())
    }
}

#[macro_export]
macro_rules! reg {
    ($str_regex:expr) => {
        $str_regex.to_regex().expect("the parameter passed to reg macro is not a legitimate regex")
    };

}