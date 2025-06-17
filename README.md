# MemVec

> **Memory-backed vectors that persist beyond your program's lifetime**

[![Crates.io](https://img.shields.io/crates/v/memvec.svg)](https://crates.io/crates/memvec)
[![Documentation](https://docs.rs/memvec/badge.svg)](https://docs.rs/memvec)
[![License](https://img.shields.io/badge/license-BSD--2--Clause-blue.svg)](LICENSE)

**MemVec** gives you the familiar `Vec<T>` interface with the power of memory-mapped storage.

## ✨ Why MemVec?

- **🔥 Zero-copy performance** - Direct memory access, no serialization overhead
- **🎯 Drop-in Vec replacement** - Familiar `push()`, `pop()`, `insert()`, `remove()` API
- **🔧 Flexible backends** - File-backed storage or anonymous memory mapping

## 🚄 Quick Start

### File-backed persistence (survive restarts!)

```rust
use memvec::{MemVec, VecFile};

#[derive(Copy, Clone)]
#[repr(C, packed)]
struct LogEntry {
    timestamp: u64,
    level: u8,
    message: [u8; 128],
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = VecFile::open("app.log")?;
    let mut log = unsafe { MemVec::<LogEntry, _>::try_from_memory(file)? };

    // Append new entries (persisted immediately!)
    log.push(LogEntry {
        timestamp: 1640995200,
        level: 1, // INFO
        message: *b"Server started successfully\0\0\0\0\0...", // pad to 128 bytes
    });

    // Read back all entries (even from previous runs!)
    println!("Total log entries: {}", log.len());
    for entry in log.iter() {
        println!("Time: {}, Level: {}", entry.timestamp, entry.level);
    }

    Ok(())
}
```

### High-performance in-memory operations

```rust
use memvec::{MemVec, MmapAnon};

#[derive(Copy, Clone, Debug)]
#[repr(C)]
struct Point3D {
    x: f64,
    y: f64,
    z: f64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create anonymous memory mapping (no file, just fast memory)
    let mmap = MmapAnon::with_size(32 * 1_000_000)?; // 32MB for 1M points
    let mut points = unsafe { MemVec::<Point3D, _>::try_from_memory(mmap)? };

    // Generate and store 1 million 3D points
    for i in 0..1_000_000 {
        points.push(Point3D {
            x: (i as f64).sin(),
            y: (i as f64).cos(),
            z: i as f64,
        });
    }

    // Lightning-fast operations on massive datasets
    println!("Processed {} points", points.len());

    Ok(())
}
```

## 🏗️ Architecture

**MemVec** is built on three key abstractions:

- **`MemVec<T, M>`** - The main vector interface, generic over memory backend `M`
- **`Memory` trait** - Pluggable storage backends (files, anonymous mapping, etc.)
- **Backend implementations** - `VecFile` for persistence, `MmapAnon` for performance

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   Your Code     │◄──►│    MemVec<T>     │◄──►│ Memory Backend  │
│                 │    │                  │    │                 │
│ vec.push(item)  │    │ • Vec-like API   │    │ • VecFile       │
│ vec[index]      │    │ • Type safety    │    │ • MmapAnon      │
│ vec.iter()      │    │ • Zero-copy      │    │ • Custom...     │
└─────────────────┘    └──────────────────┘    └─────────────────┘
```

## 🎛️ Advanced Usage

### Custom memory mapping options

```rust
use memvec::{MemVec, MmapAnon};
use memmap2::MmapOptions;

// Configure memory mapping for maximum performance
let mut options = MmapOptions::new();
options.len(1_000_000);

#[cfg(target_os = "linux")]
{
    options.populate(); // Pre-fault pages to avoid page faults
}

let mmap = MmapAnon::with_options(options)?;
let mut vec = unsafe { MemVec::<YourStruct, _>::try_from_memory(mmap)? };
```

### Working with existing files

```rust
use memvec::{MemVec, VecFile};

// Open or create a file-backed vector
let file = VecFile::open_or_create("data.bin", |vec| {
    // Initialize empty file with header
    Ok(())
})?;

let mut vec = unsafe { MemVec::<MyData, _>::try_from_memory(file)? };

if vec.is_empty() {
    println!("New file, initializing...");
    // Add initial data
} else {
    println!("Found {} existing records", vec.len());
    // Process existing data
}
```

## 🛠️ Installation

Add to your `Cargo.toml` by running:

```shell
cargo add memvec
```

## 📜 License

Licensed under the [BSD-2-Clause](LICENSE) license.
