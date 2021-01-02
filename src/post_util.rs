use chrono::{DateTime, Local, TimeZone};

fn get_first_n(n: usize, input: &str) -> String {
    input.to_lowercase().chars().filter(|c| c.is_alphanumeric() || c.is_whitespace()).take(n).collect()
}

pub fn get_slug(name: Option<&str>, now_fn: fn() -> DateTime<Local>) -> String {
    let slug = match name {
        Some(n) => {
            let now = now_fn().format("%Y/%m/%d");
            format!("{}/{}", now, get_first_n(32, n))
        },
        None => {
            let now = now_fn().format("%Y/%m/%d/%H%M%S");
            format!("{}", now)
        }
    };

    slug.replace(" ", "-")
}

pub fn get_local_datetime(datetime: &str, offset: Option<chrono::FixedOffset>) -> Result<DateTime<Local>, chrono::format::ParseError> {
    chrono::NaiveDateTime::parse_from_str(datetime, "%Y-%m-%d %H:%M:%S")
        .map(|ndt| {
            chrono::DateTime::<chrono::Local>::from_utc(
                ndt,
                offset.unwrap_or(chrono::FixedOffset::east(7 * 3600)),
            )
        })
}

#[cfg(test)]
mod test {
    use super::get_slug;

    use chrono::{DateTime, Local, TimeZone};

    fn now() -> DateTime<Local> {
        Local.timestamp(1603571553i64, 0u32)
    }

    #[test]
    fn it_uses_name_if_name_exists() {
        assert_eq!(get_slug(Some("testing"), now), "2020/10/24/testing");
    }

    #[test]
    fn it_replaces_spaces_in_name_with_hyphens() {
        assert_eq!(get_slug(Some("testing stuff"), now), "2020/10/24/testing-stuff");
    }

    #[test]
    fn it_removes_non_alpha_numeric_chars_and_truncates() {
        assert_eq!(
            get_slug(Some("testing stuff! This is a really long title."), now),
            "2020/10/24/testing-stuff-this-is-a-really-l"
        );
    }

    #[test]
    fn it_uses_content_if_no_name() {
        assert_eq!(get_slug(None, now), "2020/10/24/153233");
    }
    #[test]
    fn it_truncates_content_for_slug() {
        assert_eq!( get_slug(None, now), "2020/10/24/153233");
    }
}
