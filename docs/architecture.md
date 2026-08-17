# DedupFS Architecture

## Current Architecture

DedupFS currently consists of a single Rust binary crate.

## Planned Architecture

```text
                    DedupFS CLI
                         |
                         v
                 Deduplication Engine
                         |
             +-----------+-----------+
             |           |           |
             v           v           v
          Chunker     Hasher      Metadata
             |           |           |
             +-----------+-----------+
                         |
                         v
                Content-Addressable
                     Storage
```
Additional components will eventually include:

- reference tracking
- garbage collection
- snapshots
- concurrent processing