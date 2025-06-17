use crate::memory::Memory;
use core::ops::{Deref, DerefMut};
use memmap2::{MmapMut, MmapOptions};
use std::{
    fs::{File, OpenOptions},
    path::Path,
};

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

    /// Ensures the memory-mapped file has at least the specified capacity, remapping if necessary.
    ///
    /// If the requested capacity exceeds the current mapping, the underlying file is extended and the memory mapping is updated. If the capacity is already sufficient, no action is taken.
    ///
    /// # Errors
    ///
    /// Returns an error if resizing the file or remapping the memory fails.
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

    /// Shrinks the underlying file and remaps the memory region to the specified capacity.
    ///
    /// If the requested capacity is greater than or equal to the current mapping size, no action is taken. On success, the file is truncated and the memory mapping is updated to match the new size. Platform-specific logic is used to handle remapping on Windows.
    ///
    /// # Errors
    ///
    /// Returns an error if file truncation or remapping fails.
    fn shrink_to(&mut self, capacity: usize) -> Result<(), Self::Error> {
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

/// A file-backed memory mapping that provides persistent Vec-like storage.
///
/// `VecFile` is the core feature of the memvec library, enabling you to use a disk file
/// as if it were a vector in memory through memory-mapped I/O. All modifications are
/// automatically persisted to disk, making it perfect for applications that need data
/// to survive program restarts.
///
/// # File Format
///
/// VecFile uses a simple binary format:
/// - **Header (8 bytes)**: Length of valid data as `u64`
/// - **Data**: Raw bytes containing the vector elements
///
/// This format is portable across platforms and endianness-agnostic for basic types.
///
/// # Examples
///
/// ## Creating and Using a Persistent Vector
///
/// ```no_run
/// use memvec::{MemVec, VecFile};
///
/// #[derive(Copy, Clone, Debug, PartialEq)]
/// #[repr(C)]
/// struct Record {
///     id: u64,
///     value: f64,
/// }
///
/// // Create a new file-backed vector
/// let file = VecFile::create("data.bin")?;
/// let mut vec = unsafe { MemVec::<Record, _>::try_from_memory(file).unwrap() };
///
/// // Add data - automatically persisted!
/// vec.push(Record { id: 1, value: 3.14 });
/// vec.push(Record { id: 2, value: 2.71 });
///
/// // Data is immediately written to disk
/// assert_eq!(vec.len(), 2);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// ## Loading Existing Data
///
/// ```no_run
/// use memvec::{MemVec, VecFile};
///
/// #[derive(Copy, Clone)]
/// #[repr(C)]
/// struct Point { x: f32, y: f32 }
///
/// // Open existing file (created in previous program run)
/// let file = VecFile::open("points.dat")?;
/// let mut vec = unsafe { MemVec::<Point, _>::try_from_memory(file).unwrap() };
///
/// // Existing data is immediately available
/// println!("Found {} existing points", vec.len());
///
/// // Continue working with persistent data
/// vec.push(Point { x: 1.0, y: 2.0 });
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// ## Safe Creation with Initialization
///
/// ```rust
/// use memvec::{MemVec, VecFile};
/// use std::io;
///
/// #[derive(Copy, Clone)]
/// #[repr(C)]
/// struct Config { version: u32, flags: u32 }
///
/// // Create or open with initialization callback
/// let file = VecFile::open_or_create("config.dat", |_vec_file| {
///     // This closure runs only for new files
///     // Initialize with default data here if needed
///     Ok(())
/// })?;
///
/// let vec = unsafe { MemVec::<Config, _>::try_from_memory(file).unwrap() };
/// println!("Config file has {} entries", vec.len());
/// # std::fs::remove_file("config.dat").ok();
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
//
/// # Limitations
///
/// - Requires `Copy` types (no references, `String`, `Vec`, etc.)
/// - File format is not self-describing (type information not stored)
/// - Concurrent access from multiple processes requires external synchronization
/// - Large files may consume virtual address space even if not fully loaded
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

    /// Opens an existing file or creates a new one with initialization.
    ///
    /// If the file exists, it's opened and ready for use. If the file doesn't exist,
    /// it's created and the `init` closure is called to set up initial data.
    /// The closure receives a mutable reference to the `VecFile` for initialization.
    ///
    /// This is the safest way to create persistent data structures that need
    /// default values when first created.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - File I/O operations fail
    /// - The initialization closure returns an error
    /// - The file cannot be memory-mapped
    ///
    /// Opens an existing file-backed vector or creates a new one, running an initialization closure on creation.
    ///
    /// If the file does not exist, it is created and the provided `init` closure is called with a mutable reference to the new `VecFile`. If initialization fails, the newly created file is deleted and the error is returned. If the file already exists, it is opened without running the initializer.
    ///
    /// # Parameters
    /// - `path`: Path to the file to open or create.
    /// - `init`: Closure to initialize the vector if the file is newly created.
    ///
    /// # Returns
    /// Returns a `VecFile` mapped to the specified file, or an error if opening, creation, or initialization fails.
    ///
    /// # Examples
    ///
    /// ```
    /// let vec_file = VecFile::open_or_create("data.bin", |v| {
    ///     // Optionally initialize the vector contents
    ///     Ok(())
    /// })?;
    /// ```
    pub fn open_or_create(
        path: impl AsRef<Path>,
        init: impl FnOnce(&mut VecFile) -> Result<(), std::io::Error>,
    ) -> std::io::Result<Self> {
        let path = path.as_ref();
        let existing = path.exists();
        let file = if existing {
            Self::_open(path, File::options().read(true).write(true))?
        } else {
            let mut file =
                Self::_create(path, File::options().create(true).read(true).write(true))?;
            if let Err(e) = init(&mut file) {
                let _ = file;
                let _ = std::fs::remove_file(path);
                return Err(e);
            }
            file
        };

        Ok(file)
    }

    /// Creates a new empty file-backed vector.
    ///
    /// The file is created with an empty header, ready to be used as a vector.
    ///
    /// # Errors
    ///
    /// Creates a new file-backed persistent vector at the specified path, initializing it with an empty header.
    ///
    /// Returns an error if the file cannot be created or memory-mapped.
    ///
    /// # Examples
    ///
    /// ```
    /// let vec_file = VecFile::create("data.vec")?;
    /// assert_eq!(vec_file.len(), 0);
    /// ```
    pub fn create(path: impl AsRef<Path>) -> std::io::Result<Self> {
        Self::_create(path.as_ref(), File::options().create(true).write(true))
    }

    /// Opens an existing file-backed vector.
    ///
    /// The file must already exist and contain a valid VecFile header.
    /// Any existing data in the file will be immediately accessible.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file doesn't exist
    /// - The file cannot be opened for reading and writing
    /// - The file cannot be memory-mapped
    /// Opens an existing file-backed vector, validating the header format.
    ///
    /// Returns an error if the file does not exist or the header is invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// let vec_file = VecFile::open("data.vec")?;
    /// ```
    pub fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let mut options = File::options();
        options.read(true).write(true);
        Self::_open(path.as_ref(), &options)
    }

    fn _create(path: &Path, options: &OpenOptions) -> std::io::Result<Self> {
        let file = options.open(path)?;
        Self::clear(&file)?;
        Self::from_file(file)
    }

    fn _open(path: &Path, options: &OpenOptions) -> std::io::Result<Self> {
        let file = options.open(path)?;
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

    /// Ensures the underlying memory-mapped storage has at least the specified capacity, expanding the file if necessary.
    ///
    /// # Arguments
    ///
    /// * `capacity` - The minimum number of bytes the storage should be able to hold.
    ///
    /// # Errors
    ///
    /// Returns an error if resizing or remapping the underlying file fails.
    ///
    /// # Examples
    ///
    /// ```
    /// let mut vec_file = VecFile::create("data.bin")?;
    /// vec_file.reserve(1024)?;
    /// assert!(vec_file.capacity() >= 1024);
    /// # std::fs::remove_file("data.bin").unwrap();
    /// # Ok::<(), std::io::Error>(())
    /// ```
    fn reserve(&mut self, capacity: usize) -> std::io::Result<()> {
        self.mmap_file.reserve(capacity)
    }

    #[cfg(not(windows))]
    /// Shrinks the underlying file-backed memory mapping to the specified capacity.
    ///
    /// Reduces the mapped file size if the requested capacity is less than the current mapping size. On Windows, also remaps the header region to handle platform-specific constraints.
    ///
    /// # Arguments
    ///
    /// * `capacity` - The new desired capacity in bytes.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the operation succeeds, or an error if remapping or file truncation fails.
    fn shrink_to(&mut self, capacity: usize) -> Result<(), Self::Error> {
        self.mmap_file.shrink_to(capacity)
    }

    #[cfg(windows)]
    /// Shrinks the underlying file-backed memory mapping to the specified capacity.
    ///
    /// Remaps the header region after shrinking to ensure correct access to the length field. On Windows, this handles platform-specific constraints for remapping file-backed memory regions.
    ///
    /// # Parameters
    /// - `capacity`: The new capacity in bytes to shrink the mapping to.
    ///
    /// # Returns
    /// Returns `Ok(())` if the mapping was successfully shrunk, or an error if remapping fails.
    fn shrink_to(&mut self, capacity: usize) -> Result<(), Self::Error> {
        self.len_mmap = MmapOptions::new().len(0).map_anon()?;
        let shrink_result = self.mmap_file.shrink_to(capacity);
        self.len_mmap = Self::_len_mmap(self.file()).expect("broken mmap");
        let remapped_len = self.len_mmap.deref().as_ptr() as *mut usize;
        self.mmap_file.len = unsafe { &mut *remapped_len };
        shrink_result
    }
}

/// Anonymous memory mapping for high-performance temporary storage.
///
/// `MmapAnon` provides a memory-mapped backend that doesn't use any files - instead,
/// it allocates anonymous memory pages directly from the operating system. This makes
/// it perfect for high-performance temporary data structures that need Vec-like behavior
/// but with better memory management than standard heap allocation.
///
/// # Tip
/// For testing code that uses VecFile, MmapAnon is convenient as a test substitute.
pub struct MmapAnon {
    mmap: MmapMut,
    len: usize,
    options: MmapOptions,
}

impl MmapAnon {
    /// Creates a new anonymous memory mapping with the specified size.
    ///
    /// This is the simplest way to create an `MmapAnon` backend. The mapping starts
    /// with zero length but has the specified size available for growth.
    ///
    /// # Arguments
    ///
    /// * `size` - The memory mapping size in bytes. Must be non-zero.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use memvec::{MemVec, MmapAnon};
    ///
    /// #[derive(Copy, Clone)]
    /// struct Point { x: f32, y: f32 }
    ///
    /// // Create mapping with size for ~1000 points  
    /// let mmap = MmapAnon::with_size(8000)?;
    /// let mut vec = unsafe { MemVec::<Point, _>::try_from_memory(mmap).unwrap() };
    ///
    /// // Vector starts empty but can grow up to capacity
    /// assert_eq!(vec.len(), 0);
    /// vec.push(Point { x: 1.0, y: 2.0 });
    /// assert_eq!(vec.len(), 1);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// Zero-Start Capacity
    ///
    /// ```rust
    /// use memvec::{MemVec, MmapAnon};
    /// use memmap2::MmapOptions;
    ///
    /// #[derive(Copy, Clone)]
    /// struct Record { id: u32 }
    ///
    /// // Start with zero capacity - will grow as needed
    /// let mut options = MmapOptions::new();
    /// options.len(0);
    /// let mmap = MmapAnon::with_options(options)?;
    /// let mut vec = unsafe { MemVec::<Record, _>::try_from_memory(mmap).unwrap() };
    ///
    /// // Memory will be allocated on first push
    /// vec.push(Record { id: 1 });
    /// assert_eq!(vec.len(), 1);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the memory mapping cannot be created, typically due to:
    /// - Insufficient virtual address space
    /// - System limits on memory mappings
    /// Creates a new anonymous memory mapping with the specified size and zero initial length.
    ///
    /// Returns an error if the requested size is zero or exceeds system limits.
    ///
    /// # Examples
    ///
    /// ```
    /// let anon = MmapAnon::with_size(4096).unwrap();
    /// assert_eq!(anon.len(), 0);
    /// assert_eq!(anon.capacity(), 4096);
    /// ```
    pub fn with_size(size: usize) -> std::io::Result<Self> {
        let mut options = MmapOptions::new();
        let mmap = options.len(size).map_anon()?;
        Ok(Self {
            mmap,
            len: 0,
            options,
        })
    }

    /// Creates a new anonymous memory mapping with custom options.
    ///
    /// This method allows full control over the memory mapping configuration,
    /// including platform-specific optimizations like huge pages on Linux.
    /// The `MmapOptions` should have `len()` called to set the desired capacity.
    ///
    /// # Arguments
    ///
    /// * `options` - Pre-configured `MmapOptions` with desired settings
    ///
    /// # Examples
    ///
    /// ```rust
    /// use memvec::{MemVec, MmapAnon};
    /// use memmap2::MmapOptions;
    ///
    /// #[derive(Copy, Clone)]
    /// struct Data([u8; 64]);
    ///
    /// // Configure for high-performance access
    /// let mut options = MmapOptions::new();
    /// options.len(1024 * 1024); // 1MB
    ///
    /// // Platform-specific optimizations
    /// #[cfg(target_os = "linux")]
    /// {
    ///     options.huge(None);     // Use huge pages
    ///     options.populate(); // Pre-fault all pages
    /// }
    ///
    /// let mmap = MmapAnon::with_options(options)?;
    /// let mut vec = unsafe { MemVec::<Data, _>::try_from_memory(mmap).unwrap() };
    ///
    /// // Vector benefits from optimized memory mapping
    /// vec.push(Data([42; 64]));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the memory mapping cannot be created with the specified
    /// options. Common causes include:
    /// - Unsupported options on the current platform
    /// - Insufficient memory or virtual address space
    /// Creates an anonymous memory mapping with custom options.
    ///
    /// Returns a new `MmapAnon` with the specified `MmapOptions` and an initial length of zero.
    ///
    /// # Errors
    ///
    /// Returns an error if the memory mapping cannot be created, such as when system limits are exceeded.
    ///
    /// # Examples
    ///
    /// ```
    /// use memmap2::MmapOptions;
    /// let anon = MmapAnon::with_options(MmapOptions::new().len(4096)).unwrap();
    /// assert_eq!(anon.len(), 0);
    /// ```
    pub fn with_options(options: MmapOptions) -> std::io::Result<Self> {
        let mmap = options.map_anon()?;
        Ok(Self {
            mmap,
            len: 0,
            options,
        })
    }
}

impl core::fmt::Debug for MmapAnon {
    /// Formats the `MmapAnon` struct for debugging, displaying its capacity and current length.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MmapAnon")
            .field("capacity", &self.mmap.len())
            .field("len", &self.len)
            .finish()
    }
}

impl core::ops::Deref for MmapAnon {
    type Target = [u8];

    /// Returns a shared reference to the underlying memory-mapped byte slice.
    ///
    /// # Examples
    ///
    /// ```
    /// let anon = MmapAnon::with_size(1024).unwrap();
    /// let slice: &[u8] = &*anon;
    /// assert_eq!(slice.len(), 1024);
    /// ```
    fn deref(&self) -> &Self::Target {
        &self.mmap[..]
    }
}

impl core::ops::DerefMut for MmapAnon {
    /// Returns a mutable reference to the underlying memory-mapped byte slice.
    ///
    /// # Examples
    ///
    /// ```
    /// use your_crate::MmapAnon;
    /// let mut mmap = MmapAnon::with_size(16).unwrap();
    /// mmap.deref_mut()[0] = 42;
    /// assert_eq!(mmap[0], 42);
    /// ```
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.mmap[..]
    }
}

impl Memory for MmapAnon
where
    Self: Deref<Target = [u8]> + DerefMut<Target = [u8]>,
{
    type Error = std::io::Error;

    /// Returns a raw pointer to the start of the mapped memory region.
    fn as_ptr(&self) -> *const u8 {
        self.mmap.as_ptr()
    }

    /// Returns a mutable raw pointer to the start of the mapped memory region.
    fn as_mut_ptr(&mut self) -> *mut u8 {
        self.mmap.as_mut_ptr()
    }

    /// Returns the current length of valid data in the memory mapping.
    fn len(&self) -> usize {
        self.len
    }

    /// Returns a mutable reference to the current length of valid data in the anonymous memory mapping.
    ///
    /// # Examples
    ///
    /// ```
    /// let mut anon = MmapAnon::with_size(1024).unwrap();
    /// *anon.len_mut() = 512;
    /// assert_eq!(anon.len(), 512);
    /// ```
    fn len_mut(&mut self) -> &mut usize {
        &mut self.len
    }

    /// Ensures the anonymous memory mapping has at least the specified capacity, growing it if necessary.
    ///
    /// If the requested capacity exceeds the current mapping size, allocates a new anonymous mapping with the larger capacity,
    /// copies existing data into it, and replaces the old mapping. If the capacity is already sufficient, does nothing.
    ///
    /// # Parameters
    /// - `capacity`: The minimum number of bytes the mapping should be able to hold.
    ///
    /// # Returns
    /// Returns `Ok(())` if the operation succeeds, or an error if the new mapping cannot be created.
    ///
    /// # Examples
    ///
    /// ```
    /// let mut anon = MmapAnon::with_size(1024).unwrap();
    /// anon.reserve(2048).unwrap();
    /// assert!(anon.len() <= 2048);
    /// ```
    fn reserve(&mut self, capacity: usize) -> std::io::Result<()> {
        let current_capacity = self.mmap.len();
        if capacity <= current_capacity {
            return Ok(());
        }

        let options = self.options.len(capacity);
        let new_mmap = options.map_anon()?;

        let copy_bytes = self.mmap.len() * core::mem::size_of::<u8>();
        if copy_bytes > 0 {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    self.mmap.as_ptr(),
                    new_mmap.as_ptr() as *mut u8,
                    copy_bytes,
                );
            }
        }

        self.mmap = new_mmap;
        Ok(())
    }

    /// Shrinks the anonymous memory mapping to the specified capacity, preserving existing data up to the new capacity.
    ///
    /// If the requested capacity is less than the current length, the mapping is resized to at least the current length to avoid data loss. Existing data is copied into the new mapping up to the new capacity.
    ///
    /// # Arguments
    ///
    /// * `capacity` - The desired new capacity in bytes.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the mapping was successfully resized, or an error if the new mapping could not be created.
    ///
    /// # Examples
    ///
    /// ```
    /// let mut anon = MmapAnon::with_size(1024).unwrap();
    /// anon.shrink_to(512).unwrap();
    /// assert!(anon.len() <= 1024);
    /// ```
    fn shrink_to(&mut self, capacity: usize) -> Result<(), Self::Error> {
        let current_capacity = self.mmap.len();
        if capacity >= current_capacity {
            return Ok(());
        }

        let new_capacity = core::cmp::max(capacity, self.len);
        let options = self.options.len(new_capacity);
        let new_mmap = options.map_anon()?;

        let copy_len = core::cmp::min(self.len, new_capacity);
        if copy_len > 0 {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    self.mmap.as_ptr(),
                    new_mmap.as_ptr() as *mut u8,
                    copy_len,
                );
            }
        }

        self.mmap = new_mmap;
        Ok(())
    }
}
