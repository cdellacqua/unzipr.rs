use std::{env, fs::File, io::Read};

use tempfile::tempdir;
use unzipr::compression::{extract, ExtractOpts};

#[test]
fn verify_extracted_content() {
	tracing_subscriber::fmt::init();
	let dir = tempdir().unwrap();

	extract(ExtractOpts {
		verify_checksum: false,
		zip_root: &env::current_dir().unwrap().join("tests"),
		outdir: dir.path(),
		zip_path: &env::current_dir()
			.unwrap()
			.join("tests")
			.join("demo-206B_20B_encrypted.zip"),
		block_size: 333,
		extraction_pb: None,
		unwrap: true,
		overwrite: false,
		passwords: &vec!["Wr0ng".to_owned(), "P4ssw0rd".to_owned()],
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
