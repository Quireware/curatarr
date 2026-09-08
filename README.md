# curatarr

Ebook, comic and manga acquisition manager: the *arr application Readarr never became.
One Rust binary that scans and organises a library, extracts metadata and covers from
EPUB / CBZ / PDF files, detects duplicates, and exposes a REST API. Metadata providers,
indexers, download clients and a web UI follow in later phases.

## Status

Phase 1 (foundation, scan and import) is complete:

- Domain model for works, editions, authors, series, publishers, collections, tags and files
- SQLite repository (PostgreSQL migrations exist, repository not yet implemented)
- Format detection by magic bytes and extension for EPUB, MOBI, AZW3, CBZ, CBR, CB7, CBT, PDF, DjVu, FB2
- Metadata extraction from EPUB (Dublin Core, calibre series tags), CBZ (ComicInfo.xml) and PDF
- Cover extraction with 64/256/1024px thumbnails in content-addressed storage
- Directory scanner with glob exclusions, SHA-256 hashing, naming templates
- Import pipeline: copy / move / hardlink, entity matching, rollback on failure
- Exact (hash) and near (title similarity) duplicate detection, recycle bin with retention
- REST API under `/api/v1` and a CLI

CBR extraction is not implemented (needs an unrar library); CBR files import with
filename-derived metadata.

## Running

```sh
cp curatarr.example.toml curatarr.toml   # edit root_folders, data_dir, port
cargo run -p curatarr-bin -- --config curatarr.toml serve
curl localhost:8787/health
```

CLI:

```sh
curatarr scan /path/to/library           # catalogue files in place, register as root folder
curatarr import /path/to/inbox --into /path/to/library   # organise files with the naming template
curatarr migrate
```

Environment variables override the file: `CURATARR__SERVER__PORT=9090`.

## API

| Method | Path | Notes |
|---|---|---|
| GET/POST | `/api/v1/works` | `?page=&per_page=&content_type=&read_status=&title=` |
| GET/PUT/DELETE | `/api/v1/works/{id}` | PUT takes a partial body; JSON `null` clears a field |
| GET/POST | `/api/v1/works/{id}/authors` | link body: `{"author_id": ..., "role": "author"}` |
| GET | `/api/v1/works/{id}/editions`, `/series` | |
| CRUD | `/api/v1/editions`, `/authors`, `/series`, `/publishers` | same pattern |
| GET/POST | `/api/v1/series/{id}/entries` | |
| GET | `/api/v1/files` | `?include_deleted=true` shows recycled files |
| POST | `/api/v1/files/{id}/recycle`, `/restore` | reversible delete |
| DELETE | `/api/v1/files/{id}?confirm=true` | permanent |
| GET | `/api/v1/recycle-bin`, POST `/recycle-bin/cleanup` | |
| GET/POST | `/api/v1/root-folders` | includes free space |
| POST | `/api/v1/root-folders/{id}/scan` | 202; poll `/scan/status` |
| POST | `/api/v1/root-folders/{id}/import` | body `{"source": "/abs/dir"}` |
| GET | `/api/v1/duplicates`, `/duplicates/near?threshold=0.85` | |

Errors are `{"error": {"code": "NOT_FOUND", "message": "..."}}`.

## Naming templates

Tokens: `{Title} {Author} {AuthorSort} {Series} {SeriesPosition} {SeriesPositionPadded}
{Year} {Publisher} {Language} {Isbn} {Extension}`. Empty tokens collapse cleanly, so the
default `{Author}/{Series}/{SeriesPositionPadded} - {Title}.{Extension}` yields
`Frank Herbert/Dune.epub` for a book without a series.

## Nix

```sh
nix build .#curatarr          # package
nix run . -- --help
nix flake check               # build, clippy, rustfmt, NixOS VM test (Linux)
nix develop                   # toolchain shell
```

NixOS module (`nixosModules.default`): see `nix/examples/nixos-host.nix`. It creates a
`curatarr` system user, renders `/etc/curatarr/curatarr.toml`, registers `rootFolders`
at startup and runs the service under a strict systemd sandbox with write access only to
`dataDir` and the root folders.

## Development

```sh
cargo nextest run --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

Conventions: no `#[allow(...)]`, every `.clone()` in non-test code carries a
`// clone: <reason>` comment, proptest for parsers and boundaries, rstest for cases.

## License

MIT
