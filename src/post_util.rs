use url::form_urlencoded;

pub fn get_slug(name: Option<&str>, content: &str) -> String {
    let unencoded_slug = match name {
        Some(n) => n,
        // TODO might want to use a date/time instead for not slugs in the future?
        // maybe %Y%m%d-%H%M%S
        None => {
            let end = content.len().min(32);
            &content[0..end]
        }
    };

    form_urlencoded::byte_serialize(unencoded_slug.replace(" ", "-").as_bytes()).collect()
}

#[cfg(test)]
mod test {
    use super::get_slug;

    #[test]
    fn it_uses_name_if_name_exists() {
        assert_eq!(get_slug(Some("testing"), "nothing"), "testing");
    }

    #[test]
    fn it_replaces_spaces_in_name_with_hyphens() {
        assert_eq!(get_slug(Some("testing stuff"), "nothing"), "testing-stuff");
    }

    #[test]
    fn it_uses_content_if_no_name() {
        assert_eq!(get_slug(None, "nothing"), "nothing");
    }
    #[test]
    fn it_truncates_content_for_slug() {
        assert_eq!(
            get_slug(None, "nothing: this is a rather long title"),
            "nothing%3A-this-is-a-rather-long-t"
        );
    }
}
