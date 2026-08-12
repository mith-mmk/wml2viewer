//! GIF utility helpers.

pub fn make_metadata(header: &GifHaeder) -> HashMap<String,DataMap> {
    let mut map :HashMap<String,DataMap> = HashMap::new();
    map.insert("Format".to_string(),DataMap::Ascii("GIF".to_string()));
    map.insert("width".to_string(),DataMap::Uint(header.width as u64));
    map.insert("height".to_string(),DataMap::Uint(header.height as u64));
    if let Some(comment) = &header.comment {
        map.insert("comment".to_string(),DataMap::Ascii(comment.to_string()));
    }

    map
}


