mod pocketbook;

use rusqlite::{named_params, Connection, Result, Transaction, NO_PARAMS};
use std::{collections::HashMap, fs::File};
use std::{error::Error, io::Read};
use std::{io::BufReader, usize};
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
                .filter(|v| v.role == "aut")
                .map(|v| v.file_as.clone())
                .collect::<Vec<String>>()
                .join(" & "),
        );
    }

    None
}

struct Creator {
    role: String,
    name: String,
}

fn get_attribute_creator(opf: ZipFile) -> Option<String> {
    let parser = ParserConfig::new()
        .trim_whitespace(true)
        .ignore_comments(true)
        .coalesce_characters(true)
        .create_reader(opf);

    let mut is_epub3 = false;
    let mut creator_found = true;
    let mut creator_ids = Vec::new();
    let mut role_found = false;
    let mut creator_entries = HashMap::new();
    let mut epub2_creator_entries = Vec::new();
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
                creator_found = true;
                if !is_epub3 {
                    match attributes
                        .iter()
                        .find(|attr| attr.name.local_name == "role")
                    {
                        Some(attr) => {
                            epub2_creator_entries.push(Creator {
                                role: attr.value.clone(),
                                name: "".to_string(),
                            });
                        }
                        None => {
                            epub2_creator_entries.push(Creator {
                                role: "aut".to_string(),
                                name: "".to_string(),
                            });
                        }
                    }
                }
                for attr in attributes {
                    if is_epub3 && attr.name.local_name == "id" {
                        creator_ids.push("#".to_owned() + attr.value.as_str());
                        //creator_entries.insert(attr.value.clone(), Creator{role: "".to_string(), name: "".to_string()});
                        curr_id = "#".to_owned() + attr.value.as_str();
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
                if creator_found && is_epub3 == false {
                    epub2_creator_entries.last_mut().unwrap().name = value.clone();
                } else if creator_found && is_epub3 == true {
                    let entry = creator_entries.entry(curr_id.clone()).or_insert(Creator {
                        role: "".to_string(),
                        name: "".to_string(),
                    });
                    entry.name = value;
                    creator_found = false;
                } else if role_found == true {
                    let entry = creator_entries.entry(curr_id.clone()).or_insert(Creator {
                        role: "".to_string(),
                        name: "".to_string(),
                    });
                    entry.role = value;
                    role_found = false;
                }
            }
            Ok(XmlEvent::StartElement { .. }) => {
                if creator_found == true {
                    creator_found = false;
                }
            }
            Err(e) => {
                println!("{}", e);
                break;
            }
            _ => {}
        }
    }

    if !is_epub3 && epub2_creator_entries.len() >= 1 {
        return Some(
            epub2_creator_entries
                .into_iter()
                .filter(|v| v.role == "aut")
                .map(|v| v.name.clone())
                .collect::<Vec<String>>()
                .join(", "),
        );
    } else if creator_entries.len() >= 1 {
        return Some(
            creator_entries
                .values()
                .into_iter()
                .filter(|v| v.role == "aut")
                .map(|v| v.name.clone())
                .collect::<Vec<String>>()
                .join(", "),
        );
    }

    None
}

fn get_attribute_genre(opf: ZipFile) -> Option<String> {
    let parser = ParserConfig::new()
        .trim_whitespace(true)
        .ignore_comments(true)
        .coalesce_characters(true)
        .create_reader(opf);

    let mut genre_found = false;

    for e in parser {
        match e {
            Ok(XmlEvent::StartElement { name, .. }) if name.local_name == "subject" => {
                genre_found = true;
            }
            Ok(XmlEvent::Characters(value)) => {
                if genre_found {
                    return Some(value);
                }
            }
            Ok(XmlEvent::StartElement { .. }) => {
                if genre_found == true {
                    genre_found = false;
                }
            }
            Err(e) => {
                println!("{}", e);
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
    author: String,
    firstauthor: String,
    has_drm: bool,
    genre: String,
}

fn get_epubs_from_database(tx: &Transaction) -> Vec<BookEntry> {
    let mut book_entries = Vec::new();

    let mut stmt = tx
        .prepare(
            r"
    SELECT books.id, folders.name, files.filename, books.firstauthor, books.author, genres.name
      FROM books_impl books JOIN files
        ON books.id = files.book_id
        JOIN folders
          ON folders.id = files.folder_id
        LEFT OUTER JOIN booktogenre btg
          ON books.id = btg.bookid
        LEFT OUTER JOIN genres
          ON genres.id = btg.genreid
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
        let firstauthor: String = row.get(3).unwrap();
        let author: String = row.get(4).unwrap();
        let has_drm = match prefix.as_str() {
            "/mnt/ext1/Digital Editions" => true,
            _ => false,
        };
        let genre: String = row.get(5).unwrap_or_default();

        let entry = BookEntry {
            id: book_id,
            filepath,
            firstauthor,
            author,
            has_drm,
            genre,
        };

        book_entries.push(entry);
    }

    book_entries
}

fn remove_ghost_books_from_db(tx: &Transaction) -> usize {
    let mut stmt = tx
        .prepare(
            r"
            DELETE FROM books_impl
            WHERE id IN (
              SELECT books.id
                FROM books_impl books
                  LEFT OUTER JOIN files
                    ON books.id = files.book_id
                WHERE files.filename is NULL
            )",
        )
        .unwrap();

    let num = stmt.execute(NO_PARAMS).unwrap();

    tx.execute(
        r"DELETE FROM books_settings WHERE bookid NOT IN ( SELECT id FROM books_impl )",
        NO_PARAMS,
    )
    .unwrap();
    tx.execute(
        r"DELETE FROM books_uids WHERE book_id NOT IN ( SELECT id FROM books_impl )",
        NO_PARAMS,
    )
    .unwrap();
    tx.execute(
        r"DELETE FROM bookshelfs_books WHERE bookid NOT IN ( SELECT id FROM books_impl )",
        NO_PARAMS,
    )
    .unwrap();
    tx.execute(
        r"DELETE FROM booktogenre WHERE bookid NOT IN ( SELECT id FROM books_impl )",
        NO_PARAMS,
    )
    .unwrap();
    tx.execute(
        r"DELETE FROM social WHERE bookid NOT IN ( SELECT id FROM books_impl )",
        NO_PARAMS,
    )
    .unwrap();

    num
}

struct Statistics {
    authors_fixed: i32,
    ghost_books_cleaned: usize,
    drm_skipped: usize,
    genres_fixed: usize,
}

fn fix_db_entries(tx: &Transaction, book_entries: &Vec<BookEntry>) -> Statistics {
    let mut stat = Statistics {
        authors_fixed: 0,
        ghost_books_cleaned: 0,
        drm_skipped: 0,
        genres_fixed: 0,
    };

    for entry in book_entries {
        if entry.has_drm {
            stat.drm_skipped = stat.drm_skipped + 1;
            continue;
        }

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
                if !file_as.split(" & ").all(|s| entry.firstauthor.contains(s)) {
                    let mut stmt = tx
                        .prepare("UPDATE books_impl SET firstauthor = :file_as WHERE id = :book_id")
                        .unwrap();
                    stmt.execute_named(named_params![":file_as": file_as, ":book_id": entry.id])
                        .unwrap();
                    stat.authors_fixed = stat.authors_fixed + 1;
                }
            }
            let opf = archive.by_name(opf_file.as_str()).unwrap();
            // author…
            if let Some(creator) = get_attribute_creator(opf) {
                if !creator.split(", ").all(|s| entry.author.contains(s))
                    || creator.len() < entry.author.len()
                {
                    let mut stmt = tx
                        .prepare("UPDATE books_impl SET author = :creator WHERE id = :book_id")
                        .unwrap();
                    stmt.execute_named(named_params![":creator": creator, ":book_id": entry.id])
                        .unwrap();
                    stat.authors_fixed = stat.authors_fixed + 1;
                }
            }
            // genre…
            if entry.genre.is_empty() {
                let opf = archive.by_name(opf_file.as_str()).unwrap();
                if let Some(genre) = get_attribute_genre(opf) {
                    let mut stmt = tx
                        .prepare(
                            r#"INSERT INTO genres (name) SELECT :genre ON CONFLICT DO NOTHING"#,
                        )
                        .unwrap();
                    stmt.execute_named(named_params![":genre": &genre]).unwrap();
                    let mut stmt = tx
                        .prepare(
                            r#"
                        INSERT INTO booktogenre (bookid, genreid)
                          VALUES (:bookid, 
                            (SELECT id FROM genres WHERE name = :genre)
                          )
                          ON CONFLICT DO NOTHING"#,
                        )
                        .unwrap();
                    stmt.execute_named(named_params![":bookid": &entry.id, ":genre": &genre])
                        .unwrap();
                    stat.genres_fixed = stat.genres_fixed + 1;
                }
            }
        }
    }

    // ghost books
    let num = remove_ghost_books_from_db(tx);
    stat.ghost_books_cleaned = num;

    stat
}

fn main() {
    if cfg!(target_arch = "arm") {
        let res = pocketbook::dialog(
            pocketbook::Icon::None,
            "PocketBook has sometimes problems parsing metadata.\n\
            This app tries to fix some of these issues.\n\
            (Note: The database file explore-3.db will be altered!)\n\
            \n\
            Please be patient - this might take a while.\n\
            You will see a blank screen during the process.\n\
            \n\
            Proceed?",
            &["Cancel", "Yes"],
        );
        if res == 1 {
            return;
        }
    }

    let mut conn = Connection::open("/mnt/ext1/system/explorer-3/explorer-3.db").unwrap();

    conn.execute("PRAGMA foreign_keys = 0", NO_PARAMS).unwrap();

    let tx = conn.transaction().unwrap();
    let book_entries = get_epubs_from_database(&tx);
    let stat = fix_db_entries(&tx, &book_entries);
    tx.commit().unwrap();

    if cfg!(target_arch = "arm") {
        if stat.authors_fixed == 0 {
            if stat.drm_skipped == 0 {
                pocketbook::dialog(
                    pocketbook::Icon::Info,
                    "The database seems to be ok.\n\
                    Nothing had to be fixed.",
                    &["OK"],
                );
            } else {
                pocketbook::dialog(
                    pocketbook::Icon::Info,
                    &format!(
                        "The database seems to be ok.\n\
                        Nothing had to be fixed.\n\
                        (Books skipped (DRM): {})",
                        &stat.drm_skipped
                    ),
                    &["OK"],
                );
            }
        } else {
            pocketbook::dialog(
                pocketbook::Icon::Info,
                &format!(
                    "Authors fixed: {}\n\
                    Genres fixed:  {}\n\
                    Books skipped (DRM):   {}\n\
                    Books cleaned from DB: {}",
                    &stat.authors_fixed,
                    &stat.genres_fixed,
                    &stat.drm_skipped,
                    &stat.ghost_books_cleaned
                ),
                &["OK"],
            );
        }
    } else {
        println!(
            "Authors fixed: {}\n\
            Genres fixed:  {}\n\
            Books skipped (DRM):   {}\n\
            Books cleaned from DB: {}",
            &stat.authors_fixed, &stat.genres_fixed, &stat.drm_skipped, &stat.ghost_books_cleaned
        );
    }
}
