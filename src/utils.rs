#![allow(dead_code)]
use std::any::Any;

/// A convenience class to contains RequestParams
#[derive(Debug)]
pub struct RequestAddonCollection {
    inner: Vec<RequestAddon>
}

impl RequestAddonCollection {
    ///
    pub fn new() -> Self {
        RequestAddonCollection {
            inner: Vec::new(),
        }
    }

    /// Retrieve a Ref of a param by its name
    pub fn get(&self, name: &str) -> Option<&RequestAddon> {
        self.inner.iter().find(|p| p.name.eq(name))
    }

    /// Retrieve a RefMut of a param by its name
    pub fn get_mut(&mut self, name: &str) -> Option<&mut RequestAddon> {
        self.inner.as_mut_slice().iter_mut().find(|p| p.name.eq(name))
    }

    /// Add a `RequestAddon` to the collection
    pub fn add(&mut self, p: RequestAddon) {
        self.inner.push(p);
    }

    /// Remove a `RequestAddon` from the collection
    pub fn remove(&mut self, name: &str) {
        if let Some((index, _)) = self.inner.iter().enumerate().find(|t| t.1.name.eq(name)) {
            self.inner.remove(index);
        }
    }
}

use std::ops::{Index, IndexMut};

impl Index<usize> for RequestAddonCollection {
    type Output = RequestAddon;

    fn index(&self, index: usize) -> &RequestAddon {
        &self.inner[index]
    }
}

impl IndexMut<usize> for RequestAddonCollection {
    fn index_mut(&mut self, index: usize) -> &mut RequestAddon {
        &mut self.inner[index]
    }
}

impl<'a> IntoIterator for &'a mut RequestAddonCollection {
    type Item = &'a mut RequestAddon;
    type IntoIter = ::std::slice::IterMut<'a, RequestAddon>;

    fn into_iter(self) -> <Self as IntoIterator>::IntoIter {
        let inner = &mut self.inner;
        inner.into_iter()
    }
}

impl IntoIterator for RequestAddonCollection {
    type Item = RequestAddon;
    type IntoIter = ::std::vec::IntoIter<RequestAddon>;

    fn into_iter(self) -> <Self as IntoIterator>::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a> IntoIterator for &'a RequestAddonCollection {
    type Item = &'a RequestAddon;
    type IntoIter = ::std::slice::Iter<'a, RequestAddon>;

    fn into_iter(self) -> <Self as IntoIterator>::IntoIter {
        let inner = &self.inner;
        inner.into_iter()
    }
}

///
#[derive(Debug)]
pub struct RequestAddon {
    ///
    name: String,
    ///
    data: Box<Any + Send>
}

impl RequestAddon {
    /// Create a new RequestParam
    pub fn new<T>(name: String, data: T) -> Self where T: 'static + Any + Send {
        RequestAddon {
            name,
            data: Box::new(data),
        }
    }

    /// Check if data is of type T
    pub fn is<T: 'static + Any + Send>(&self) -> bool {
        self.data.is::<T>()
    }

    /// Retrieve RequestParam as Ref of type T, or none if the conversion failed
    pub fn borrow_as<T: 'static + Any + Send>(&self) -> Option<&T> {
        self.data.downcast_ref::<T>()
    }

    /// Retrieve RequestParam as RefMut of type T, or none if the conversion failed
    pub fn borrow_mut_as<T: 'static + Any + Send>(&mut self) -> Option<&mut T> {
        self.data.downcast_mut::<T>()
    }

    /// Get the name of the request param
    pub fn name(&self) -> &str {
        self.name.as_ref()
    }
}

impl<S: ToString, T: 'static + Any + Send> From<(S, T)> for RequestAddon {
    fn from(tup: (S, T)) -> Self {
        let (name, data) = tup;
        RequestAddon {
            name: name.to_string(),
            data: Box::new(data),
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