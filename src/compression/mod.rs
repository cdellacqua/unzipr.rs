use std::{
	fs::{self, File},
	io::{Read, Write},
	path::{Path, PathBuf},
	process::exit,
	str::FromStr,
};

use indicatif::ProgressBar;
use sha2::{Digest, Sha256};
use tracing::{error, warn};
use zip::ZipArchive;

use crate::rust_ext::ResultExt;

#[derive(thiserror::Error, Debug, Clone)]
pub enum ExtractionError {
	#[error("unable to open archive file")]
	UnableToOpenFile,
	#[error("incompatible archive")]
	Incompatible,
	#[error("partial extraction")]
	Partial,
	#[error("write failed, this usually means that the drive is full")]
	WriteFailed,
}

pub struct ExtractOpts<'a> {
	pub verify_checksum: bool,
	pub zip_root: &'a Path,
	pub outdir: &'a Path,
	pub zip_path: &'a Path,
	pub block_size: usize,
	pub extraction_pb: Option<&'a ProgressBar>,
	pub unwrap: bool,
	pub overwrite: bool,
}

pub fn extract(
	ExtractOpts {
		verify_checksum,
		zip_root,
		outdir,
		zip_path,
		block_size,
		extraction_pb,
		unwrap,
		overwrite,
	}: ExtractOpts,
) -> Result<(), ExtractionError> {
	let mut buf = vec![0u8; block_size];

	let file = File::open(zip_path).map_err(|err| {
		error!(?err, "unable to read file {zip_path:?}");
		ExtractionError::UnableToOpenFile
	})?;

	let mut zip = ZipArchive::new(file).map_err(|err| {
		error!(?err, "unable to open zip archive {zip_path:?}");
		ExtractionError::Incompatible
	})?;

	let zip_path_relative_to_initial =
		PathBuf::from_iter(zip_path.components().skip(zip_root.components().count()));

	for i in 0..zip.len() {
		let Ok(mut file) = zip.by_index(i).inspect_err(|err| {
			error!(
				?err,
				"unable to read file with index {i} in zip archive {zip_path:?}",
			);
		}) else {
			continue;
		};

		let Some(enclosed_name) = file.enclosed_name() else {
			let mangled_name = file.mangled_name();
			error!("unable to read a proper enclosed path for file {mangled_name:?}, archive {zip_path:?}");
			continue;
		};
		let output_path = PathBuf::from_iter([
			outdir,
			zip_path_relative_to_initial
				.parent()
				.expect("a valid zip path should always have a parent dir"),
			&*PathBuf::from_str(if unwrap {
				""
			} else {
				zip_path.file_stem().unwrap().to_str().unwrap()
			})
			.unwrap(),
			&enclosed_name,
		]);

		if let Some(extraction_pb) = extraction_pb {
			extraction_pb.reset();
		}

		let file_name = output_path
			.file_name()
			.expect("output path to contain a file name")
			.to_os_string()
			.into_string()
			.expect("file name to contain a valid utf8 string");

		if let Some(extraction_pb) = extraction_pb {
			extraction_pb.set_message(format!("extracting {file_name}..."));
		}

		if file.is_dir() {
			fs::create_dir_all(&output_path).if_err(|err| {
				error!(
					?err,
					"unable to create empty dir {output_path:?}, archive {zip_path:?}",
				);
			});
		} else {
			if output_path.exists() && !overwrite {
				warn!("file {output_path:?}, archive {zip_path:?} already exists, skipping...");
				continue;
			}

			if let Some(p) = output_path.parent() {
				if !p.exists() {
					fs::create_dir_all(p).if_err(|_| {
						error!("unable to create dir {p:?}, archive {zip_path:?}");
					});
				}
			}

			let Ok(mut output_file) = File::create(&output_path).inspect_err(|err| {
				error!(
					?err,
					"unable to create file {output_path:?}, archive {zip_path:?}",
				);
			}) else {
				continue;
			};

			let mut sha_zipped = if verify_checksum {
				Some(Sha256::new())
			} else {
				None
			};

			if let Some(extraction_pb) = extraction_pb {
				extraction_pb.set_length(file.size());
			}

			loop {
				let Ok(n) = file.read(&mut buf).inspect_err(|err| {
					error!(
						?err,
						"unable to read zipped file {enclosed_name:?}, archive {zip_path:?}",
					);
				}) else {
					break;
				};
				if n == 0 {
					break;
				}

				if let Some(extraction_pb) = extraction_pb {
					extraction_pb.inc(n as u64);
				}

				if let Some(ref mut sha) = sha_zipped {
					sha.update(&buf[..n]);
				}
				output_file.write(&buf[..n]).map_err(|err| {
					error!(
						?err,
						"unable to extract zipped file {enclosed_name:?}, archive {zip_path:?}",
					);
					ExtractionError::WriteFailed
				})?;
			}
			output_file.flush().if_err(|err| {
				error!(
					?err,
					"unable to extract zipped file {enclosed_name:?}, archive {zip_path:?}",
				);
				exit(1);
			});
			drop(output_file);
			let input_hash = sha_zipped.map(Sha256::finalize);

			if let Some(input_hash) = input_hash {
				if let Some(extraction_pb) = extraction_pb {
					extraction_pb.reset();
					extraction_pb.set_message(format!("verifying {file_name}..."));
				}
				let Ok(mut output_file) = File::open(&output_path).inspect_err(|err| {
					error!(
						?err,
						"unable to open output file {output_path:?}, archive {zip_path:?}",
					);
				}) else {
					continue;
				};
				let mut sha_unzipped = Sha256::new();
				loop {
					let Ok(n) = output_file.read(&mut buf).inspect_err(|err| {
						error!(
							?err,
							"unable to read unzipped file {output_path:?}, archive {zip_path:?}",
						);
					}) else {
						break;
					};
					if n == 0 {
						break;
					}
					if let Some(extraction_pb) = extraction_pb {
						extraction_pb.inc(n as u64);
					}
					sha_unzipped.update(&buf[..n]);
				}
				let output_hash = sha_unzipped.finalize();
				if output_hash[..] != input_hash[..] {
					error!(input_hash = hex::encode(&input_hash[..]), output_hash = hex::encode(&output_hash[..]), "unzipped file {output_path:?} doesn't have the same hash as the input one, archive {zip_path:?}");
				}
			}
		}
	}
	Ok(())
}
