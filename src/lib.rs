mod mem_vec;
mod mmap;

#[cfg(test)]
mod tests;

pub use mem_vec::{MemVec, Memory};
pub use mmap::{MmapFile, VecFile};
