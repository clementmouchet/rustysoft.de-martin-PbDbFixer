mod pocketbook;

use rusqlite::{named_params, Connection, Result, Transaction, NO_PARAMS};
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
    let mut refines_entries = Vec::new();
    let mut is_epub3 = false;
    let mut creator_ids = Vec::new();

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
                        return Some(attr.value);
                    }
                    if is_epub3 && attr.name.local_name == "id" {
                        creator_ids.push("#".to_owned() + attr.value.as_str());
                    }
                }
            }
            Ok(XmlEvent::StartElement {
                name, attributes, ..
            }) if name.local_name == "meta" => {
                if attributes.iter().any(|attr| {
                    attr.name.local_name == "refines" && creator_ids.contains(&attr.value)
                }) && attributes
                    .iter()
                    .any(|attr| attr.name.local_name == "property" && attr.value == "file-as")
                {
                    refines_found = true;
                }
            }
            Ok(XmlEvent::Characters(value)) => {
                if refines_found == true {
                    refines_entries.push(value);
                    refines_found = false;
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

    if refines_entries.len() == 1 {
        return Some(refines_entries.remove(0));
    } else if refines_entries.len() >= 2 {
        return Some(refines_entries.join(" & "));
    }

    None
}

struct BookEntry {
    id: i32,
    filepath: String,
}

fn fix_firstauthor(tx: &Transaction) -> i32 {
    let mut authors_fixed = 0;

    // Get book ids from entries where we have something like "firstname lastname" in author
    // but no "lastname, firstname" in fistauthor
    // Get also book ids from the special case where we have multiple authors (separated by ", " in authors)
    // but no ampersand ("&") in firstauthor
    let mut stmt = tx.prepare(r"
        SELECT files.book_id, folders.name, files.filename 
          FROM files INNER JOIN folders
            ON files.folder_id = folders.id
          WHERE files.book_id IN 
            (
              SELECT DISTINCT id FROM books_impl 
                WHERE (ext LIKE 'epub' AND author LIKE '% %' AND (firstauthor NOT LIKE '%\,%' ESCAPE '\' OR firstauthor LIKE '%&amp;%'))
                  OR (ext LIKE 'epub' AND author LIKE '%\, %' ESCAPE '\' AND firstauthor NOT LIKE '%&%')
            )
            AND files.storageid = 1
        ;").unwrap();

    let mut rows = stmt.query(NO_PARAMS).unwrap();
    let mut bookentries = Vec::new();

    while let Some(row) = rows.next().unwrap() {
        let book_id: i32 = row.get(0).unwrap();
        let prefix: String = row.get(1).unwrap();
        let filename: String = row.get(2).unwrap();
        let filepath = format!("{}/{}", prefix, filename);
        bookentries.push(BookEntry {
            id: book_id,
            filepath,
        });
    }

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
                authors_fixed = authors_fixed + 1;
            }
        }
    }

    authors_fixed
}

fn main() {
    let mut conn = Connection::open("/mnt/ext1/system/explorer-3/explorer-3.db").unwrap();

    let tx = conn.transaction().unwrap();
    let authors_fixed = fix_firstauthor(&tx);
    tx.commit().unwrap();

    if cfg!(target_arch = "arm") {
        if authors_fixed == 0 {
            pocketbook::dialog(
                pocketbook::Icon::Info,
                "The database seems to be ok.\nNothing had to be fixed.",
            );
        } else {
            pocketbook::dialog(
                pocketbook::Icon::Info,
                &format!("Authors fixed: {}", &authors_fixed),
            );
        }
    }
}
