mod epub;
mod pocketbook;

use rusqlite::{named_params, Connection, Transaction, NO_PARAMS};
use std::usize;

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

        if let Some(epub_metadata) = epub::get_epub_metadata(&entry.filepath) {
            let authors = epub_metadata
                .authors
                .iter()
                .filter(|aut| aut.firstauthor.len() > 0)
                .collect::<Vec<_>>();

            // Fix firstauthor…
            let firstauthors = authors
                .iter()
                .map(|aut| aut.firstauthor.clone())
                .collect::<Vec<_>>();
            if !firstauthors.iter().all(|s| entry.firstauthor.contains(s)) {
                let mut stmt = tx
                    .prepare("UPDATE books_impl SET firstauthor = :file_as WHERE id = :book_id")
                    .unwrap();
                stmt.execute_named(
                    named_params![":file_as": firstauthors.join(" & "), ":book_id": entry.id],
                )
                .unwrap();
                stat.authors_fixed = stat.authors_fixed + 1;
            }

            // Fix author names…
            let authornames = authors
                .iter()
                .map(|aut| aut.name.clone())
                .collect::<Vec<_>>();
            if !authornames.iter().all(|s| entry.author.contains(s)) {
                let mut stmt = tx
                    .prepare("UPDATE books_impl SET author = :authors WHERE id = :book_id")
                    .unwrap();
                stmt.execute_named(
                    named_params![":authors": authornames.join(", "), ":book_id": entry.id],
                )
                .unwrap();
                stat.authors_fixed = stat.authors_fixed + 1;
            }

            if entry.genre.is_empty() && epub_metadata.genre.len() > 0 {
                let mut stmt = tx
                    .prepare(r#"INSERT INTO genres (name) SELECT :genre ON CONFLICT DO NOTHING"#)
                    .unwrap();
                stmt.execute_named(named_params![":genre": &epub_metadata.genre])
                    .unwrap();
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
                stmt.execute_named(
                    named_params![":bookid": &entry.id, ":genre": &epub_metadata.genre],
                )
                .unwrap();
                stat.genres_fixed = stat.genres_fixed + 1;
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
