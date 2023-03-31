use std::fs::File;
use std::hash::Hasher;
use std::io::Read;
use std::path::Path;

use seahash::SeaHasher;
use walkdir::WalkDir;

fn main() {
    let mut args = std::env::args().skip(1);

    cpxor(
        &args
            .next()
            .expect("First arg should be the first directory to compare"),
        &args
            .next()
            .expect("Second arg should be the second directory to compare"),
        &args
            .next()
            .expect("Third arg should be the output directory"),
    )
    .expect("Failed to copy files");
}

fn hash_file(buf: &mut [u8], mut file: File) -> std::io::Result<u64> {
    let mut hasher = SeaHasher::new();

    loop {
        let bytes_read = file.read(buf)?;
        if bytes_read == 0 {
            break;
        }
        hasher.write(&buf[..bytes_read]);

        if bytes_read < buf.len() {
            break;
        }
    }

    Ok(hasher.finish())
}

fn cpxor<T: AsRef<Path>, U: AsRef<Path>, V: AsRef<Path>>(
    path1: T,
    path2: U,
    out_path: V,
) -> std::io::Result<()> {
    if File::open(&out_path).is_err() {
        std::fs::create_dir_all(&out_path)?;
    }

    let mut buf = [0u8; 1024];

    for entry in WalkDir::new(&path2) {
        let entry = entry?;
        let entry_path = entry.path();
        let relative = entry_path.strip_prefix(&path2).unwrap();
        let file = File::open(&entry.path())?;
        if file.metadata()?.is_dir() {
            continue;
        }

        if !path1.as_ref().join(relative).exists() {
            cp_file_safe(out_path.as_ref(), &entry.path(), relative)?;
            continue;
        }

        let orig_file = File::open(path1.as_ref().join(relative))?;

        let h1 = hash_file(&mut buf, file)?;
        let h2 = hash_file(&mut buf, orig_file)?;

        if h1 != h2 {
            cp_file_safe(out_path.as_ref(), &entry.path(), relative)?;
        }
    }

    Ok(())
}

#[inline(always)]
fn cp_file_safe(parent: &Path, source: &Path, relative: &Path) -> std::io::Result<()> {
    let out_path = parent.join(relative);
    if let Some(parent_dir) = out_path.parent() {
        if !parent_dir.exists() {
            std::fs::create_dir_all(parent_dir)?;
        }
    }
    std::fs::copy(source, out_path)?;
    Ok(())
}
