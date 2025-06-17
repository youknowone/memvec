//! # MemVec - Memory-Mapped Vectors
//!
//! A high-performance library that provides `Vec`-like interface over memory-mapped storage.
//! MemVec bridges the gap between standard Rust vectors and persistent or high-performance
//! memory backends through a clean, pluggable architecture.
//!
//! ## Core Architecture
//!
//! MemVec is built around three key concepts:
//!
//! ### 1. Frontend: MemVec
//! [`MemVec<T, A>`] is the main interface that provides all the familiar `Vec<T>` methods
//! you know and love - `push()`, `pop()`, `insert()`, `remove()`, indexing, iteration, etc.
//! It acts as a frontend that wraps any memory backend.
//!
//! ### 2. Backend: Memory Trait
//! Any type implementing the [`Memory`] trait can serve as a backend for `MemVec`.
//! This pluggable design allows you to choose the right storage strategy for your needs
//! or implement custom backends.
//!
//! ### 3. Built-in Backends
//! The library provides several ready-to-use backends:
//!
//! - **[`VecFile`]** - File-backed storage with automatic persistence
//! - **[`MmapAnon`]** - Anonymous memory mapping for high-performance temporary storage  
//! - **[`MmapFile`]** - Low-level file mapping for advanced use cases
//!
//! ## Performance Benefits
//!
//! - **Zero-copy operations** - Direct memory access without serialization
//! - **Memory-mapped I/O** - Let the OS handle efficient data loading
//! - **Familiar API** - No learning curve if you know `Vec<T>`

mod mem_vec;
mod memory;
mod mmap;

#[cfg(test)]
mod tests;

pub use mem_vec::MemVec;
pub use memory::{Memory, MemoryLayoutError};
pub use mmap::{MmapAnon, MmapFile, VecFile};
