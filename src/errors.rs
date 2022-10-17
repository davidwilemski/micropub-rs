use warp::reject;
use http::StatusCode;

#[derive(Debug)]
pub struct DBError {
    not_found: bool,
}
impl DBError {
    pub fn new() -> Self {
        DBError { not_found: false }
    }

    pub fn not_found() -> Self {
        DBError { not_found: true }
    }
}
impl reject::Reject for DBError {}
impl From<DBError> for StatusCode {
    fn from(e: DBError) -> Self {
        if e.not_found {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

#[derive(Debug)]
pub struct JSONSerializationError;
impl reject::Reject for JSONSerializationError {}

#[derive(Debug)]
pub struct TemplateError;
impl reject::Reject for TemplateError {}
impl From<TemplateError> for StatusCode {
    fn from(e: TemplateError) -> Self {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

#[derive(Debug)]
pub struct HTTPClientError;
impl reject::Reject for HTTPClientError {}

#[derive(Debug)]
pub struct ValidateResponseDeserializeError;
impl reject::Reject for ValidateResponseDeserializeError {}

#[derive(Debug)]
pub struct NotAuthorized;
impl reject::Reject for NotAuthorized {}

#[derive(Debug)]
pub struct MediaUploadError;
impl reject::Reject for MediaUploadError {}

#[derive(Debug)]
pub struct MediaFetchError;
impl reject::Reject for MediaFetchError {}

#[derive(Debug)]
pub struct MediaStripError(&'static str);
impl reject::Reject for MediaStripError {}

impl From<magick_rust::MagickError> for MediaStripError {
    fn from(s: magick_rust::MagickError) -> Self {
        Self(s.0)
    }
}
