# Implementation Notes

- Walkdir - Read filenames from a directory recursively
- Seahash - Fast hash function (using streaming, 1KiB chunks)
- std::io::Read - Reading chunks for files
- std::fs::File - Opening files
- std::fs::copy - Copying files
- std::fs::create_dir_all - Creating parent directories automatically

Command: `$ cpxor testpaths/Source testpaths/Modified testpaths/Out`

- Iteration 1: 67ms
- Iteration 2: 56ms
- Iteration 3: 52ms
- Iteration 4: 47ms
- Iteration 5: 37ms
