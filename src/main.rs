#![feature(iter_array_chunks)]

use std::ffi::CString;
use std::fs::File;
use std::hash::Hasher;
use std::io::Read;
use std::os::fd::{FromRawFd, IntoRawFd};
use std::path::{Path, PathBuf};

use io_uring::{opcode, types, IoUring};
use nix::fcntl::{self, OFlag};
use nix::sys::stat::Mode;
use seahash::SeaHasher;
use walkdir::WalkDir;

const BUF_LEN: usize = 1024 * 8;
const RING_ENTRIES: usize = 1024;

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
    if !out_path.as_ref().exists() {
        std::fs::create_dir_all(&out_path)?;
    }

    let mut ring = IoUring::new(RING_ENTRIES as _)?;

    let mut buf = [0u8; BUF_LEN];

    let mut parent_dir = None;
    let mut modified_parent_fd = None;
    let mut source_parent_fd = None;
    let mut active_files = Vec::with_capacity(BUF_LEN);
    let mut source_files = Vec::with_capacity(BUF_LEN);
    let mut item_in_ring = 0;

    for entry in WalkDir::new(&path2) {
        let entry = entry?;
        let entry_path = entry.path();
        let relative = entry_path.strip_prefix(&path2).unwrap();

        if ring.submission().is_full() {
            ring_finish(
                path2.as_ref(),
                &mut ring,
                &mut buf,
                out_path.as_ref(),
                &active_files,
                &source_files,
            )?;
            item_in_ring = 0;
        } else if entry.file_type().is_file() {
            let file_name = entry_path.file_name().unwrap().to_str().unwrap();

            if parent_dir != entry_path.parent().map(|p| p.to_owned()) {
                parent_dir = Some(entry_path.parent().unwrap().to_owned());

                modified_parent_fd = fd(parent_dir.as_ref());
                source_parent_fd = fd(replace_prefix(parent_dir.as_ref(), &path2, &path1));
            }

            if path1.as_ref().join(relative).exists() {
                set_or_push(&mut active_files, item_in_ring, CString::new(file_name)?);
                set_or_push(&mut source_files, item_in_ring, entry.path().to_owned());

                let modified_entry_op = &opcode::OpenAt::new(
                    types::Fd(modified_parent_fd.unwrap()),
                    active_files[item_in_ring].as_ptr(),
                )
                .build()
                .user_data(0x42);

                let source_entry_op = &opcode::OpenAt::new(
                    types::Fd(source_parent_fd.unwrap()),
                    active_files[item_in_ring].as_ptr(),
                )
                .build()
                .user_data(0x43);

                unsafe {
                    ring.submission()
                        .push(&modified_entry_op)
                        .expect("submission queue is full");

                    ring.submission()
                        .push(&source_entry_op)
                        .expect("submission queue is full");
                }

                item_in_ring += 1;
            } else {
                cp_file_safe(out_path.as_ref(), &entry.path(), relative)?;
            }
        }
    }

    ring_finish(
        path2.as_ref(),
        &mut ring,
        &mut buf,
        out_path.as_ref(),
        &active_files,
        &source_files,
    )?;

    Ok(())
}

fn ring_finish(
    source_path: &Path,
    ring: &mut IoUring,
    buf: &mut [u8],
    out_path: &Path,
    active_files: &Vec<CString>,
    source_files: &Vec<PathBuf>,
) -> std::io::Result<()> {
    let l = ring.submission().len();
    ring.submit_and_wait(l).unwrap();

    for (i, [x1, x2]) in ring.completion().array_chunks().enumerate() {
        let source_file_dir = Path::new(source_files[i].to_str().unwrap());
        let relative_dir = Path::new(active_files[i].to_str().unwrap());

        unsafe {
            let h1 = hash_file(buf, File::from_raw_fd(x1.result()))?;
            let h2 = hash_file(buf, File::from_raw_fd(x2.result()))?;

            if h1 != h2 {
                cp_file_safe(
                    &replace_prefix(source_file_dir.parent(), source_path, out_path).unwrap(),
                    &source_file_dir,
                    &relative_dir,
                )?;
            }
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
    std::fs::copy(source, out_path).unwrap();
    Ok(())
}

#[inline(always)]
fn set_or_push<T>(v: &mut Vec<T>, index: usize, value: T) {
    if v.len() == index {
        v.push(value);
    } else {
        v[index] = value;
    }
}

#[inline(always)]
fn replace_prefix<T: AsRef<Path>, U: AsRef<Path>, V: AsRef<Path>>(
    src: Option<T>,
    original: U,
    new: V,
) -> Option<PathBuf> {
    src.map(|s| {
        new.as_ref()
            .join(s.as_ref().strip_prefix(&original).unwrap())
    })
}

#[inline(always)]
fn fd<P: AsRef<Path>>(path: Option<P>) -> Option<i32> {
    path.map(|p| {
        fcntl::open(
            p.as_ref(),
            OFlag::O_DIRECTORY | OFlag::O_RDONLY,
            Mode::empty(),
        )
        .unwrap()
        .into_raw_fd()
    })
}
