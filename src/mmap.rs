use crate::mem_vec::Memory;
use memmap2::*;
use std::{
    fs::File,
    ops::{Deref, DerefMut},
};

pub struct MmapFile<'a, T> {
    options: MmapOptions,
    mmap: MmapMut,
    len: &'a mut usize,
    file: File,
    _marker: std::marker::PhantomData<T>,
}

impl<'a, T> MmapFile<'a, T> {
    pub unsafe fn new(
        file: File,
        len: &'a mut usize,
        data_options: MmapOptions,
    ) -> std::io::Result<Self> {
        let mmap = data_options.map_mut(&file)?;
        let (prefix, _, suffix) = mmap.deref().align_to::<T>();
        assert_eq!(prefix.len(), 0);
        assert_eq!(suffix.len(), 0);
        assert!(mmap.deref().len() >= *len);
        Ok(Self {
            options: data_options,
            mmap,
            len,
            file,
            _marker: std::marker::PhantomData,
        })
    }

    pub fn into_file(self) -> File {
        self.file
    }
}

impl<'a, T> std::ops::Deref for MmapFile<'a, T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        unsafe {
            let (prefix, slice, suffix) = self.mmap.deref().align_to::<T>();
            debug_assert_eq!(prefix.len(), 0);
            debug_assert_eq!(suffix.len(), 0);
            slice
        }
    }
}

impl<'a, T> std::ops::DerefMut for MmapFile<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe {
            let (prefix, slice, suffix) = self.mmap.deref_mut().align_to_mut::<T>();
            debug_assert_eq!(prefix.len(), 0);
            debug_assert_eq!(suffix.len(), 0);
            slice
        }
    }
}

impl<'a, T> Memory<T> for MmapFile<'a, T>
where
    Self: Deref<Target = [T]> + DerefMut<Target = [T]>,
{
    type Err = std::io::Error;

    fn len(&self) -> usize {
        *self.len
    }

    fn len_mut(&mut self) -> &mut usize {
        self.len
    }

    fn reserve(&mut self, capacity: usize) -> std::io::Result<()> {
        let additional_cap = capacity.wrapping_sub(self.deref().len());
        if (additional_cap as isize) < 0 {
            return Ok(());
        }
        let additional_bytes = additional_cap * std::mem::size_of::<T>();
        let bytes_len = self.file.metadata()?.len() + additional_bytes as u64;
        self.file.set_len(bytes_len)?;
        self.mmap = unsafe { self.options.map_mut(&self.file)? };
        Ok(())
    }

    fn shrink(&mut self, capacity: usize) -> Result<(), Self::Err> {
        let redundant_cap = self.deref().len().wrapping_sub(capacity);
        if (redundant_cap as isize) < 0 {
            return Ok(());
        }
        let redundant_bytes = redundant_cap * std::mem::size_of::<T>();
        let bytes_len = self.file.metadata()?.len() - redundant_bytes as u64;
        self.file.set_len(bytes_len)?;
        self.mmap = unsafe { self.options.map_mut(&self.file)? };
        Ok(())
    }
}

pub struct VecFile<'a, T> {
    mmap_file: MmapFile<'a, T>,
    #[allow(dead_code)]
    len_mmap: MmapMut,
}

impl<'a, T> VecFile<'a, T> {
    pub fn new(file: File) -> std::io::Result<Self> {
        const HEADER_LEN: u64 = std::mem::size_of::<u64>() as u64;

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

impl<'a, T> std::ops::Deref for VecFile<'a, T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.mmap_file.deref()
    }
}

impl<'a, T> std::ops::DerefMut for VecFile<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.mmap_file.deref_mut()
    }
}

impl<'a, T> Memory<T> for VecFile<'a, T>
where
    Self: Deref<Target = [T]> + DerefMut<Target = [T]>,
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
