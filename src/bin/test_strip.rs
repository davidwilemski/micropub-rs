use micropub_rs::media_util;

use std::io::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut file = std::fs::File::open("/home/dtw/Downloads/nokomis.jpg")?;
    let mut newfile = std::fs::File::create("/home/dtw/Downloads/stripped-nokomis.jpg")?;

    let mut data: Vec<u8> = Vec::new();
    file.read_to_end(&mut data)?;

    println!("attempting to strip media starting with: {:?}", &data[0..64]);
    println!("media length: {}", data.len());

    data = media_util::strip_media(&data, "jpeg").unwrap();

    newfile.write_all(&data)?;
    newfile.sync_all()?;

    Ok(())
}
