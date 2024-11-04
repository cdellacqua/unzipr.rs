use std::{
	collections::VecDeque,
	ffi::OsStr,
	fs::{self, File, ReadDir},
	io::{self, BufReader},
	path::{Path, PathBuf},
	sync::{Arc, Condvar, Mutex},
	thread,
};

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use itertools::Itertools;
use zip::ZipArchive;

fn process_dir<Queue: FnMut(PathBuf)>(
	dir: ReadDir,
	exploration_pb: &ProgressBar,
	_extraction_pb: &ProgressBar,
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
				_extraction_pb,
				queue_extraction,
			);
		} else if entry.path().is_file() && entry.path().extension() == Some(OsStr::new("zip")) {
			queue_extraction(entry.path());
		}
	}
}

fn extract(path: PathBuf) {
	let file = File::open(path).expect("open file");
	let reader = BufReader::new(file);
	let mut zip = ZipArchive::new(reader).expect("open zip file");
	for i in 0..zip.len() {
		let mut file = zip.by_index(i).expect("zipped file");
		let outpath = match file.enclosed_name() {
			Some(path) => PathBuf::from_iter([path.parent().unwrap(), &path]),
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

	// let cores = num_cpus::get();
	let cores = 1;

	let queue = Some(VecDeque::<PathBuf>::with_capacity(cores));
	let condvar_pair = Arc::new((Mutex::new(queue), Condvar::new()));

	let mut workers = Vec::with_capacity(cores);
	for _ in 0..cores {
		let condvar_pair = condvar_pair.clone();
		workers.push(thread::spawn(move || loop {
			let (queue_lock, condvar) = &*condvar_pair;
			let mut guard = queue_lock.lock().unwrap();
			while guard.as_ref().is_some_and(|x| x.len() == 0) {
				guard = condvar.wait(guard).unwrap();
			}
			if let Some(queue) = guard.as_mut() {
				let path = queue.pop_front().unwrap();
				extract(path);
				condvar.notify_all();
			} else {
				break;
			}
			drop(guard);
		}));
	}

	let args = std::env::args().collect_vec();
	let dir = Path::new(&args[1]);
	let dir = fs::read_dir(dir).expect("read folder");

	{
		let condvar_pair = condvar_pair.clone();
		let (queue_lock, condvar) = &*condvar_pair;
		process_dir(dir, &exploration_pb, &extraction_pb, &mut move |path| {
			let mut guard = queue_lock.lock().unwrap();
			while guard.as_ref().is_some_and(|x| x.len() >= cores) {
				guard = condvar.wait(guard).unwrap();
			}
			if let Some(queue) = guard.as_mut() {
				queue.push_back(path);
				condvar.notify_one();
			}
		});
	}
	let (queue_lock, condvar) = &*condvar_pair;
	let mut guard = queue_lock.lock().unwrap();
	while guard.as_ref().is_some_and(|x| x.len() > 0) {
		guard = condvar.wait(guard).unwrap();
	}
	guard.as_mut().take();
	condvar.notify_all();
	drop(guard);
	for w in workers {
		w.join().unwrap();
	}

	multi_pb.clear().unwrap();
}
