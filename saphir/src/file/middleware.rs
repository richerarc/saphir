use crate::prelude::*;
use percent_encoding::percent_decode;
use std::path::{Path, PathBuf};
use std::str::Utf8Error;
use std::time::SystemTime;
use crate::file::etag::{SystemTimeExt, EntityTag};
use crate::file::conditional_request::{is_precondition_failed, is_fresh};
use mime::Mime;
use mime_guess::from_path;
use time::PrimitiveDateTime;

struct FileMiddleware {
    base_path: PathBuf,
    www_path: PathBuf,
}

impl FileMiddleware {
    pub fn new(base_path: &str, www_path: &str) -> Self {
        FileMiddleware {
            base_path: PathBuf::from(base_path.to_string()),
            www_path: PathBuf::from(www_path.to_string()),
        }
    }

    async fn next_inner(&self, mut ctx: HttpContext, _chain: &dyn MiddlewareChain) -> Result<HttpContext, SaphirError> {
        let mut builder = Builder::new();
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
            res.status(StatusCode::NOT_MODIFIED);

            ctx.after(
                builder
                    .status(304)
                    .header(header::ETAG, etag.get_tag())
                    .header(header::LAST_MODIFIED, Self::format_systemtime(last_modified))
                    .build()?,
            );
            return Ok(ctx);
        }

        ctx.after(
            builder
                .header(header::CACHE_CONTROL, "public")
                .header(header::CACHE_CONTROL, "max-age=0")
                .build()?,
        );
        Ok(ctx)
    }

    fn format_systemtime(time: SystemTime) -> String {
        PrimitiveDateTime::from(time).format("%a, %d %b %Y %T %Z")
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