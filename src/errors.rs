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
pub struct ServerError;
impl From<tokio::task::JoinError> for ServerError {
    fn from(e: tokio::task::JoinError) -> Self {
        Self {}
    }
}

impl From<ServerError> for StatusCode {
    fn from(e: ServerError) -> Self {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

#[derive(Debug)]
pub struct JSONSerializationError;

#[derive(Debug)]
pub struct TemplateError;
impl From<TemplateError> for StatusCode {
    fn from(e: TemplateError) -> Self {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

#[derive(Debug)]
pub struct HTTPClientError;
impl From<HTTPClientError> for StatusCode {
    fn from(e: HTTPClientError) -> Self {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

#[derive(Debug)]
pub struct ValidateResponseDeserializeError;
impl From<ValidateResponseDeserializeError> for StatusCode {
    fn from(e: ValidateResponseDeserializeError) -> Self {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

#[derive(Debug)]
pub struct NotAuthorized;

#[derive(Debug)]
pub struct MediaUploadError;
impl From<MediaUploadError> for StatusCode {
    fn from(e: MediaUploadError) -> Self {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

#[derive(Debug)]
pub struct MediaFetchError;
impl From<MediaFetchError> for StatusCode {
    fn from(e: MediaFetchError) -> Self {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

#[derive(Debug)]
pub struct MediaStripError(&'static str);
impl From<MediaStripError> for StatusCode {
    fn from(e: MediaStripError) -> Self {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

impl From<magick_rust::MagickError> for MediaStripError {
    fn from(s: magick_rust::MagickError) -> Self {
        Self(s.0)
    }
}
