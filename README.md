# unzipr

<!-- [![Documentation](https://docs.rs/unzipr/badge.svg)](https://docs.rs/unzipr/) -->
<!-- [![Crates.io](https://img.shields.io/crates/v/unzipr.svg)](https://crates.io/crates/unzipr) -->
[![Build status](https://github.com/cdellacqua/unzipr.rs/workflows/CI/badge.svg)](https://github.com/cdellacqua/unzipr.rs/actions/workflows/ci.yml)

A command line utility that recursively unzip files in a directory.

Features:

- [x] supports zip files (thanks to [crates/zip](https://crates.io/crates/zip))
- [x] multithreading
- [x] same name protection (by creating a directory for each extracted archive)
- [x] overwrite protection
- [x] cool progress visualization (thanks to [crates/indicatif](https://crates.io/crates/indicatif))
- [x] checksum validation of the extracted files
- [ ] encrypted archives

## Usage

Basic:

```sh
unzipr path/to/directory/containing/zips
```

Redirect output:

```sh
unzipr path/to/directory/containing/zips --outdir path/to/output
```

Do not put the extracted content in a dedicated folder:

```sh
unzipr path/to/directory/containing/zips --unwrap
```

...and many more! Check out the help command for more options.

```sh
unzipr --help
```
