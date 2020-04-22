use crate::{
    file::{
        conditional_request::{format_systemtime, is_fresh, is_precondition_failed},
        etag::{EntityTag, SystemTimeExt},
        range::Range,
        range_requests::{extract_range, is_range_fresh, is_satisfiable_range},
        Compression, FileCache,
    },
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

struct FileMiddleware {
    base_path: PathBuf,
    www_path: PathBuf,
    cache: FileCache,
}

impl FileMiddleware {
    pub fn new(base_path: &str, www_path: &str) -> Self {
        FileMiddleware {
            base_path: PathBuf::from(base_path.to_string()),
            www_path: PathBuf::from(www_path.to_string()),
            cache: FileCache::new(),
        }
    }

    async fn next_inner(&self, mut ctx: HttpContext, _chain: &dyn MiddlewareChain) -> Result<HttpContext, SaphirError> {
        let mut builder = Builder::new();
        let mut cache = self.cache.clone();
        let req = ctx.state.request_unchecked();
        let path = match self.file_path_from_path(req.uri().path()) {
            Ok(path) => path,
            Err(_) => {
                ctx.after(builder.status(400).build()?);
                return Ok(ctx);
            }
        };

        if !self.path_exists(&path) {
            info!("Path doesn't exist: {}", path.display());
            ctx.after(builder.status(404).build()?);
            return Ok(ctx);
        }

        if !self.path_is_under_base_path(&path) {
            ctx.after(builder.status(401).build()?);
            return Ok(ctx);
        }

        let (mtime, size) = (path.mtime(), path.size());
        let last_modified = mtime.into();
        let etag = EntityTag::new(false, format!("{}-{}", mtime.timestamp(), size).as_str());

        if is_precondition_failed(req, &etag, &last_modified) {
            ctx.after(builder.status(412).build()?);
            return Ok(ctx);
        }

        let mime_type = Self::guess_path_mime(&path);
        if mime_type.subtype() == mime::HTML {
            builder = builder.header(header::X_FRAME_OPTIONS, "DENY")
        }

        if is_fresh(&req, &etag, &last_modified) {
            ctx.after(builder.status(304).header(header::LAST_MODIFIED, format_systemtime(last_modified)).build()?);
            return Ok(ctx);
        }

        let mut is_partial_content = false;

        let compression = req
            .headers()
            .get(header::ACCEPT_ENCODING)
            .and_then(|header| header.to_str().ok())
            .map(|str| str.split(',').map(|encoding| Compression::from_str(encoding.trim()).unwrap_or_default()).max())
            .flatten()
            .unwrap_or_default();

        if let Some(range) = req
            .headers()
            .get(header::RANGE)
            .and_then(|header| header.to_str().ok())
            .and_then(|header| Range::from_str(header).ok())
        {
            match (is_range_fresh(&req, &etag, &last_modified), is_satisfiable_range(&range, size as u64)) {
                (true, Some(content_range)) => {
                    if let Some(range) = extract_range(&content_range) {
                        builder = builder.file(cache.open_file_with_range(&path, range).await?).map_err(|error| error.1)?;
                    }
                    builder = builder
                        .header(http::header::CONTENT_RANGE, content_range.to_string())
                        .status(StatusCode::PARTIAL_CONTENT);
                    is_partial_content = true;
                }
                _ => (),
            }
        }

        if !is_partial_content {
            builder = builder.file(cache.open_file(&path, compression).await?).map_err(|(_, e)| e)?;
        }

        if compression != Compression::Raw {
            builder = builder.header(header::CONTENT_ENCODING, compression.to_string())
        }

        builder = builder
            .header(header::CONTENT_TYPE, mime_type.to_string())
            .header(header::CACHE_CONTROL, "public")
            .header(header::CACHE_CONTROL, "max-age=0")
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
            .map(|path| if path.is_dir() { path.join("index.html") } else { path })
    }

    fn path_exists<P: AsRef<Path>>(&self, path: P) -> bool {
        let path = path.as_ref();
        path.exists() && !self.path_is_hidden(path)
    }

    fn path_is_hidden<P: AsRef<Path>>(&self, path: P) -> bool {
        path.as_ref().is_hidden()
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
        self.next_inner(ctx, chain).boxed()
    }
}

pub trait PathExt {
    fn is_hidden(&self) -> bool;
    fn mtime(&self) -> SystemTime;
    fn size(&self) -> u64;
    fn mime(&self) -> Option<Mime>;
}

impl PathExt for Path {
    /// Guess MIME type from a path.
    fn mime(&self) -> Option<Mime> {
        from_path(&self).first()
    }

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
}
