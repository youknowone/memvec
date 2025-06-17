use crate::*;
use memmap2::MmapOptions;
use std::{fs::File, io::Write};

trait Record: Sized + Copy {
    fn new(id: usize) -> Self;
    fn validate(&self, id: usize) -> bool;
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct Record41 {
    id: u8,
    a: u64,
    text: [u8; 32],
}
static_assertions::assert_eq_size!(Record41, [u8; 41]); // unpleasant size

impl Record for Record41 {
    fn new(id: usize) -> Self {
        let str = format!("FIELD: {}", id);
        let mut text = [0; 32];
        text[..str.len()].copy_from_slice(str.as_bytes());
        Self {
            id: id as u8,
            a: 9,
            text,
        }
    }

    fn validate(&self, id: usize) -> bool {
        let s = core::str::from_utf8(self.text.as_slice()).unwrap();
        self.id == id as u8 && self.a == 9 && s.starts_with(&format!("FIELD: {}", id))
    }
}

#[test]
fn mmap_file() {
    let mut path = std::env::temp_dir();
    path.push("mmap.memvec");

    let _ = std::fs::remove_file(&path);

    eprintln!("path: {path:?}");

    let _ = std::fs::remove_file(&path);

    let file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .open(&path)
        .expect("file failed");
    file.set_len(17).unwrap();

    let mut len: usize = 0;
    let mut data_options = MmapOptions::new();
    data_options.offset(17); // random header
    let mmap = MmapFile::new(file, &mut len, data_options.clone()).expect("mmap failed");

    let mut vec = unsafe { mmap.try_into_memvec::<Record41>() }.unwrap();
    memvec_push10(&mut vec);

    let mut file = vec.into_mem().into_file();
    file.flush().expect("flush failed");

    let mmap = MmapFile::new(file, &mut len, data_options).expect("mmap failed");
    let mut vec = unsafe { mmap.try_into_memvec::<Record41>() }.unwrap();
    memvec_check10(&vec);
    vec.reserve(15);
    memvec_shrink10(&mut vec);
    drop(vec);

    std::fs::remove_file(path).expect("delete fail");
}

#[test]
/// Tests file-backed memory-mapped vector operations using `VecFile` and `MemVec<Record41>`.
///
/// This test creates or opens a file-backed vector, pushes 10 records, flushes and reopens the file,
/// validates the records, manages capacity, and ensures proper cleanup by deleting the file. Multiple
/// access patterns are exercised to verify correct persistence and memory mapping behavior.
fn memvec_file() {
    let mut path = std::env::temp_dir();
    path.push("memvec.memvec");

    let _ = std::fs::remove_file(&path);

    eprintln!("path: {path:?}");

    let mut file = {
        let vec_file = VecFile::open_or_create(&path, |_v| Ok(())).expect("mmap failed");
        let mut vec = unsafe { vec_file.try_into_memvec::<Record41>() }.unwrap();
        memvec_push10(&mut vec);
        vec.into_mem().into_file()
    };
    file.flush().expect("flush failed");

    {
        let vec_file = VecFile::from_file(file).expect("mmap failed");
        let mut vec = unsafe { vec_file.try_into_memvec::<Record41>() }.unwrap();
        memvec_check10(&vec);
        vec.reserve(15);
        memvec_shrink10(&mut vec);
    }

    {
        let vec_file = VecFile::open(&path).expect("mmap failed");
        let mut vec = unsafe { MemVec::<Record41, _>::try_from_memory(vec_file) }.unwrap();
        memvec_check10(&vec);
        vec.reserve(15);
        memvec_shrink10(&mut vec);
    }

    std::fs::remove_file(path).expect("delete fail");
}

#[test]
fn mmap_anon() {
    let mmap = MmapAnon::with_size(0).expect("mmap anon failed");
    let mut vec = unsafe { mmap.try_into_memvec::<Record41>() }.unwrap();
    memvec_push10(&mut vec);
    memvec_check10(&vec);
    vec.reserve(15);
    memvec_shrink10(&mut vec);
}

#[test]
/// Tests creating a memory-mapped vector from an anonymous memory map with a specified size.
///
/// Creates an anonymous memory map of 500 bytes, converts it into a `MemVec<Record41>`, pushes 10 records, validates them, reserves additional capacity, and shrinks the vector to fit.
///
/// # Examples
///
/// ```
/// mmap_anon_with_size();
/// ```
fn mmap_anon_with_size() {
    let mmap = MmapAnon::with_size(500).expect("mmap anon failed");
    let mut vec = unsafe { mmap.try_into_memvec::<Record41>() }.unwrap();
    assert!(vec.capacity() >= 10);

    for i in 0..10 {
        vec.push(Record41::new(i));
    }
    assert!(vec.capacity() > 0);

    memvec_check10(&vec);
    vec.reserve(15);
    memvec_shrink10(&mut vec);
}

#[test]
/// Tests creating an anonymous memory map with custom options and verifies vector operations.
///
/// This test configures a `MmapOptions` instance (enabling population on Linux), creates an anonymous memory map with those options, converts it into a `MemVec<Record41>`, and exercises push, validation, reserve, and shrink operations.
fn mmap_anon_with_options() {
    #[allow(unused_mut)]
    let mut options = MmapOptions::new();
    #[cfg(target_os = "linux")]
    options.populate();

    let mmap = MmapAnon::with_options(options).expect("mmap anon failed");
    let mut vec = unsafe { mmap.try_into_memvec::<Record41>() }.unwrap();

    memvec_push10(&mut vec);

    memvec_check10(&vec);
    vec.reserve(15);
    memvec_shrink10(&mut vec);
}

/// Pushes 10 new records with sequential IDs into the provided memory-backed vector.
///
/// The function asserts that the vector initially has zero capacity, then pushes records with IDs 0 through 9, and finally asserts that the capacity has increased.
///
/// # Examples
///
/// ```
/// let mut vec = MemVec::<Record41, MmapAnon>::new();
/// memvec_push10(&mut vec);
/// assert_eq!(vec.len(), 10);
/// ```
fn memvec_push10<T: Record, A: Memory>(vec: &mut MemVec<T, A>) {
    assert_eq!(vec.capacity(), 0);
    for i in 0..10 {
        vec.push(T::new(i));
    }
    assert!(vec.capacity() > 0);
}

fn memvec_check10<T: Record, A: Memory>(vec: &MemVec<T, A>) {
    assert_eq!(vec.len(), 10);
    for i in 0..10 {
        vec[i].validate(i);
    }
    for (i, item) in vec.iter().enumerate() {
        assert!(item.validate(i));
    }
    for _ in vec {}
}

fn memvec_shrink10<T: Record, A: Memory>(vec: &mut MemVec<T, A>) {
    assert_eq!(vec.len(), 10);
    assert!(vec.capacity() > 10);

    vec.shrink_to_fit();
    assert_eq!(vec.capacity(), 10);
}
