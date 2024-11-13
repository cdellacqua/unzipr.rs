use std::{env, fs::File, io::Read};

use tempfile::tempdir;
use unzipr::compression::{extract, ExtractOpts};

#[test]
fn smoke_test() {
	let dir = tempdir().unwrap();

	extract(ExtractOpts {
		verify_checksum: false,
		zip_root: &env::current_dir().unwrap().join("tests"),
		outdir: dir.path(),
		zip_path: &env::current_dir()
			.unwrap()
			.join("tests")
			.join("demo-206B_20B.zip"),
		block_size: 206,
		extraction_pb: None,
		unwrap: true,
		overwrite: false,
		passwords: &vec![],
	})
	.unwrap();
}

#[test]
fn verify_extracted_size() {
	let dir = tempdir().unwrap();

	extract(ExtractOpts {
		verify_checksum: false,
		zip_root: &env::current_dir().unwrap().join("tests"),
		outdir: dir.path(),
		zip_path: &env::current_dir()
			.unwrap()
			.join("tests")
			.join("demo-206B_20B.zip"),
		block_size: 7,
		extraction_pb: None,
		unwrap: true,
		overwrite: false,
		passwords: &vec![],
	})
	.unwrap();

	assert_eq!(
		dir.path().join("demo.txt").metadata().unwrap().len(),
		20,
		"extracted file exists"
	);
}
#[test]
fn verify_extracted_content() {
	let dir = tempdir().unwrap();

	extract(ExtractOpts {
		verify_checksum: false,
		zip_root: &env::current_dir().unwrap().join("tests"),
		outdir: dir.path(),
		zip_path: &env::current_dir()
			.unwrap()
			.join("tests")
			.join("demo-206B_20B.zip"),
		block_size: 333,
		extraction_pb: None,
		unwrap: true,
		overwrite: false,
		passwords: &vec![],
	})
	.unwrap();

	let mut str = String::new();
	File::open(dir.path().join("demo.txt"))
		.unwrap()
		.read_to_string(&mut str)
		.unwrap();
	assert_eq!(
		str, "This is a test file\n",
		"Extracted content doesn't match"
	);
}
