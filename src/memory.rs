use crate::MemVec;

#[allow(clippy::len_without_is_empty)]
pub trait Memory
where
    Self: core::ops::Deref<Target = [u8]> + core::ops::DerefMut<Target = [u8]>,
{
    type Error: core::fmt::Debug;

    fn as_ptr(&self) -> *const u8;
    fn as_mut_ptr(&mut self) -> *mut u8;
    fn len(&self) -> usize;
    fn len_mut(&mut self) -> &mut usize;
    fn reserve(&mut self, capacity: usize) -> Result<(), Self::Error>;
    fn shrink(&mut self, capacity: usize) -> Result<(), Self::Error>;
    /// Create a MemVec object with memory.
    /// # Safety
    /// The memory must represent valid len and bytes representations of T.
    unsafe fn try_into_vec<'a, T: Copy>(
        self,
    ) -> Result<MemVec<'a, T, Self>, (Self, MemoryConversionError)>
    where
        Self: Sized,
    {
        MemVec::try_from_memory(self)
    }
}

#[derive(Debug)]
pub enum MemoryConversionError {
    AlignMismatch,
    SizeMismatch,
}
