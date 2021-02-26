mod database;
mod epub;
mod pocketbook;

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

    let stat = database::fix_db_entries();

    if cfg!(target_arch = "arm") {
        if stat.anything_fixed() == false {
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
                    "Authors fixed: {}\n\
                    Sorting fixed: {}\n\
                    Genres fixed:  {}\n\
                    Series fixed:  {}\n\
                    Books cleaned from DB: {}",
                    &stat.authors_fixed,
                    &stat.sorting_fixed,
                    &stat.genres_fixed,
                    &stat.series_fixed,
                    &stat.ghost_books_cleaned
                ),
                &["OK"],
            );
        }
    } else {
        println!(
            "Authors fixed: {}\n\
            Sorting fixed: {}\n\
            Genres fixed:  {}\n\
            Series fixed:  {}\n\
            Books cleaned from DB: {}",
            &stat.authors_fixed,
            &stat.sorting_fixed,
            &stat.genres_fixed,
            &stat.series_fixed,
            &stat.ghost_books_cleaned
        );
    }
}
