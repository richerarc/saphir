#![allow(dead_code)]

/// Enum representing whether or not a request should continue to be processed be the server
pub enum RequestContinuation {
    /// Next
    Next,
    /// None
    None,
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
