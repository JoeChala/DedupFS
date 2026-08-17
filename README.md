# DedupFS

DedupFS is a local content-aware storage engine written in Rust.

The project explores how a storage system can reduce redundant storage by
splitting files into chunks, hashing their contents, identifying duplicate
chunks, and storing identical content only once.

## Project Status

DedupFS is currently under active development.

The current version contains the initial Rust project foundation and CI
pipeline. Storage, chunking, hashing, metadata, deduplication, and
concurrency will be implemented incrementally.

## Goals

The long-term goal is to build a production-quality local storage engine
with:

- file and directory ingestion
- streaming file processing
- content-based chunking
- content hashing
- content-addressable storage
- persistent metadata
- reference tracking
- garbage collection
- snapshots
- concurrent processing
- storage statistics
- benchmarking
- automated testing
- CI/CD
- a clean library API

## Development

Requirements:

- Rust stable toolchain
- Cargo
- Git

Build the project:

```bash
cargo build