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

    pub fn file(&self) -> &File {
        &self.file
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
        let additional_cap = capacity.wrapping_sub(self.mmap.len());
        if (additional_cap as isize) < 0 {
            return Ok(());
        }
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
        #[cfg(windows)]
        {
            self.mmap = MmapOptions::new().len(0).map_anon()?;

            let set_len_result = self.file.set_len(bytes_len);
            self.mmap = unsafe { self.options.map_mut(&self.file).expect("mmap is broken") };
            set_len_result?;
        }
        #[cfg(not(windows))]
        {
            self.file.set_len(bytes_len)?;
            self.mmap = unsafe { self.options.map_mut(&self.file)? };
        }
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
    const HEADER_LEN: usize = core::mem::size_of::<u64>();

    pub fn open_or_create(path: impl AsRef<std::path::Path>) -> std::io::Result<Self> {
        let path = path.as_ref();
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;

        let need_init = file.metadata()?.len() == 0;
        if need_init {
            Self::clear(&file)?;
        };

        Self::from_file(file)
    }

    pub fn open(path: impl AsRef<std::path::Path>) -> std::io::Result<Self> {
        let path = path.as_ref();
        let file = File::options().read(true).write(true).open(path)?;
        Self::from_file(file)
    }

    /// Set header and the value of len to 0
    pub fn clear(file: &File) -> std::io::Result<()> {
        assert_eq!(0, file.metadata()?.len());
        file.set_len(Self::HEADER_LEN as u64)?;
        let len_mmap = Self::_len_mmap(file)?;
        let len = unsafe { &mut *(len_mmap.deref().as_ptr() as *mut usize) };
        *len = 0;
        Ok(())
    }

    pub fn from_file(file: File) -> std::io::Result<Self> {
        let len_mmap = Self::_len_mmap(&file)?;
        let len = unsafe { &mut *(len_mmap.deref().as_ptr() as *mut usize) };

        let mut data_options = MmapOptions::new();
        data_options.offset(Self::HEADER_LEN as u64);

        let mmap_file = MmapFile::new(file, len, data_options)?;
        Ok(Self {
            mmap_file,
            len_mmap,
        })
    }

    fn _len_mmap(file: &File) -> std::io::Result<MmapMut> {
        let mut len_options = MmapOptions::new();
        len_options.len(Self::HEADER_LEN);
        assert!(file.metadata()?.len() >= Self::HEADER_LEN as u64);
        let len_mmap = unsafe { len_options.map_mut(file) }?;
        {
            // validation
            let (prefix, body, suffix) = unsafe { len_mmap.deref().align_to::<u64>() };
            assert_eq!(prefix.len(), 0);
            assert_eq!(suffix.len(), 0);
            assert_eq!(body.len(), 1);
        }
        Ok(len_mmap)
    }

    pub fn into_file(self) -> File {
        self.mmap_file.into_file()
    }

    pub fn file(&self) -> &File {
        self.mmap_file.file()
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

    #[cfg(not(windows))]
    fn shrink(&mut self, capacity: usize) -> Result<(), Self::Error> {
        self.mmap_file.shrink(capacity)
    }

    #[cfg(windows)]
    fn shrink(&mut self, capacity: usize) -> Result<(), Self::Error> {
        self.len_mmap = MmapOptions::new().len(0).map_anon()?;
        let shrink_result = self.mmap_file.shrink(capacity);
        self.len_mmap = Self::_len_mmap(self.file()).expect("broken mmap");
        let remapped_len = self.len_mmap.deref().as_ptr() as *mut usize;
        self.mmap_file.len = unsafe { &mut *remapped_len };
        shrink_result
    }
}
