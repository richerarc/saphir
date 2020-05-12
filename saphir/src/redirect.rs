use crate::{body::TransmuteBody, http_context::HttpContext, responder::Responder, response::Builder as ResponseBuilder};
use http::{header::HeaderName, HeaderMap, HeaderValue, StatusCode, Uri};
use hyper::body::Body as RawBody;
use mime::Mime;
use serde::Serialize;
use std::{collections::HashMap, convert::TryInto, fmt::Debug};

#[derive(Debug)]
pub enum BuilderError {
    InvalidStatus,
    InvalidLocation,
    UnexpectedLocation,
    MissingLocation,
    InvalidQuery,
    UnexpectedQuery,
    InvalidFragment,
    UnexpectedFragment,
    InvalidFormData,
    UnexpectedFormData,
    UnexpectedContent,
    MissingContent,
    UnexpectedContentType,
    HeaderError(Box<http::Error>),
}

impl From<http::Error> for BuilderError {
    fn from(e: http::Error) -> Self {
        BuilderError::HeaderError(Box::new(e))
    }
}

#[derive(Default)]
pub struct Builder {
    status: StatusCode,
    location: Option<String>,
    query: Option<Result<String, BuilderError>>,
    fragment: Option<Result<String, BuilderError>>,
    form_data: Option<Result<HashMap<String, String>, BuilderError>>,
    content: Option<Box<dyn TransmuteBody + Send + Sync>>,
    content_type: Option<Mime>,
    extra_headers: HeaderMap<HeaderValue>,
    extra_headers_errors: Vec<http::Error>,
}

impl Builder {
    #[inline]
    pub fn location(mut self, location: &str) -> Self {
        self.location = Some(location.to_string());
        self
    }

    #[inline]
    pub fn query_param<T: Serialize>(mut self, query_param: &T) -> Self {
        self.query = Some(serde_urlencoded::to_string(query_param).map_err(|_e| BuilderError::InvalidQuery));
        self
    }

    #[inline]
    pub fn query_string(mut self, query_string: &str) -> Self {
        self.query = Some(Ok(query_string.to_string()));
        self
    }

    #[inline]
    pub fn fragment_param<T: Serialize>(mut self, fragment_param: &T) -> Self {
        self.fragment = Some(serde_urlencoded::to_string(fragment_param).map_err(|_e| BuilderError::InvalidQuery));
        self
    }

    #[inline]
    pub fn fragment_string(mut self, fragment_string: &str) -> Self {
        self.fragment = Some(Ok(fragment_string.to_string()));
        self
    }

    #[inline]
    pub fn choices<B: 'static + Into<RawBody> + Send + Sync>(mut self, content: B) -> Self {
        self.content = Some(Box::new(Some(content)));
        self
    }

    #[inline]
    pub fn header<E, K, V>(mut self, name: K, value: V) -> Self
    where
        E: Into<http::Error>,
        K: TryInto<HeaderName, Error = E>,
        V: TryInto<HeaderValue, Error = E>,
    {
        let name = match name.try_into() {
            Ok(name) => Some(name),
            Err(e) => {
                self.extra_headers_errors.push(e.into());
                None
            }
        };
        let value = match value.try_into() {
            Ok(value) => Some(value),
            Err(e) => {
                self.extra_headers_errors.push(e.into());
                None
            }
        };
        if let (Some(name), Some(value)) = (name, value) {
            self.extra_headers.insert(name, value);
        }
        self
    }

    pub fn build(mut self) -> Result<Redirect, BuilderError> {
        match self.status {
            StatusCode::MOVED_PERMANENTLY | StatusCode::PERMANENT_REDIRECT | StatusCode::FOUND | StatusCode::SEE_OTHER | StatusCode::TEMPORARY_REDIRECT => {
                self.assert_location(true)?;
                self.assert_content(false)?;
                self.assert_form_data()?;
            }
            StatusCode::NOT_MODIFIED => {
                self.assert_location(false)?;
                self.assert_query()?;
                self.assert_fragment()?;
                self.assert_content(false)?;
                self.assert_form_data()?;
            }
            StatusCode::MULTIPLE_CHOICES => {
                self.assert_location(false)?;
                self.assert_query()?;
                self.assert_fragment()?;
                self.assert_content(true)?;
                self.assert_form_data()?;
            }
            #[cfg(feature = "post-redirect")]
            StatusCode::OK => {
                self.assert_location(true)?;
            }
            _ => return Err(BuilderError::InvalidStatus),
        }

        let mut location = self.format_location()?;

        #[cfg(feature = "post-redirect")]
        {
            if let StatusCode::OK = self.status {
                self.format_form_data(location.take().as_deref().unwrap_or("/"))?;
            }
        }

        let Builder {
            status,
            content,
            content_type,
            extra_headers,
            ..
        } = self;

        Ok(Redirect {
            status,
            location,
            content,
            content_type,
            extra_headers,
        })
    }

    fn format_location(&mut self) -> Result<Option<String>, BuilderError> {
        let mut url = match self.location.take() {
            Some(url) => url.parse::<Uri>().map_err(|_| BuilderError::InvalidLocation)?.to_string(),
            None => return Ok(None),
        };

        if let Some(query) = self.query.take().transpose()? {
            url.push('?');
            url.push_str(query.as_str());
        }

        if let Some(fragment) = self.fragment.take().transpose()? {
            url.push('#');
            url.push_str(fragment.as_str());
        }

        Ok(Some(url))
    }

    #[inline]
    fn assert_form_data(&self) -> Result<(), BuilderError> {
        if self.form_data.is_some() {
            Err(BuilderError::UnexpectedFormData)
        } else {
            Ok(())
        }
    }

    #[inline]
    fn assert_location(&self, needed: bool) -> Result<(), BuilderError> {
        if self.location.is_some() && !needed {
            Err(BuilderError::UnexpectedLocation)
        } else if self.location.is_none() && needed {
            Err(BuilderError::MissingLocation)
        } else {
            Ok(())
        }
    }

    #[inline]
    fn assert_query(&self) -> Result<(), BuilderError> {
        if self.query.is_some() {
            Err(BuilderError::UnexpectedQuery)
        } else {
            Ok(())
        }
    }

    #[inline]
    fn assert_fragment(&self) -> Result<(), BuilderError> {
        if self.fragment.is_some() {
            Err(BuilderError::UnexpectedFragment)
        } else {
            Ok(())
        }
    }

    #[inline]
    fn assert_content(&self, needed: bool) -> Result<(), BuilderError> {
        if self.content.is_some() && !needed {
            Err(BuilderError::UnexpectedContent)
        } else if self.content_type.is_some() && !needed {
            Err(BuilderError::UnexpectedContentType)
        } else if self.content.is_none() && needed {
            Err(BuilderError::MissingContent)
        } else {
            Ok(())
        }
    }
}

pub struct Redirect {
    status: StatusCode,
    location: Option<String>,
    content: Option<Box<dyn TransmuteBody + Send + Sync>>,
    content_type: Option<Mime>,
    extra_headers: HeaderMap<HeaderValue>,
}

impl Redirect {
    #[inline]
    pub fn status(&self) -> &StatusCode {
        &self.status
    }

    #[inline]
    pub fn location(&self) -> Option<&str> {
        self.location.as_deref()
    }

    #[inline]
    pub fn content_type(&self) -> Option<&Mime> {
        self.content_type.as_ref()
    }

    #[inline]
    pub fn moved_permanently() -> Builder {
        Builder {
            status: StatusCode::MOVED_PERMANENTLY,
            ..Default::default()
        }
    }

    #[inline]
    pub fn permanent_redirect() -> Builder {
        Builder {
            status: StatusCode::PERMANENT_REDIRECT,
            ..Default::default()
        }
    }

    #[inline]
    pub fn found() -> Builder {
        Builder {
            status: StatusCode::FOUND,
            ..Default::default()
        }
    }

    #[inline]
    pub fn see_other() -> Builder {
        Builder {
            status: StatusCode::SEE_OTHER,
            ..Default::default()
        }
    }

    #[inline]
    pub fn temporary_redirect() -> Builder {
        Builder {
            status: StatusCode::TEMPORARY_REDIRECT,
            ..Default::default()
        }
    }

    #[inline]
    pub fn not_modified() -> Builder {
        Builder {
            status: StatusCode::NOT_MODIFIED,
            ..Default::default()
        }
    }

    #[inline]
    pub fn multiple_choice() -> Builder {
        Builder {
            status: StatusCode::MULTIPLE_CHOICES,
            content_type: Some(mime::TEXT_HTML),
            ..Default::default()
        }
    }
}

impl Responder for Redirect {
    fn respond_with_builder(self, mut builder: ResponseBuilder, _ctx: &HttpContext) -> ResponseBuilder {
        builder = builder.status(self.status);

        if let Some(headers) = builder.headers_mut() {
            headers.extend(self.extra_headers);
        }

        if let Some(location) = self.location {
            builder = builder.header("Location", location.as_str())
        }

        if let Some(mut c) = self.content {
            builder = builder.body(c.transmute())
        }

        if let Some(ct) = self.content_type {
            builder = builder.header("Content-Type", ct.to_string())
        }

        builder
    }
}

#[cfg(feature = "post-redirect")]
mod post_redirect {
    use super::*;

    fn format_input(name: &str, value: &str) -> String {
        format!("<input type=\"hidden\" name=\"{name}\" value=\"{value}\"/>\n", name = name, value = value)
    }

    fn format_form(loc: &str, inputs: &str) -> String {
        format!(
            "<body onload=\"javascript:document.forms[0].submit()\">\n\
    <form method=\"post\" action=\"{location}\">\n\
        {inputs}\n\
    </form>\n\
</body>",
            location = loc,
            inputs = inputs
        )
    }

    impl Builder {
        #[inline]
        pub fn form_data<T: Serialize>(mut self, data: &T) -> Self {
            self.form_data = Some(
                serde_json::to_value(data)
                    .and_then(serde_json::from_value::<HashMap<String, String>>)
                    .map_err(|_e| BuilderError::InvalidFormData),
            );
            self
        }

        #[doc(hidden)]
        pub fn format_form_data(&mut self, loc: &str) -> Result<(), BuilderError> {
            let mut inputs = String::new();

            if let Some(data) = self.form_data.take().transpose()? {
                for (n, v) in data.into_iter() {
                    inputs.push_str(format_input(&n, &v).as_str())
                }
            }
            self.content = Some(Box::new(Some(format_form(loc, &inputs))));
            Ok(())
        }
    }

    impl Redirect {
        #[inline]
        pub fn post_redirect() -> Builder {
            Builder {
                status: StatusCode::OK,
                content_type: Some(mime::TEXT_HTML_UTF_8),
                ..Default::default()
            }
        }
    }
}
