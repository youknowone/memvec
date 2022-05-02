use crate::memory::Memory;
use core::ops::{Deref, DerefMut};
use memmap2::{MmapMut, MmapOptions};
use std::fs::File;

pub struct MmapFile<'a> {
    options: MmapOptions,
    mmap: MmapMut,
    len: &'a mut usize,
    file: File,
}

impl<'a> MmapFile<'a> {
    pub fn new(file: File, len: &'a mut usize, data_options: MmapOptions) -> std::io::Result<Self> {
        let mmap = unsafe { data_options.map_mut(&file) }?;
        Ok(Self {
            options: data_options,
            mmap,
            len,
            file,
        })
    }

    pub fn into_file(self) -> File {
        self.file
    }
}

impl<'a> core::fmt::Debug for MmapFile<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MmapFile")
            .field("options", &self.options)
            .field("len", &self.len)
            .field("file", &self.file)
            .finish()
    }
}

impl<'a> core::ops::Deref for MmapFile<'a> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.mmap.deref()
    }
}

impl<'a> core::ops::DerefMut for MmapFile<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.mmap.deref_mut()
    }
}

impl<'a> Memory for MmapFile<'a>
where
    Self: Deref<Target = [u8]> + DerefMut<Target = [u8]>,
{
    type Error = std::io::Error;

    fn as_ptr(&self) -> *const u8 {
        self.mmap.as_ptr()
    }
    fn as_mut_ptr(&mut self) -> *mut u8 {
        self.mmap.as_mut_ptr()
    }

    fn len(&self) -> usize {
        *self.len
    }

    fn len_mut(&mut self) -> &mut usize {
        self.len
    }

    fn reserve(&mut self, capacity: usize) -> std::io::Result<()> {
        // use std::io::Write;

        let additional_cap = capacity.wrapping_sub(self.mmap.len());
        if (additional_cap as isize) < 0 {
            return Ok(());
        }
        // self.file.flush().unwrap();
        let bytes_len = self.file.metadata()?.len() + additional_cap as u64;
        // eprintln!("new cap requested {} current {} gap {} total {}", capacity, self.deref().len(), additional_cap, bytes_len);
        self.file.set_len(bytes_len)?;
        assert_eq!(bytes_len, self.file.metadata()?.len());
        self.mmap = unsafe { self.options.map_mut(&self.file)? };
        Ok(())
    }

    fn shrink(&mut self, capacity: usize) -> Result<(), Self::Error> {
        let redundant_cap = self.mmap.len().wrapping_sub(capacity);
        if (redundant_cap as isize) < 0 {
            return Ok(());
        }
        let bytes_len = self.file.metadata()?.len() - redundant_cap as u64;
        self.file.set_len(bytes_len)?;
        self.mmap = unsafe { self.options.map_mut(&self.file)? };
        Ok(())
    }
}

pub struct VecFile<'a> {
    mmap_file: MmapFile<'a>,
    #[allow(dead_code)]
    len_mmap: MmapMut,
}

impl<'a> core::fmt::Debug for VecFile<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VecFile")
            .field("mmap_file", &self.mmap_file)
            .finish()
    }
}

impl<'a> VecFile<'a> {
    pub fn from_path(path: impl AsRef<std::path::Path>) -> std::io::Result<Self> {
        let file = File::options()
            .read(true)
            .append(true)
            .create(true)
            .open(path.as_ref())?;
        Self::from_file(file)
    }
    pub fn from_file(file: File) -> std::io::Result<Self> {
        const HEADER_LEN: u64 = core::mem::size_of::<u64>() as u64;

        let need_init = file.metadata()?.len() == 0;
        if need_init {
            file.set_len(HEADER_LEN)?;
        };
        let mut len_options = MmapOptions::new();
        len_options.len(HEADER_LEN as usize);

        let len_mmap = unsafe { len_options.map_mut(&file) }?;
        let len = {
            let (prefix, body, suffix) = unsafe { len_mmap.deref().align_to::<u64>() };
            assert_eq!(prefix.len(), 0);
            assert_eq!(suffix.len(), 0);
            assert_eq!(body.len(), 1);
            unsafe { &mut *(body.as_ptr() as *mut usize) }
        };

        let mut data_options = MmapOptions::new();
        data_options.offset(HEADER_LEN);

        let mut mmap_file = MmapFile::new(file, len, data_options)?;
        if need_init {
            *mmap_file.len_mut() = 0;
        }
        Ok(Self {
            mmap_file,
            len_mmap,
        })
    }

    pub fn into_file(self) -> File {
        self.mmap_file.into_file()
    }
}

impl<'a> core::ops::Deref for VecFile<'a> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.mmap_file.deref()
    }
}

impl<'a> core::ops::DerefMut for VecFile<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.mmap_file.deref_mut()
    }
}

impl<'a> Memory for VecFile<'a>
where
    Self: Deref<Target = [u8]> + DerefMut<Target = [u8]>,
{
    type Error = std::io::Error;

    fn as_ptr(&self) -> *const u8 {
        self.mmap_file.as_ptr()
    }
    fn as_mut_ptr(&mut self) -> *mut u8 {
        self.mmap_file.as_mut_ptr()
    }

    fn len(&self) -> usize {
        self.mmap_file.len()
    }

    fn len_mut(&mut self) -> &mut usize {
        self.mmap_file.len_mut()
    }

    fn reserve(&mut self, capacity: usize) -> std::io::Result<()> {
        self.mmap_file.reserve(capacity)
    }

    fn shrink(&mut self, capacity: usize) -> Result<(), Self::Error> {
        self.mmap_file.shrink(capacity)
    }
}
