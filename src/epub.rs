use std::{
    collections::HashMap,
    fs::{self, File},
    io::Read,
};

use quick_xml::{events::Event, Reader};
use zip::ZipArchive;

#[derive(Debug)]
pub struct Author {
    pub name: String,
    pub firstauthor: String,
}

#[derive(Debug)]
pub struct Series {
    pub name: String,
    pub index: i32,
}

impl Series {
    fn new() -> Self {
        Series {
            name: String::new(),
            index: 0,
        }
    }
}

#[derive(Debug)]
pub struct EpubMetadata {
    pub authors: Vec<Author>,
    pub genre: String,
    pub series: Series,
}

impl EpubMetadata {
    fn new() -> Self {
        EpubMetadata {
            authors: Vec::new(),
            genre: String::new(),
            series: Series::new(),
        }
    }
}

fn get_rootfile(archive: &mut ZipArchive<File>) -> String {
    let mut container = archive.by_name("META-INF/container.xml").unwrap();
    let mut xml_str_buffer = String::new();

    container.read_to_string(&mut xml_str_buffer).unwrap();

    let mut reader = Reader::from_str(&xml_str_buffer);
    reader.trim_text(true);

    let mut buf = Vec::new();
    let mut opf_filename = String::new();

    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) if e.local_name() == b"rootfile" => {
                opf_filename = String::from_utf8(
                    e.attributes()
                        .filter(|attr| attr.as_ref().unwrap().key == b"full-path")
                        .next()
                        .unwrap()
                        .unwrap()
                        .value
                        .to_vec(),
                )
                .unwrap();
                break;
            }
            Ok(Event::Eof) => break,
            _ => (),
        }
    }
    opf_filename
}

pub fn get_epub_metadata(filename: &str) -> Option<EpubMetadata> {
    let mut epub_meta = EpubMetadata::new();
    let file = fs::File::open(&filename);

    let file = match file {
        Err(_) => return None,
        Ok(file) => file,
    };

    let mut archive = ZipArchive::new(file).unwrap();

    let opf_filename = get_rootfile(&mut archive);

    let mut xml_str_buffer = String::new();
    let mut opf = archive.by_name(&opf_filename).unwrap();
    opf.read_to_string(&mut xml_str_buffer).unwrap();

    let mut reader = Reader::from_str(&xml_str_buffer);
    let mut buf = Vec::new();

    let mut curr_id = String::new();
    let mut creator_found = false;
    let mut file_as_found = false;
    let mut role_found = false;
    let mut genre_found = false;
    let mut series_found = false;
    let mut series_index_found = false;
    let mut is_epub3 = false;

    #[derive(Debug)]
    struct XmlAut {
        name: String,
        sort: String,
        role: String,
    }

    impl XmlAut {
        fn new() -> Self {
            XmlAut {
                name: String::new(),
                sort: String::new(),
                role: String::new(),
            }
        }
    }

    let mut xml_authors = HashMap::new();

    loop {
        match reader.read_event(&mut buf) {
            // See if we have EPUB3 or EPUB2
            Ok(Event::Start(ref e)) if e.local_name() == b"package" => {
                if e.attributes().any(|attr| {
                    attr.as_ref().unwrap().key == b"version"
                        && attr.as_ref().unwrap().value.starts_with(b"3")
                }) {
                    is_epub3 = true;
                }
            }
            Ok(Event::Start(ref e)) if e.local_name() == b"creator" => {
                creator_found = true;
                if is_epub3 {
                    if let Some(idval) = e
                        .attributes()
                        .filter(|attr| attr.as_ref().unwrap().key == b"id")
                        .next()
                    {
                        curr_id = "#".to_string()
                            + String::from_utf8(idval.unwrap().value.to_vec())
                                .unwrap()
                                .as_str();
                        xml_authors.insert(curr_id.clone(), XmlAut::new());
                    } else {
                        curr_id = "none".to_string() + xml_authors.len().to_string().as_str();
                        let entry = xml_authors.entry(curr_id.clone()).or_insert(XmlAut::new());
                        entry.role = "aut".to_string();
                    }
                } else {
                    if let Some(file_as_val) = e
                        .attributes()
                        .filter(|attr| attr.as_ref().unwrap().key.ends_with(b"file-as"))
                        .next()
                    {
                        curr_id = "none".to_string() + xml_authors.len().to_string().as_str();
                        let entry = xml_authors.entry(curr_id.clone()).or_insert(XmlAut::new());
                        entry.sort = file_as_val
                            .unwrap()
                            .unescape_and_decode_value(&reader)
                            .unwrap_or_default();
                        entry.role = "aut".to_string();
                    } else if let Some(_role_val) = e
                        .attributes()
                        .filter(|attr| attr.as_ref().unwrap().key.ends_with(b"role"))
                        .next()
                    {
                        curr_id = "none".to_string() + xml_authors.len().to_string().as_str();
                    }
                }
            }
            Ok(Event::Text(ref e)) if creator_found => {
                if is_epub3 {
                    let entry = xml_authors.entry(curr_id.clone()).or_insert(XmlAut::new());
                    entry.name = String::from_utf8(e.to_vec()).unwrap();
                } else {
                    let entry = xml_authors.entry(curr_id.clone()).or_insert(XmlAut::new());
                    entry.name = String::from_utf8(e.to_vec()).unwrap();
                    entry.role = "aut".to_string();
                }

                creator_found = false;
            }
            Ok(Event::Start(ref e)) if e.local_name() == b"meta" && is_epub3 => {
                if let Some(refines) = e
                    .attributes()
                    .filter(|attr| attr.as_ref().unwrap().key == b"refines")
                    .next()
                {
                    if e.attributes().any(|attr| {
                        attr.as_ref().unwrap().key == b"property"
                            && attr.as_ref().unwrap().value.ends_with(b"file-as")
                    }) {
                        curr_id = String::from_utf8(refines.unwrap().value.to_vec()).unwrap();
                        file_as_found = true;
                    } else if e.attributes().any(|attr| {
                        attr.as_ref().unwrap().key == b"property"
                            && attr.as_ref().unwrap().value.ends_with(b"role")
                    }) {
                        curr_id = String::from_utf8(refines.unwrap().value.to_vec()).unwrap();
                        role_found = true;
                    } else if e.attributes().any(|attr| {
                        attr.as_ref().unwrap().key == b"property"
                            && attr.as_ref().unwrap().value.ends_with(b"group-position")
                    }) {
                        series_index_found = true;
                    }
                }
                if e.attributes().any(|attr| {
                    attr.as_ref().unwrap().key == b"property"
                        && attr
                            .as_ref()
                            .unwrap()
                            .value
                            .ends_with(b"belongs-to-collection")
                }) {
                    series_found = true;
                }
            }
            Ok(Event::Empty(ref e)) if e.local_name() == b"meta" && !is_epub3 => {
                if e.attributes().any(|attr| {
                    attr.as_ref().unwrap().key == b"name"
                        && attr
                            .as_ref()
                            .unwrap()
                            .unescaped_value()
                            .unwrap()
                            .ends_with(b"series")
                }) {
                    epub_meta.series.name = e
                        .attributes()
                        .filter(|attr| attr.as_ref().unwrap().key == b"content")
                        .next()
                        .unwrap()
                        .unwrap()
                        .unescape_and_decode_value(&reader)
                        .unwrap_or_default();
                } else if e.attributes().any(|attr| {
                    attr.as_ref().unwrap().key == b"name"
                        && attr
                            .as_ref()
                            .unwrap()
                            .unescaped_value()
                            .unwrap()
                            .ends_with(b"series_index")
                }) {
                    let index_float = e
                        .attributes()
                        .filter(|attr| attr.as_ref().unwrap().key == b"content")
                        .next()
                        .unwrap()
                        .unwrap()
                        .unescape_and_decode_value(&reader)
                        .unwrap_or_default()
                        .parse::<f32>()
                        .unwrap_or_default();
                    epub_meta.series.index = index_float as i32;
                }
            }
            Ok(Event::Text(ref e)) if file_as_found && is_epub3 => {
                let entry = xml_authors.entry(curr_id.clone()).or_insert(XmlAut::new());
                entry.sort = String::from_utf8(e.to_vec()).unwrap();

                file_as_found = false;
            }
            Ok(Event::Text(ref e)) if role_found && is_epub3 => {
                let entry = xml_authors.entry(curr_id.clone()).or_insert(XmlAut::new());
                entry.role = String::from_utf8(e.to_vec()).unwrap();

                role_found = false;
            }
            Ok(Event::Text(ref e)) if series_found && is_epub3 => {
                epub_meta.series.name = String::from_utf8(e.to_vec()).unwrap();

                series_found = false;
            }
            Ok(Event::Text(ref e)) if series_index_found && is_epub3 => {
                epub_meta.series.index = String::from_utf8(e.to_vec())
                    .unwrap()
                    .parse()
                    .unwrap_or_default();

                series_index_found = false;
            }
            Ok(Event::Start(ref e)) if e.local_name() == b"subject" => {
                genre_found = true;
            }
            Ok(Event::Text(ref e)) if genre_found => {
                epub_meta.genre = e.unescape_and_decode(&reader).unwrap();
                genre_found = false;
            }
            Ok(Event::Eof) => break,
            _ => (),
        }
    }

    epub_meta.authors = xml_authors
        .into_iter()
        .filter(|&(_, ref xml_author)| &xml_author.role == "aut" && &xml_author.name.len() > &0)
        .map(|(_key, value)| Author {
            name: value.name,
            firstauthor: value.sort,
        })
        .collect();

    Some(epub_meta)
}
