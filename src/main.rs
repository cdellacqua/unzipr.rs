use byte_unit::Byte;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use itertools::Itertools;
use sha2::{Digest, Sha256};
use std::{
	ffi::OsStr,
	fs::{self, File, ReadDir},
	io::{BufReader, Read, Write},
	path::{Path, PathBuf},
	process::exit,
	str::FromStr,
	time::Duration,
};
use tracing::{debug, error, warn, Level};
use unzipr::{indicatif_ext::IndicatifWriter, rust_ext::ResultExt, thread_pool::ThreadPool};
use zip::ZipArchive;

fn process_dir<Queue: FnMut(PathBuf)>(
	dir: ReadDir,
	exploration_pb: &ProgressBar,
	queue_extraction: &mut Queue,
) {
	let entries = dir.into_iter().filter_map(|entry| entry.ok()).collect_vec();
	exploration_pb.inc_length(entries.len() as u64);
	for entry in entries {
		let entry_path = entry.path();
		exploration_pb.inc(1);
		if entry_path.is_dir() {
			let Ok(nested) = fs::read_dir(&entry_path).inspect_err(|err| {
				error!(?err, "unable to read directory {entry_path:?}");
			}) else {
				return;
			};
			exploration_pb.set_message(format!("exploring nested dir {entry_path:?}..."));
			process_dir(nested, exploration_pb, queue_extraction)
		} else if entry_path.is_file() && entry_path.extension() == Some(OsStr::new("zip")) {
			exploration_pb.set_message(format!("extracting {entry_path:?}..."));
			queue_extraction(entry_path);
			exploration_pb.set_message("");
		}
	}
}

fn extract(opts: &CliArgs, zip_path: PathBuf, extraction_pb: &ProgressBar) {
	let mut buf = vec![0u8; opts.block_size.as_u64() as usize];

	let Ok(file) = File::open(&zip_path).inspect_err(|err| {
		error!(?err, "unable to read file {zip_path:?}");
	}) else {
		return;
	};

	let reader = BufReader::new(file);
	let Ok(mut zip) = ZipArchive::new(reader).inspect_err(|err| {
		error!(?err, "unable to open zip archive {zip_path:?}");
	}) else {
		return;
	};

	let zip_path_relative_to_initial =
		PathBuf::from_iter(zip_path.components().skip(opts.path.components().count()));

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
			opts.outdir.as_ref().unwrap_or(&opts.path),
			zip_path_relative_to_initial
				.parent()
				.expect("a valid zip path should always have a parent dir"),
			&*PathBuf::from_str(if opts.unwrap {
				""
			} else {
				zip_path.file_stem().unwrap().to_str().unwrap()
			})
			.unwrap(),
			&enclosed_name,
		]);

		extraction_pb.reset();

		let file_name = output_path
			.file_name()
			.expect("output path to contain a file name")
			.to_os_string()
			.into_string()
			.expect("file name to contain a valid utf8 string");

		extraction_pb.set_message(format!("extracting {file_name}..."));

		if file.is_dir() {
			fs::create_dir_all(&output_path).if_err(|err| {
				error!(
					?err,
					"unable to create empty dir {output_path:?}, archive {zip_path:?}",
				);
			});
		} else {
			if output_path.exists() && !opts.overwrite {
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

			let mut sha_zipped = if opts.checksum {
				Some(Sha256::new())
			} else {
				None
			};

			extraction_pb.set_length(if opts.checksum {
				file.size() * 2
			} else {
				file.size()
			});
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
				extraction_pb.inc(n as u64);
				if let Some(ref mut sha) = sha_zipped {
					sha.update(&buf);
				}
				output_file.write(&buf).if_err(|err| {
					error!(
						?err,
						"unable to extract zipped file {enclosed_name:?}, archive {zip_path:?}",
					);
					exit(1);
				});
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
				extraction_pb.set_message(format!("verifying {file_name}..."));
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
					extraction_pb.inc(n as u64);
					sha_unzipped.update(&buf);
				}
				let output_hash = sha_unzipped.finalize();
				if output_hash[..] != input_hash[..] {
					error!(input_hash = hex::encode(&input_hash[..]), output_hash = hex::encode(&output_hash[..]), "unzipped file {output_path:?} doesn't have the same hash as the input one, archive {zip_path:?}");
				}
			}
		}
	}
}

use clap::{ArgAction, Parser};

#[derive(Parser, Clone, Debug)]
#[command(version = "1.0.0", about = "unzipr - recursively extract every zip file found in a directory and its subdirectories", long_about = None)]
#[command(next_line_help = true)]
struct CliArgs {
	path: PathBuf,
	#[arg(
		short = 't',
		long,
		help = "choose the number of threads to spawn to unzip files. By default, one thread per core is spawned to maximize parallelism"
	)]
	threads: Option<usize>,
	#[arg(
		short = 'c',
		long,
		help = "use the SHA256 hashing function as a checksum to verify that extracted files match the original from the archives",
		default_value_t = true,
		action = ArgAction::Set
	)]
	checksum: bool,
	#[arg(
		short = 'f',
		long,
		help = "overwrite existing files with the same name as the ones in the archive while extracting"
	)]
	overwrite: bool,
	#[arg(
		short = 'o',
		long,
		help = "change the output directory. By default, the input path is used"
	)]
	outdir: Option<PathBuf>,
	#[arg(
		short = 'u',
		long,
		help = "put the content of the archive in the same directory as the zip archive it came from, rather than in a directory with the name of the source archive"
	)]
	unwrap: bool,
	#[arg(short = 'v', long, action = clap::ArgAction::Count, help = "select the log level by passing this flag multiple times. The min log level is 0 (no flag), max is 3 (-vvv)")]
	verbose: u8,
	#[arg(
		short = 'b',
		long = "block",
		help = "block size for the copy operation. The unit can be omitted (implies bytes) or any of the usual MB/MiB/KB/KiB",
		default_value = "1MiB"
	)]
	block_size: Byte,
}

fn main() {
	let opts = CliArgs::parse();

	let multi_pb = MultiProgress::new();
	let exploration_pb = multi_pb.add(ProgressBar::empty());
	exploration_pb.enable_steady_tick(Duration::from_millis(100));
	exploration_pb.set_style(
		ProgressStyle::with_template(
			"[{bar:40.green/yellow}] {human_pos}/{human_len} {spinner} {wide_msg}",
		)
		.expect("a valid progress template template")
		.progress_chars("=>-"),
	);

	tracing_subscriber::fmt()
		.without_time()
		.with_target(false)
		.with_thread_names(true)
		.with_writer(IndicatifWriter::from(&multi_pb))
		.with_max_level(match opts.verbose {
			0 => Level::ERROR,
			1 => Level::WARN,
			2 => Level::INFO,
			3 => Level::DEBUG,
			_ => Level::TRACE,
		})
		.init();

	let cores = opts.threads.unwrap_or_else(num_cpus::get);

	let dir = Path::new(&opts.path);
	let Ok(dir) = fs::read_dir(dir).inspect_err(|err| {
		error!(?err, "unable to scan dir {:?}", dir);
	}) else {
		return;
	};

	debug!("extracting zip files found in {dir:?}...");

	let workers_progress_bars = (0..cores).map(|i| {
		let pb = multi_pb.add(ProgressBar::empty()
			.with_style(
				ProgressStyle::with_template(&format!("{} [{{bar:40.green/yellow}}] {{bytes}}/{{total_bytes}} {{bytes_per_sec}} {{spinner}} {{wide_msg}}", match i {
					0 if cores == 1 => '─',
					0 => '┌',
					n if n < cores - 1 => '├',
					_ => '└',
				}))
					.expect("a valid progress bar template")
					.progress_chars("=>-")
			));
			pb.set_message("idle");
			pb
	}).collect_vec();

	let mut pool = ThreadPool::new(
		workers_progress_bars
			.iter()
			.map(|pb| (opts.clone(), pb.clone()))
			.collect_vec(),
	);

	process_dir(dir, &exploration_pb, &mut |path| {
		pool.send_blocking(|(opts, extraction_pb)| {
			extract(opts, path, extraction_pb);
			extraction_pb.reset();
			extraction_pb.unset_length();
			extraction_pb.set_message("idle");
		});
	});

	pool.join();
	multi_pb
		.clear()
		.if_err(|err| warn!(?err, "unable to clear multi progress bar"));

	debug!("extraction completed");
}
