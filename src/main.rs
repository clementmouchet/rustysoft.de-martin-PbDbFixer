mod pocketbook;

use rusqlite::{named_params, Connection, Result, Transaction, NO_PARAMS};
use std::io::BufReader;
use std::{collections::HashMap, fs::File};
use std::{error::Error, io::Read};
use xml::reader::{EventReader, ParserConfig, XmlEvent};
use zip::{read::ZipFile, ZipArchive};

fn get_root_file(mut container: ZipFile) -> Result<Option<String>, Box<dyn Error>> {
    let mut buf = String::new();
    container.read_to_string(&mut buf).unwrap();

    // Get rid of the BOM mark, if any
    if buf.starts_with("\u{feff}") {
        buf = buf.strip_prefix("\u{feff}").unwrap().to_owned();
    }

    let parser = EventReader::new(BufReader::new(buf.as_bytes()));

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

struct Refine {
    role: String,
    file_as: String,
}

fn get_attribute_file_as(opf: ZipFile) -> Option<String> {
    let parser = ParserConfig::new()
        .trim_whitespace(true)
        .ignore_comments(true)
        .coalesce_characters(true)
        .create_reader(opf);

    let mut is_epub3 = false;
    let mut creator_ids = Vec::new();
    let mut refines_found = false;
    let mut role_found = false;
    let mut refine_entries = HashMap::new();
    let mut curr_id = String::new();

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
                    curr_id = attributes
                        .iter()
                        .find(|a| a.name.local_name == "refines")
                        .unwrap()
                        .value
                        .clone();
                } else if attributes.iter().any(|attr| {
                    attr.name.local_name == "refines" && creator_ids.contains(&attr.value)
                }) && attributes
                    .iter()
                    .any(|attr| attr.name.local_name == "property" && attr.value == "role")
                {
                    role_found = true;
                    curr_id = attributes
                        .iter()
                        .find(|a| a.name.local_name == "refines")
                        .unwrap()
                        .value
                        .clone();
                }
            }
            Ok(XmlEvent::Characters(value)) => {
                if role_found == true {
                    if value == "aut" {
                        let entry = refine_entries.entry(curr_id.clone()).or_insert(Refine {
                            role: "".to_string(),
                            file_as: "".to_string(),
                        });
                        entry.role = value;
                    }
                    role_found = false;
                } else if refines_found == true {
                    let entry = refine_entries.entry(curr_id.clone()).or_insert(Refine {
                        role: "".to_string(),
                        file_as: "".to_string(),
                    });
                    entry.file_as = value;
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

    if refine_entries.len() == 1 {
        return Some(refine_entries.values().next().unwrap().file_as.clone());
    } else if refine_entries.len() >= 2 {
        return Some(
            refine_entries
                .values()
                .into_iter()
                .filter(|v|v.role == "aut")
                .map(|v| v.file_as.clone())
                .collect::<Vec<String>>()
                .join(" & "),
        );
    }

    None
}

struct BookEntry {
    id: i32,
    filepath: String,
    author_sort: String,
}

fn get_epubs_from_database(tx: &Transaction) -> Vec<BookEntry> {
    let mut book_entries = Vec::new();

    let mut stmt = tx
        .prepare(
            r"
    SELECT books.id, folders.name, files.filename, books.firstauthor
      FROM books_impl books JOIN files
        ON books.id = files.book_id
        JOIN folders
          ON folders.id = files.folder_id
      WHERE files.storageid = 1 AND books.ext = 'epub'
      ORDER BY books.id",
        )
        .unwrap();

    let mut rows = stmt.query(NO_PARAMS).unwrap();

    while let Some(row) = rows.next().unwrap() {
        let book_id: i32 = row.get(0).unwrap();
        let prefix: String = row.get(1).unwrap();
        let filename: String = row.get(2).unwrap();
        let filepath = format!("{}/{}", prefix, filename);
        let author_sort: String = row.get(3).unwrap();

        let entry = BookEntry {
            id: book_id,
            filepath,
            author_sort,
        };

        book_entries.push(entry);
    }

    book_entries
}

struct Statistics {
    authors_fixed: i32,
}

fn fix_db_entries(tx: &Transaction, book_entries: &Vec<BookEntry>) -> Statistics {
    let mut stat = Statistics { authors_fixed: 0 };

    for entry in book_entries {
        let file = File::open(entry.filepath.as_str());
        let file = match file {
            Err(_) => continue,
            Ok(file) => file,
        };

        let mut archive = ZipArchive::new(BufReader::new(file)).unwrap();

        let container = archive.by_name("META-INF/container.xml").unwrap();

        if let Some(opf_file) = get_root_file(container).unwrap() {
            let opf = archive.by_name(opf_file.as_str()).unwrap();
            // firstauthor…
            if let Some(file_as) = get_attribute_file_as(opf) {
                if file_as != entry.author_sort {
                    println!("::: '{}' vs. '{}'", entry.author_sort, file_as);
                    let mut stmt = tx
                        .prepare("UPDATE books_impl SET firstauthor = :file_as WHERE id = :book_id")
                        .unwrap();
                    //stmt.execute_named(named_params![":file_as": file_as, ":book_id": entry.id])
                    //    .unwrap();
                    stat.authors_fixed = stat.authors_fixed + 1;
                }
            }
        }
    }

    stat
}

fn main() {
    let mut conn = Connection::open("/mnt/ext1/system/explorer-3/explorer-3.db").unwrap();

    let tx = conn.transaction().unwrap();
    let book_entries = get_epubs_from_database(&tx);
    let stat = fix_db_entries(&tx, &book_entries);
    tx.commit().unwrap();

    if cfg!(target_arch = "arm") {
        if stat.authors_fixed == 0 {
            pocketbook::dialog(
                pocketbook::Icon::Info,
                "The database seems to be ok.\nNothing had to be fixed.",
            );
        } else {
            pocketbook::dialog(
                pocketbook::Icon::Info,
                &format!("Authors fixed: {}", &stat.authors_fixed),
            );
        }
    } else {
        println!("Authors fixed: {}", &stat.authors_fixed);
    }
}
