mod mem_vec;
mod memory;
mod mmap;

#[cfg(test)]
mod tests;

pub use mem_vec::MemVec;
pub use memory::Memory;
pub use mmap::{MmapFile, VecFile};
