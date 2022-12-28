// TODO make these configurable via command line, environment, or config file?
pub const MAX_CONTENT_LENGTH: u64 = 1024 * 1024 * 50; // 50 megabytes
pub const AUTH_TOKEN_ENDPOINT: &str = "https://tokens.indieauth.com/token";
pub const HOST_WEBSITE: &str = "https://davidwilemski.com/";
pub const MENU_ITEMS: &[(&str, &str)] = &[("Archive", "/archives")];
pub const MEDIA_ENDPOINT_VAR: &str = "MICROPUB_RS_MEDIA_ENDPOINT";
pub const TEMPLATE_DIR_VAR: &str = "MICROPUB_RS_TEMPLATE_DIR";
pub const SOCIAL: &str = "https://github.com/davidwilemski";
pub const MICROPUB_ENDPOINT: &str = "/micropub";
pub const AUTH_ENDPOINT: &str = "https://indieauth.com/auth";
pub const TOKEN_ENDPOINT: &str = "https://tokens.indieauth.com/token";
