use warp::reject;

#[derive(Debug)]
pub struct DBError;
impl reject::Reject for DBError {}

#[derive(Debug)]
pub struct JSONSerializationError;
impl reject::Reject for JSONSerializationError {}

#[derive(Debug)]
pub struct TemplateError;
impl reject::Reject for TemplateError {}

#[derive(Debug)]
pub struct HTTPClientError;
impl reject::Reject for HTTPClientError {}

#[derive(Debug)]
pub struct ValidateResponseDeserializeError;
impl reject::Reject for ValidateResponseDeserializeError {}

#[derive(Debug)]
pub struct NotAuthorized;
impl reject::Reject for NotAuthorized {}
