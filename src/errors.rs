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
