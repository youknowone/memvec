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

    eprintln!("path: {path:?}");

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

    let mut vec = unsafe { mmap.try_into_vec::<Record41>() }.unwrap();
    memvec_push10(&mut vec);

    let mut file = vec.into_mem().into_file();
    file.flush().expect("flush failed");

    let mmap = MmapFile::new(file, &mut len, data_options).expect("mmap failed");
    let mut vec = unsafe { mmap.try_into_vec::<Record41>() }.unwrap();
    memvec_check10(&vec);
    memvec_shirink10(&mut vec);

    std::fs::remove_file(path).expect("delete fail");
}

#[test]
fn memvec_file() {
    let mut path = std::env::temp_dir();
    path.push("memvec.memvec");

    eprintln!("path: {path:?}");

    let file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .open(&path)
        .expect("file failed");

    let vec_file = VecFile::new(file).expect("mmap failed");
    let mut vec = unsafe { vec_file.try_into_vec::<Record41>() }.unwrap();
    memvec_push10(&mut vec);

    let mut file = vec.into_mem().into_file();
    file.flush().expect("flush failed");

    let vec_file = VecFile::new(file).expect("mmap failed");
    let mut vec = unsafe { vec_file.try_into_vec::<Record41>() }.unwrap();
    memvec_check10(&vec);
    memvec_shirink10(&mut vec);

    std::fs::remove_file(path).expect("delete fail");
}

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

fn memvec_shirink10<T: Record, A: Memory>(vec: &mut MemVec<T, A>) {
    assert_eq!(vec.len(), 10);
    assert!(vec.capacity() > 10);

    vec.shrink_to_fit();
    assert_eq!(vec.capacity(), 10);
}
