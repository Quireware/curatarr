{ lib
, rustPlatform
, stdenv
,
}:
let
  manifest = lib.importTOML ../Cargo.toml;
in
rustPlatform.buildRustPackage {
  pname = "curatarr";
  version = manifest.workspace.package.version;

  # Only what the build needs: keeps the store path stable when docs or
  # editor files change.
  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../crates
      ../migrations
    ];
  };

  cargoLock.lockFile = ../Cargo.lock;

  # SQLite is bundled by libsqlite3-sys; TLS is rustls. No native libraries needed.
  cargoBuildFlags = [ "--package" "curatarr-bin" ];
  cargoTestFlags = [ "--workspace" ];

  # Tests write to $TMPDIR and use in-memory SQLite; all run offline.
  doCheck = true;

  meta = {
    description = "Ebook, comic and manga acquisition manager (the *arr Readarr never became)";
    homepage = "https://github.com/Quireware/curatarr";
    license = lib.licenses.mit;
    mainProgram = "curatarr";
    platforms = lib.platforms.unix;
    broken = stdenv.hostPlatform.isWindows;
  };
}
