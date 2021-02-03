# PbDbFixer

## Motivation
Since Pocketbook has some problems with extracting metadata correctly from
EPUB files, this program tries fix these issues. It tries to identify
wrong database entries and fix it by reading the corresponding epub
metadata.

## Compatibility
This program is tested on a PocketBook 
- *Touch HD 3* (software version 6.1.900)
- *Touch Lux 4* (software version 6.0.1118)

It might work with other PocketBook devices/software versions. Please tell me, if it works for you (and do make a backup of the explorer-3.db file before trying!).

## Installation and Usage
Just copy the executable file into the PocketBook's application directory.
If you encounter duplicate authors in the PocketBook's library, open the
applications screen and tap on the PbDbFixer icon.

## Build
To be able to build PbDbFixer, you have to have the cross compiler for
ARM CPUs installed. On Arch Linux, the AUR package `arm-linux-gnueabi-gcc75-linaro-bin`
does the job. Don't forget to tell `cargo` which compiler/linker it has to
invoke. In my case, I had to edit `~/.cargo/config`:
```
[target.arm-unknown-linux-gnueabi]
linker = "arm-linux-gnueabi-gcc"
```
Now you can easily compile the stuff by invoking
```
cargo build --release --target=arm-unknown-linux-gnueabi
```
