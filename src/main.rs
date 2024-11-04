use std::{
	ffi::OsStr,
	fs::{self, File, ReadDir},
	io::{self, BufReader},
	path::{Path, PathBuf}, thread::spawn,
};

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use itertools::Itertools;
use rayon::{ThreadPool, ThreadPoolBuilder};
use zip::ZipArchive;

fn process_dir<Queue: Fn(PathBuf)>(
	dir: ReadDir,
	exploration_pb: &ProgressBar,
	_extraction_pb: &ProgressBar,
	queue_extraction: &Queue,
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
				_extraction_pb,
				queue_extraction,
			);
		} else if entry.path().is_file() && entry.path().extension() == Some(OsStr::new("zip")) {
			queue_extraction(entry.path());
		}
	}
}

fn extract(zip_path: PathBuf) {
	let file = File::open(&zip_path).expect("open file");
	let reader = BufReader::new(file);
	let mut zip = ZipArchive::new(reader).expect("open zip file");

	for i in 0..zip.len() {
		let mut file = zip.by_index(i).expect("zipped file");
		let outpath = match file.enclosed_name() {
			Some(enclosed_path) => PathBuf::from_iter([zip_path.parent().unwrap(), &enclosed_path]),
			None => continue,
		};

		let _file_name = outpath
			.file_name()
			.unwrap()
			.to_os_string()
			.into_string()
			.unwrap();
		// extraction_pb.set_message(file_name);

		if file.is_dir() {
			fs::create_dir_all(&outpath).expect("create empty folder");
		} else {
			if let Some(p) = outpath.parent() {
				if !p.exists() {
					fs::create_dir_all(p).expect("create folder for file");
				}
			}
			// extraction_pb.inc_length(file.size());
			let mut outfile = File::create(&outpath).expect("create file");
			io::copy(&mut file, &mut outfile).expect("copy file content");
			// extraction_pb.inc(file.size());
		}
	}
}

fn main() {
	let multi_pb = MultiProgress::new();
	let exploration_pb = multi_pb.add(ProgressBar::new(0));
	exploration_pb.set_style(
		ProgressStyle::with_template(
			"{prefix:.bold.dim} {bar:40.cyan/blue} {human_pos}/{human_len} {spinner} {wide_msg}",
		)
		.unwrap(),
	);
	let extraction_pb = multi_pb.add(ProgressBar::new(0));
	extraction_pb.set_style(
		ProgressStyle::with_template("{prefix:.bold.dim} {bar:40.cyan/blue} {bytes}/{total_bytes} {bytes_per_sec} {spinner} {wide_msg}")
			.unwrap(),
	);

	let args = std::env::args().collect_vec();
	let dir = Path::new(&args[1]);
	let dir = fs::read_dir(dir).expect("read folder");

	// let cores = num_cpus::get();
	let cores = 1;

	let thread_pool = ThreadPoolBuilder::new().num_threads(cores).build().expect("thread pool");

	process_dir(dir, &exploration_pb, &extraction_pb, &|path| {
		thread_pool.install(|| extract(path));
	});

	drop(thread_pool);

	multi_pb.clear().unwrap();
}
