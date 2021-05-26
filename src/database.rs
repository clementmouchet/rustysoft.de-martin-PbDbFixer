use rusqlite::{named_params, Connection, Transaction};

use crate::epub;

const DATABASE_FILE: &str = "/mnt/ext1/system/explorer-3/explorer-3.db";

pub struct BookEntry {
    id: i32,
    filepath: String,
    author: String,
    firstauthor: String,
    genre: String,
    first_author_letter: String,
    series: String,
}

fn get_epubs_from_database(tx: &Transaction) -> Vec<BookEntry> {
    let mut book_entries = Vec::new();

    let mut stmt = tx
        .prepare(
            r#"
    SELECT books.id, folders.name, files.filename, books.firstauthor,
      books.author, genres.name, first_author_letter, series
      FROM books_impl books JOIN files
        ON books.id = files.book_id
        JOIN folders
          ON folders.id = files.folder_id
        LEFT OUTER JOIN booktogenre btg
          ON books.id = btg.bookid
        LEFT OUTER JOIN genres
          ON genres.id = btg.genreid
      WHERE files.storageid = 1 AND books.ext = 'epub'
      ORDER BY books.id"#,
        )
        .unwrap();

    let mut rows = stmt.query([]).unwrap();

    while let Some(row) = rows.next().unwrap() {
        let book_id: i32 = row.get(0).unwrap();
        let prefix: String = row.get(1).unwrap();
        let filename: String = row.get(2).unwrap();
        let filepath = format!("{}/{}", prefix, filename);
        let firstauthor: String = row.get(3).unwrap_or_default();
        let author: String = row.get(4).unwrap_or_default();
        let genre: String = row.get(5).unwrap_or_default();
        let first_author_letter = row.get(6).unwrap_or_default();
        let series: String = row.get(7).unwrap_or_default();

        let entry = BookEntry {
            id: book_id,
            filepath,
            firstauthor,
            author,
            genre,
            first_author_letter,
            series,
        };

        book_entries.push(entry);
    }

    book_entries
}

fn remove_ghost_books_from_db(tx: &Transaction) -> usize {
    let mut stmt = tx
        .prepare(
            r#"
            DELETE FROM books_impl
            WHERE id IN (
              SELECT books.id
                FROM books_impl books
                  LEFT OUTER JOIN files
                    ON books.id = files.book_id
                WHERE files.filename is NULL
            )"#,
        )
        .unwrap();

    let num = stmt.execute([]).unwrap();

    tx.execute(
        r#"DELETE FROM books_settings WHERE bookid NOT IN ( SELECT id FROM books_impl )"#,
        [],
    )
    .unwrap();

    let version: i32 = tx
        .query_row(r#"SELECT id FROM version"#, [], |r| r.get(0))
        .unwrap();
    if version >= 37 {
        tx.execute(
            r#"DELETE FROM books_fast_hashes WHERE book_id NOT IN ( SELECT id FROM books_impl )"#,
            [],
        )
        .unwrap();
    } else {
        tx.execute(
            r#"DELETE FROM books_uids WHERE book_id NOT IN ( SELECT id FROM books_impl )"#,
            [],
        )
        .unwrap();
    }

    tx.execute(
        r#"DELETE FROM bookshelfs_books WHERE bookid NOT IN ( SELECT id FROM books_impl )"#,
        [],
    )
    .unwrap();
    tx.execute(
        r#"DELETE FROM booktogenre WHERE bookid NOT IN ( SELECT id FROM books_impl )"#,
        [],
    )
    .unwrap();
    tx.execute(
        r#"DELETE FROM social WHERE bookid NOT IN ( SELECT id FROM books_impl )"#,
        [],
    )
    .unwrap();

    num
}

pub struct Statistics {
    pub authors_fixed: i32,
    pub ghost_books_cleaned: usize,
    pub genres_fixed: usize,
    pub sorting_fixed: usize,
    pub series_fixed: usize,
}

impl Statistics {
    pub fn anything_fixed(&self) -> bool {
        &self.authors_fixed > &0
            || &self.genres_fixed > &0
            || &self.ghost_books_cleaned > &0
            || &self.sorting_fixed > &0
            || &self.series_fixed > &0
    }
}

pub fn fix_db_entries() -> Statistics {
    let mut stat = Statistics {
        authors_fixed: 0,
        ghost_books_cleaned: 0,
        genres_fixed: 0,
        sorting_fixed: 0,
        series_fixed: 0,
    };

    let mut conn = Connection::open(DATABASE_FILE).unwrap();
    conn.pragma_update(None, "foreign_keys", &0).unwrap();
    let tx = conn.transaction().unwrap();

    let book_entries = get_epubs_from_database(&tx);

    for entry in book_entries {
        if let Some(epub_metadata) = epub::get_epub_metadata(&entry.filepath) {
            // Fix firstauthor…
            let mut firstauthors = epub_metadata
                .authors
                .iter()
                .filter(|aut| aut.firstauthor.len() > 0)
                .map(|aut| aut.firstauthor.clone())
                .collect::<Vec<_>>();
            firstauthors.sort();
            if !firstauthors.iter().all(|s| entry.firstauthor.contains(s)) {
                let mut stmt = tx
                    .prepare("UPDATE books_impl SET firstauthor = :file_as WHERE id = :book_id")
                    .unwrap();
                stmt.execute(
                    named_params![":file_as": firstauthors.join(" & "), ":book_id": entry.id],
                )
                .unwrap();
                stat.authors_fixed = stat.authors_fixed + 1;
            }

            // Fix first_author_letter
            let first_author_letter = firstauthors
                .join(" & ")
                .chars()
                .next()
                .unwrap_or_default()
                .to_string()
                .to_uppercase();

            if first_author_letter != "\0" && (entry.first_author_letter != first_author_letter) {
                let mut stmt = tx
                        .prepare("UPDATE books_impl SET first_author_letter = :first_letter WHERE id = :book_id")
                        .unwrap();
                stmt.execute(
                    named_params![":first_letter": first_author_letter,":book_id": entry.id],
                )
                .unwrap();
                stat.sorting_fixed = stat.sorting_fixed + 1;
            }

            // Fix author names…
            let authornames = epub_metadata
                .authors
                .iter()
                .map(|aut| aut.name.clone())
                .collect::<Vec<_>>();
            if !authornames.iter().all(|s| entry.author.contains(s))
                || authornames.join(", ").len() != entry.author.len()
            {
                let mut stmt = tx
                    .prepare("UPDATE books_impl SET author = :authors WHERE id = :book_id")
                    .unwrap();
                stmt.execute(
                    named_params![":authors": authornames.join(", "), ":book_id": entry.id],
                )
                .unwrap();
                stat.authors_fixed = stat.authors_fixed + 1;
            }

            // Fix genre…
            if entry.genre.is_empty() && epub_metadata.genre.len() > 0 {
                let mut stmt = tx
                    .prepare(r#"INSERT INTO genres (name) SELECT :genre ON CONFLICT DO NOTHING"#)
                    .unwrap();
                stmt.execute(named_params![":genre": &epub_metadata.genre])
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
                stmt.execute(
                    named_params![":bookid": &entry.id, ":genre": &epub_metadata.genre],
                )
                .unwrap();
                stat.genres_fixed = stat.genres_fixed + 1;
            }

            // Fix series…
            if !epub_metadata.series.name.is_empty() && entry.series.is_empty() {
                let mut stmt = tx
                    .prepare("UPDATE books_impl SET series = :series, numinseries = :series_index WHERE id = :book_id")
                    .unwrap();
                stmt.execute(
                        named_params![":series": &epub_metadata.series.name, ":series_index": &epub_metadata.series.index, ":book_id": entry.id],
                    )
                    .unwrap();
                stat.series_fixed = stat.series_fixed + 1;
            }
        }
    }

    // ghost books
    let num = remove_ghost_books_from_db(&tx);
    stat.ghost_books_cleaned = num;

    tx.commit().unwrap();

    stat
}
