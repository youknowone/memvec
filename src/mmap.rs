use crate::mem_vec::Memory;
use core::ops::{Deref, DerefMut};
use memmap2::*;
use std::fs::File;

pub struct MmapFile<'a> {
    options: MmapOptions,
    mmap: MmapMut,
    len: &'a mut usize,
    file: File,
}

impl<'a> MmapFile<'a> {
    pub unsafe fn new(
        file: File,
        len: &'a mut usize,
        data_options: MmapOptions,
    ) -> std::io::Result<Self> {
        let mmap = data_options.map_mut(&file)?;
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
    type Err = std::io::Error;

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

    fn shrink(&mut self, capacity: usize) -> Result<(), Self::Err> {
        let redundant_cap = self.deref().len().wrapping_sub(capacity);
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

impl<'a> VecFile<'a> {
    pub fn new(file: File) -> std::io::Result<Self> {
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

        let mut mmap_file = unsafe { MmapFile::new(file, len, data_options) }?;
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
    type Err = std::io::Error;

    fn len(&self) -> usize {
        self.mmap_file.len()
    }

    fn len_mut(&mut self) -> &mut usize {
        self.mmap_file.len_mut()
    }

    fn reserve(&mut self, capacity: usize) -> std::io::Result<()> {
        self.mmap_file.reserve(capacity)
    }

    fn shrink(&mut self, capacity: usize) -> Result<(), Self::Err> {
        self.mmap_file.shrink(capacity)
    }
}
