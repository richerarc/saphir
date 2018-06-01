#![allow(dead_code)]

pub enum RequestContinuation {
    Next,
    None
}

pub trait ToRegex {
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

#[macro_export]
macro_rules! hset {
    ($($x:expr),*) => {
        {
            #[allow(unused_mut)]
            let mut hs = ::std::collections::HashSet::new();
            {
                $(hs.insert($x);)*
            }
            hs
        }
    };
}

#[macro_export]
macro_rules! scrypt {
    ($pw:expr) => {
        ::ring_pwhash::scrypt::scrypt_simple($pw, &::ring_pwhash::scrypt::ScryptParams::new(10, 8, 1)).expect("Invalid SCrypt input, this should never happen")
    };
}

#[macro_export]
macro_rules! scrypt_check {
    ($pw:expr,$hs:expr) => {
        ::ring_pwhash::scrypt::scrypt_check($pw, $hs).unwrap_or(false)
    };
}