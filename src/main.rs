use rusqlite::{named_params, Connection, Result, NO_PARAMS};
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use xml::reader::{EventReader, ParserConfig, XmlEvent};
use zip::{read::ZipFile, ZipArchive};

fn get_root_file(container: ZipFile) -> Result<Option<String>, Box<dyn Error>> {
    let parser = EventReader::new(container);

    for e in parser {
        match e {
            Ok(XmlEvent::StartElement {
                name, attributes, ..
            }) if name.local_name == "rootfile" => {
                for attr in attributes {
                    if attr.name.local_name == "full-path" {
                        return Ok(Some(attr.value));
                    }
                }
            }
            Err(e) => {
                return Err(Box::new(e));
            }
            _ => {}
        }
    }
    Ok(None)
}

fn get_attribute_file_as(opf: ZipFile) -> Option<String> {
    let parser = ParserConfig::new()
        .trim_whitespace(true)
        .ignore_comments(true)
        .coalesce_characters(true)
        .create_reader(opf);

    let mut refines_found = false;
    let mut is_epub3 = false;
    let mut creator_id = String::new();

    for e in parser {
        match e {
            Ok(XmlEvent::StartElement {
                name, attributes, ..
            }) if name.local_name == "package" => {
                for attr in attributes {
                    if attr.name.local_name == "version" {
                        if attr.value.starts_with("3") == true {
                            is_epub3 = true;
                        }
                    }
                }
            }
            Ok(XmlEvent::StartElement {
                name, attributes, ..
            }) if name.local_name == "creator" => {
                for attr in attributes {
                    if attr.name.local_name == "file-as" {
                        println!("File-as: {}", &attr.value);
                        return Some(attr.value);
                    }
                    if is_epub3 && attr.name.local_name == "id" {
                        creator_id = attr.value;
                    }
                }
            }
            Ok(XmlEvent::StartElement {
                name, attributes, ..
            }) if name.local_name == "meta" => {
                if attributes.iter().any(|attr| {
                    attr.name.local_name == "refines"
                        && (attr.value == creator_id
                            || attr.value == "#".to_owned() + creator_id.as_str())
                }) && attributes
                    .iter()
                    .any(|attr| attr.name.local_name == "property" && attr.value == "file-as")
                {
                    refines_found = true;
                }
            }
            Ok(XmlEvent::Characters(value)) => {
                if refines_found == true {
                    return Some(value);
                }
            }
            Ok(XmlEvent::StartElement { .. }) => {
                if refines_found == true {
                    refines_found = false;
                }
            }
            Err(_e) => {
                break;
            }
            _ => {}
        }
    }

    None
}

struct BookEntry {
    id: i32,
    filepath: String,
}

fn main() {
    let mut conn = Connection::open("/mnt/ext1/system/explorer-3/explorer-3.db").unwrap();
    let tx = conn.transaction().unwrap();
    {
        let mut stmt = tx.prepare("SELECT id FROM books_impl WHERE ext LIKE 'epub' AND author LIKE '% %' AND firstauthor NOT LIKE '%\\,%' ESCAPE '\\'").unwrap();
        let mut rows = stmt.query(NO_PARAMS).unwrap();
        let mut book_ids: Vec<i32> = Vec::new();
        while let Some(row) = rows.next().unwrap() {
            book_ids.push(row.get(0).unwrap());
        }

        let mut bookentries = Vec::new();

        for book_id in book_ids {
            let mut stmt = tx.prepare("SELECT folders.name,files.filename FROM files,folders WHERE files.book_id = :book_id AND files.storageid = 1 AND files.folder_id = folders.id").unwrap();
            let mut rows = stmt
                .query_named(named_params! { ":book_id": book_id })
                .unwrap();
            while let Some(row) = rows.next().unwrap() {
                let prefix: String = row.get(0).unwrap();
                let filename: String = row.get(1).unwrap();
                let filepath = format!("{}/{}", prefix, filename);
                bookentries.push(BookEntry {
                    id: book_id,
                    filepath: filepath,
                });
            }
        }

        //println!("Number of entries found: {}", bookentries.len());

        for entry in bookentries {
            let file = File::open(entry.filepath.as_str());
            let file = match file {
                Err(_) => continue,
                Ok(file) => file,
            };

            let mut archive = ZipArchive::new(BufReader::new(file)).unwrap();

            let container = archive.by_name("META-INF/container.xml").unwrap();

            if let Some(opf_file) = get_root_file(container).unwrap() {
                let opf = archive.by_name(opf_file.as_str()).unwrap();
                if let Some(file_as) = get_attribute_file_as(opf) {
                    let mut stmt = tx
                        .prepare("UPDATE books_impl SET firstauthor = :file_as WHERE id = :book_id")
                        .unwrap();
                    stmt.execute_named(named_params![":file_as": file_as, ":book_id": entry.id])
                        .unwrap();
                }
            }
        }
    }
    tx.commit().unwrap();
}
