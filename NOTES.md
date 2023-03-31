# Iteration 1

- Walkdir - Read filenames from a directory recursively
- Seahash - Fast hash function (using streaming, 1KiB chunks)
- std::io::Read - Reading chunks for files
- std::fs::File - Opening files
- std::fs::copy - Copying files
- std::fs::create_dir_all - Creating parent directories automatically

## Speed

`$ cpxor testpaths/Source testpaths/Modified testpaths/Out`

- 2 Files: 67ms
