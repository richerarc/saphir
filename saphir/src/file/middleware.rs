use crate::{
    file::{
        cache::FileCache,
        conditional_request::{format_systemtime, is_fresh, is_precondition_failed},
        etag::{EntityTag, SystemTimeExt},
        range::Range,
        range_requests::{extract_range, is_range_fresh, is_satisfiable_range},
        Compression,
    },
    handler::DynHandler,
    prelude::*,
};
use mime::Mime;
use mime_guess::from_path;
use percent_encoding::percent_decode;
use std::{
    path::{Path, PathBuf},
    str::{FromStr, Utf8Error},
    time::SystemTime,
};

const DEFAULT_CACHE_MAX_FILE_SIZE: u64 = 2_097_152;
const DEFAULT_CACHE_MAX_CAPACITY: u64 = 536_870_912;
const DEFAULT_MAX_AGE: i64 = 0;
const DEFAULT_INDEX_FILES: [&str; 2] = ["index.html", "index.htm"];
const DEFAULT_TRY_FILES: [&str; 2] = ["$uri", "$uri/"];

pub struct FileMiddleware {
    base_path: PathBuf,
    www_path: PathBuf,
    index_files: Vec<String>,
    try_files: Vec<String>,
    cache: FileCache,
    file_not_found_handler: Option<Box<dyn DynHandler<Body> + 'static + Send + Sync>>,
    max_age: i64,
}

impl FileMiddleware {
    pub fn new(base_path: &str, www_path: &str) -> Self {
        FileMiddleware {
            base_path: PathBuf::from(base_path.to_string()),
            www_path: PathBuf::from(www_path.to_string()),
            index_files: DEFAULT_INDEX_FILES.iter().map(|s| s.to_string()).collect(),
            try_files: DEFAULT_TRY_FILES.iter().map(|s| s.to_string()).collect(),
            cache: FileCache::new(DEFAULT_CACHE_MAX_FILE_SIZE, DEFAULT_CACHE_MAX_CAPACITY),
            file_not_found_handler: None,
            max_age: DEFAULT_MAX_AGE,
        }
    }

    async fn next_inner(&self, mut ctx: HttpContext, _chain: &dyn MiddlewareChain) -> Result<HttpContext, SaphirError> {
        let mut builder = Builder::new();
        let mut cache = self.cache.clone();
        let req = ctx.state.request_unchecked();
        let req_path = req.uri().path();
        let is_head_request = matches!(req.method(), &Method::HEAD);

        let mut file_path = None;
        let mut response_code: Option<u16> = None;
        'try_files: for path in &self.try_files {
            if path.len() == 4 && path.starts_with('=') {
                let code = path[1..]
                    .parse()
                    .unwrap_or_else(|_| panic!("Invalid token provided to `FileMiddleware::try_files`: {}", path));
                response_code = Some(code);
                break 'try_files;
            }

            let path = path.replace("$uri", req_path);
            let is_dir = path.ends_with('/');

            let path = match self.file_path_from_path(&path) {
                Ok(p) => p,
                Err(_) => continue 'try_files,
            };

            if path.is_hidden() {
                continue 'try_files;
            }

            if is_dir {
                if path.is_dir() {
                    for index in &self.index_files {
                        let index_path = path.join(index);
                        if index_path.is_file() {
                            file_path = Some(index_path);
                            break 'try_files;
                        }
                    }
                }
            } else if path.is_file() {
                file_path = Some(path);
                break 'try_files;
            }
        }

        let path = match (file_path, &self.file_not_found_handler) {
            (Some(f), _) => f,
            (None, Some(handler)) => {
                let req = ctx.state.take_request().ok_or(SaphirError::RequestMovedBeforeHandler)?;
                ctx.after((*handler).dyn_handle(req).await.dyn_respond(Builder::new(), &ctx).build()?);
                return Ok(ctx);
            }
            (None, None) => {
                ctx.after(builder.status(response_code.unwrap_or(404)).build()?);
                return Ok(ctx);
            }
        };

        if !self.path_is_under_base_path(&path) {
            ctx.after(builder.status(401).build()?);
            return Ok(ctx);
        }

        let (last_modified, mut size) = (path.mtime(), path.size());
        let etag = EntityTag::new(false, format!("{}-{}", last_modified.timestamp(), size).as_str());

        if is_precondition_failed(req, &etag, &last_modified) {
            ctx.after(builder.status(412).build()?);
            return Ok(ctx);
        }

        if is_fresh(req, &etag, &last_modified) {
            ctx.after(builder.status(304).header(header::LAST_MODIFIED, format_systemtime(last_modified)).build()?);
            return Ok(ctx);
        }

        let mut is_partial_content = false;

        let compression = req
            .headers()
            .get(header::ACCEPT_ENCODING)
            .and_then(|header| header.to_str().ok())
            .and_then(|str| str.split(',').map(|encoding| Compression::from_str(encoding.trim()).unwrap_or_default()).max())
            .unwrap_or_default();

        if let Some(range) = req
            .headers()
            .get(header::RANGE)
            .and_then(|header| header.to_str().ok())
            .and_then(|header| Range::from_str(header).ok())
        {
            if let (true, Some(content_range)) = (is_range_fresh(req, &etag, &last_modified), is_satisfiable_range(&range, size)) {
                if let Some(range) = extract_range(&content_range) {
                    if !is_head_request {
                        let file = cache.open_file_with_range(&path, range).await?;
                        size = (range.1 - range.0) + 1;
                        builder = builder.file(file);
                    }
                }
                builder = builder
                    .header(http::header::CONTENT_RANGE, content_range.to_string())
                    .status(StatusCode::PARTIAL_CONTENT);
                is_partial_content = true;
            }
        }

        if !is_partial_content && !is_head_request {
            let file = cache.open_file(&path, compression).await?;
            size = file.get_size();
            builder = builder.file(file);
        }

        if compression != Compression::Raw {
            builder = builder.header(header::CONTENT_ENCODING, compression.to_string())
        }

        builder = builder
            .header(http::header::ACCEPT_RANGES, "bytes")
            .header(header::CONTENT_TYPE, Self::guess_path_mime(&path).to_string())
            .header(header::CONTENT_LENGTH, size)
            .header(header::CACHE_CONTROL, format!("public, max-age={}", self.max_age))
            .header(header::ETAG, etag.get_tag());
        ctx.after(builder.build()?);

        Ok(ctx)
    }

    fn file_path_from_path(&self, path: &str) -> Result<PathBuf, Utf8Error> {
        percent_decode(path[1..].as_bytes())
            .decode_utf8()
            .map(|path_str| PathBuf::from(path_str.to_string()))
            .map(|path| {
                if path.starts_with(&self.base_path) {
                    path.strip_prefix(&self.base_path).unwrap().to_path_buf()
                } else {
                    path
                }
            })
            .map(|path| self.www_path.join(path))
    }

    fn path_is_under_base_path<P: AsRef<Path>>(&self, path: P) -> bool {
        Path::starts_with(path.as_ref(), &self.www_path)
    }

    fn guess_path_mime<P: AsRef<Path>>(path: P) -> mime::Mime {
        let path = path.as_ref();
        path.mime()
            .unwrap_or_else(|| if path.is_dir() { mime::TEXT_HTML_UTF_8 } else { mime::TEXT_PLAIN_UTF_8 })
    }
}

impl Middleware for FileMiddleware {
    fn next(&'static self, ctx: HttpContext, chain: &'static dyn MiddlewareChain) -> BoxFuture<'static, Result<HttpContext, SaphirError>> {
        #[cfg(feature = "tracing-instrument")]
        {
            use tracing::Instrument;
            let span = tracing::span!(tracing::Level::ERROR, "saphir:file");
            let span2 = span.clone();
            async move {
                let res = self.next_inner(ctx, chain).instrument(span).await;
                res.map(|mut ctx| {
                    if let Some(res) = ctx.state.response_mut() {
                        res.span = Some(span2);
                    }
                    ctx
                })
            }
            .boxed()
        }
        #[cfg(not(feature = "tracing-instrument"))]
        {
            self.next_inner(ctx, chain).boxed()
        }
    }
}

pub struct FileMiddlewareBuilder {
    base_path: PathBuf,
    www_path: PathBuf,
    index_files: Option<Vec<String>>,
    try_files: Option<Vec<String>>,
    max_file_size: Option<u64>,
    max_capacity: Option<u64>,
    file_not_found_handler: Option<Box<dyn 'static + DynHandler<Body> + Send + Sync>>,
    max_age: i64,
}

impl FileMiddlewareBuilder {
    pub fn new(base_path: &str, www_path: &str) -> Self {
        FileMiddlewareBuilder {
            base_path: PathBuf::from(base_path),
            www_path: PathBuf::from(www_path),
            index_files: None,
            try_files: None,
            max_file_size: None,
            max_capacity: None,
            file_not_found_handler: None,
            max_age: DEFAULT_MAX_AGE,
        }
    }

    /// Maximum size of a single file for the in-memory cache.
    /// Files exceeding this size won't be cached, regardless of the specified
    /// cache max capacity.
    ///
    /// Default: 2MB
    pub fn max_file_size(mut self, size: u64) -> Self {
        self.max_file_size = Some(size);
        self
    }

    /// Maximum total capacity for the in-memory cache.
    /// All fetched files will be kept cached in memory until reaching this
    /// maximum capacity.
    ///
    /// Default: 512MB
    pub fn max_capacity(mut self, size: u64) -> Self {
        self.max_capacity = Some(size);
        self
    }

    /// Specify the `Cache-Control: max-age` header returned by this middleware.
    ///
    /// Default: `Cache-Control: max-age=0`
    pub fn max_age(mut self, max_age: i64) -> Self {
        self.max_age = max_age;
        self
    }

    /// Specify a list of index files which will be tried in order when
    /// reaching a directory. This behave similarly to nginx's [index]
    /// directive.
    ///
    /// Default: `"index.html index.htm"`.
    ///
    /// [index]: https://docs.nginx.com/nginx/admin-guide/web-server/serving-static-content/#root
    pub fn index_files(mut self, index_files: &str) -> Self {
        self.index_files = Some(index_files.split(' ').map(|s| s.trim().to_string()).collect());
        self
    }

    /// Specify that no index file should be looked up when pointing to a
    /// directory. This effectively remove the default `index.html` and
    /// `index.htm` index files.
    pub fn no_directory_index(mut self) -> Self {
        self.index_files = Some(Vec::new());
        self
    }

    /// List of files to try. This should be construced using the `$uri` token.
    /// This behave similarly to nginx's [try_files] directive.
    ///
    /// Default: `"$uri $uri"`.
    ///
    /// [try_files]: https://docs.nginx.com/nginx/admin-guide/web-server/serving-static-content/#options
    pub fn try_files(mut self, try_files: &str) -> Self {
        self.try_files = Some(try_files.split(' ').map(|s| s.trim().to_string()).collect());
        self
    }

    /// Attach a handler to be called when the requested file is not found.
    pub fn file_not_found_handler<H>(mut self, handler: H) -> Self
    where
        H: 'static + DynHandler<Body> + Sync + Send,
    {
        self.file_not_found_handler = Some(Box::new(handler));
        self
    }

    pub fn build(self) -> Result<FileMiddleware, SaphirError> {
        Ok(FileMiddleware {
            base_path: self.base_path,
            www_path: self.www_path,
            index_files: self.index_files.unwrap_or_else(|| DEFAULT_INDEX_FILES.iter().map(|s| s.to_string()).collect()),
            try_files: self.try_files.unwrap_or_else(|| DEFAULT_TRY_FILES.iter().map(|s| s.to_string()).collect()),
            cache: FileCache::new(
                self.max_file_size.unwrap_or(DEFAULT_CACHE_MAX_FILE_SIZE),
                self.max_capacity.unwrap_or(DEFAULT_CACHE_MAX_CAPACITY),
            ),
            file_not_found_handler: self.file_not_found_handler,
            max_age: self.max_age,
        })
    }
}

pub trait PathExt {
    fn is_hidden(&self) -> bool;
    fn mtime(&self) -> SystemTime;
    fn size(&self) -> u64;
    fn mime(&self) -> Option<Mime>;
}

impl PathExt for Path {
    /// Check if path is hidden.
    fn is_hidden(&self) -> bool {
        self.file_name().and_then(|s| s.to_str()).map(|s| s.starts_with('.')).unwrap_or(false)
    }

    /// Get modified time from a path.
    fn mtime(&self) -> SystemTime {
        self.metadata().and_then(|meta| meta.modified()).unwrap()
    }

    /// Get file size from a path.
    fn size(&self) -> u64 {
        self.metadata().map(|meta| meta.len()).unwrap_or_default()
    }

    /// Guess MIME type from a path.
    fn mime(&self) -> Option<Mime> {
        from_path(self).first()
    }
}
