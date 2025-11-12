use std::sync::Once;

use magick_rust::{magick_wand_genesis, MagickWand};
use mime;

use crate::errors::MediaStripError;

static START: Once = Once::new();

pub fn guess_format(content_type: &Option<&str>) -> Option<String> {
    if let Some(ct) = content_type {
        if let Ok(m) = ct.parse::<mime::Mime>() {
            match (m.type_(), m.subtype()) {
                (mime::IMAGE, mime::STAR) => None,
                (mime::IMAGE, format) => Some(format.as_str().into()),
                _ => None,
            }
        } else {
            None
        }
    } else {
        None
    }
}

pub fn strip_media(contents: &[u8], format: &str) -> Result<Vec<u8>, MediaStripError> {
    START.call_once(|| {
        magick_wand_genesis();
    });

    let wand = MagickWand::new();
    // suspect the blob to image function in the docker container is not working properly?
    // some runtime dep missing?
    let r = wand.read_image_blob(contents);
    println!("wand: {:?}", wand);
    r?;
    wand.strip_image()?;
    Ok(wand.write_image_blob(format)?)
}
