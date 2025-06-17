use crate::MemVec;

#[allow(clippy::len_without_is_empty)]
pub trait Memory
where
    Self: core::ops::Deref<Target = [u8]> + core::ops::DerefMut<Target = [u8]>,
{
    type Error: core::fmt::Debug;

    /// Returns a raw pointer to the memory's buffer.
    fn as_ptr(&self) -> *const u8;

    /// Returns an unsafe mutable pointer to the memory's buffer.
    fn as_mut_ptr(&mut self) -> *mut u8;

    /// Returns the number of elements in the memory, also referred to as its 'length'.
    fn len(&self) -> usize;

    /// Returns a mutable reference to the length field for direct manipulation.
    /// This method is used when the length of Vec elements changes, such as with set_len.
    fn len_mut(&mut self) -> &mut usize;

    /// Reserves capacity for at least `capacity` bytes of data.
    ///
    /// The allocator may reserve more space to speculatively avoid frequent allocations.
    /// After calling `reserve`, capacity will be greater than or equal to `capacity`.
    /// Does nothing if capacity is already sufficient.
    fn reserve(&mut self, capacity: usize) -> Result<(), Self::Error>;

    /// Shrinks the capacity of the memory as much as possible.
    ///
    /// It will drop down as close as possible to the given capacity, but the allocator
    /// may still inform the memory that there is space for a few more elements.
    fn shrink_to(&mut self, capacity: usize) -> Result<(), Self::Error>;

    /// Create a MemVec object with memory.
    /// # Safety
    /// Attempts to convert the memory into a `MemVec` of type `T`.
    ///
    /// # Safety
    ///
    /// The memory must contain a valid representation of elements of type `T`, with correct alignment and length. If these conditions are not met, the behavior is undefined.
    ///
    /// Returns a `MemVec` if the conversion succeeds, or the original memory and a `MemoryLayoutError` if the layout is invalid (e.g., misaligned memory or capacity exceeded).
    ///
    /// # Errors
    ///
    /// Returns `Err((self, MemoryLayoutError))` if the memory is not properly aligned for `T` or if the logical length exceeds the memory's capacity.
    ///
    /// # Examples
    ///
    /// ```
    /// # use your_crate::{Memory, MemVec, MemoryLayoutError};
    /// // Assume `mem` implements `Memory`
    /// let result = unsafe { mem.try_into_memvec::<u32>() };
    /// match result {
    ///     Ok(memvec) => { /* use memvec */ }
    ///     Err((original_mem, err)) => { /* handle error */ }
    /// }
    /// ```
    unsafe fn try_into_memvec<'a, T: Copy>(
        self,
    ) -> Result<MemVec<'a, T, Self>, (Self, MemoryLayoutError)>
    where
        Self: Sized,
    {
        MemVec::try_from_memory(self)
    }
}

/// Errors that can occur when converting memory into a `MemVec`.
///
/// These errors indicate fundamental incompatibilities between the memory layout
/// and the target type `T`. When these errors occur, the original memory is returned
/// unchanged, allowing for recovery or alternative handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryLayoutError {
    /// The memory is not properly aligned for the target type `T`.
    ///
    /// This occurs when the memory address is not aligned to the alignment
    /// requirements of type `T`. Common causes include memory-mapped files
    /// that don't start on proper boundaries or custom memory implementations
    /// with incorrect alignment.
    MisalignedMemory,

    /// The logical length stored in memory exceeds the available capacity.
    ///
    /// This occurs when the memory claims to contain more elements than can
    /// physically fit in the allocated space. Common causes include corrupted
    /// metadata in file-backed storage, version incompatibility, or bugs in
    /// length management.
    CapacityExceeded,
}

impl core::fmt::Display for MemoryLayoutError {
    /// Formats the `MemoryLayoutError` as a human-readable message.
    ///
    /// This implementation provides descriptive error messages for each variant of `MemoryLayoutError` when used with formatting macros.
    ///
    /// # Examples
    ///
    /// ```
    /// use your_crate::MemoryLayoutError;
    /// assert_eq!(format!("{}", MemoryLayoutError::MisalignedMemory), "memory is not properly aligned for the target type");
    /// assert_eq!(format!("{}", MemoryLayoutError::CapacityExceeded), "logical length exceeds available memory capacity");
    /// ```
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            MemoryLayoutError::MisalignedMemory => {
                write!(f, "memory is not properly aligned for the target type")
            }
            MemoryLayoutError::CapacityExceeded => {
                write!(f, "logical length exceeds available memory capacity")
            }
        }
    }
}

impl core::error::Error for MemoryLayoutError {}
