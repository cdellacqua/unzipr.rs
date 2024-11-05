use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use itertools::Itertools;
use std::{
	ffi::OsStr,
	fs::{self, File, ReadDir},
	io::{BufReader, Read, Write},
	path::{Path, PathBuf},
	str::FromStr,
};
use tracing::{debug, Level};
use unzip_all::{byte_unit::KiBToBytes, thread_pool::ThreadPool};
use zip::ZipArchive;

fn process_dir<Queue: FnMut(PathBuf)>(
	dir: ReadDir,
	exploration_pb: &ProgressBar,
	queue_extraction: &mut Queue,
) {
	let entries = dir.into_iter().filter_map(|entry| entry.ok()).collect_vec();
	exploration_pb.inc_length(entries.len() as u64);
	for entry in entries {
		let file_name = entry.file_name().into_string().unwrap();
		exploration_pb.set_message(file_name);
		exploration_pb.inc(1);
		if entry.path().is_dir() {
			process_dir(
				fs::read_dir(entry.path()).expect("read folder"),
				exploration_pb,
				queue_extraction,
			);
		} else if entry.path().is_file() && entry.path().extension() == Some(OsStr::new("zip")) {
			queue_extraction(entry.path());
		}
	}
}

fn extract(zip_path: PathBuf, extraction_pb: &ProgressBar) {
	let file = File::open(&zip_path).expect("open file");
	let reader = BufReader::new(file);
	let mut zip = ZipArchive::new(reader).expect("open zip file");
	for i in 0..zip.len() {
		let mut file = zip.by_index(i).expect("zipped file");
		let output_path = match file.enclosed_name() {
			Some(path) => PathBuf::from_iter([
				&zip_path.parent().unwrap(),
				&*PathBuf::from_str(zip_path.file_stem().unwrap().to_str().unwrap()).unwrap(),
				&path,
			]),
			None => continue,
		};

		extraction_pb.reset();

		let file_name = output_path
			.file_name()
			.unwrap()
			.to_os_string()
			.into_string()
			.unwrap();

		extraction_pb.set_message(file_name);

		if file.is_dir() {
			fs::create_dir_all(&output_path).expect("create empty folder");
		} else {
			if let Some(p) = output_path.parent() {
				if !p.exists() {
					fs::create_dir_all(p).expect("create folder for file");
				}
			}

			extraction_pb.set_length(file.size());

			let mut output_file = File::create(&output_path).expect("create file");

			let mut buf = vec![0u8; 10i32.KiB()];
			while let Ok(n) = file.read(&mut *buf) {
				if n == 0 {
					break;
				}
				extraction_pb.inc(n as u64);
				output_file.write(&*buf).unwrap();
			}
		}
	}
}

fn main() {
	tracing_subscriber::fmt().with_max_level(Level::INFO).init();

	let multi_pb = MultiProgress::new();
	let exploration_pb = multi_pb.add(ProgressBar::new(0));
	exploration_pb.set_style(
		ProgressStyle::with_template(
			"{bar:40.green/yellow} {human_pos}/{human_len} {spinner} {wide_msg}",
		)
		.unwrap()
		.progress_chars("▇▆▅▄▃▂▁ "),
	);

	// let cores = num_cpus::get();
	let cores = 4;

	let args = std::env::args().collect_vec();
	let dir = Path::new(&args[1]);
	let dir = fs::read_dir(dir).expect("read folder");

	let mut pool = ThreadPool::new((0..cores).into_iter().map(|i| {
		let extraction_pb = multi_pb.add(ProgressBar::new(0));
		extraction_pb.set_style(
			ProgressStyle::with_template(&format!("{}─{{bar:40.green/yellow}} {{bytes}}/{{total_bytes}} {{bytes_per_sec}} {{spinner}} {{wide_msg}}", match i {
				0 => '┌',
				n if n < cores-1 => '├',
				_ => '└',
			}))
				.unwrap()
        .progress_chars("▇▆▅▄▃▂▁ ")
		);
		extraction_pb
	}).collect_vec());

	process_dir(dir, &exploration_pb, &mut |path| {
		debug!("[process_dir] enqueueing task...");
		pool.send_blocking(move |extraction_pb| extract(path, extraction_pb));
	});

	pool.join();
	multi_pb.clear().unwrap();
}
