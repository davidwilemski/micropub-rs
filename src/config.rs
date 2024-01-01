use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct MicropubSiteConfig {
    pub blobject_store_base_uri: String,
    pub template_dir: String,
    pub database_url: String,

    pub micropub: MicropubConfig,

    pub site: SiteConfig,
}

#[derive(Debug, Deserialize)]
pub struct SiteConfig {
    pub site_name: String,
    pub menu_items: Vec<(String, String)>,
    pub socials: Vec<String>,

}

#[derive(Debug, Deserialize)]
pub struct MicropubConfig {
    #[serde(default = "default_auth_endpoint")]
    pub auth_endpoint: String,

    #[serde(default = "default_auth_token_endpoint")]
    pub auth_token_endpoint: String,

    pub host_website: String,
    pub media_endpoint: String,
    #[serde(default = "default_max_upload_length")]
    pub media_endpoint_max_upload_length: usize,
    pub micropub_endpoint: String,
}

fn default_auth_token_endpoint() -> String {
    crate::DEFAULT_AUTH_TOKEN_ENDPOINT.into()
}

fn default_auth_endpoint() -> String {
    crate::DEFAULT_AUTH_ENDPOINT.into()
}

fn default_max_upload_length() -> usize {
    crate::DEFAULT_MAX_CONTENT_LENGTH
}
