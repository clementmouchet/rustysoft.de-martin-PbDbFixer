# PbDbFixer

## Motivation
Since Pocketbook has some problems with extracting metadata correctly from
EPUB files, this program tries fix these issues. It tries to identify
wrong database entries and fix it by reading the corresponding epub
metadata.

## Features
The app tries to fix the following issues in the database
- Correction of wrong firstauthor entries (books_impl table)
- Correction of wrong first_author_letter entries (books_impl table)
- Correction of wrong author entries (books_impl table)
- Removing deleted e-books from the database (various tables)
- Add missing genre if present in epub (genre and booktogenre tables)
- Add missing series information (books_impl table)

## Compatibility
This program is tested on a PocketBook 
- *Touch HD 3* (software version 6.1.900)
- *Inkpad 3 Pro* (software version 6.0.1067)
- *Touch Lux 4* (software version 6.0.1118)

It might work with other PocketBook devices/software versions. Please tell me if it works for you (and do make a backup of the explorer-3.db file before trying!).

## Installation and Usage
Just copy the executable file into the PocketBook's application directory. If you encounter duplicate authors or other issues (see "Features" above) in the PocketBook's library, open the applications screen and tap on the PbDbFixer icon.

---
**WARNING**:

Use at your own risk. In case of doubt it is not a mistake to make a backup of the file `/system/explorer-3/explorer-3.db` beforehand.

---

## Build
If you want to build PbDbFixer yourself, you have to have the cross compiler for ARM CPUs installed. On Arch Linux, the AUR package `arm-linux-gnueabi-gcc75-linaro-bin` does the job. Don't forget to tell `cargo` which compiler/linker it has to invoke. In my case, I had to edit `~/.cargo/config`:
```
[target.arm-unknown-linux-gnueabi]
linker = "arm-linux-gnueabi-gcc"
```
Now you can easily compile the stuff by invoking
```
cargo build --release --target=arm-unknown-linux-gnueabi
```
