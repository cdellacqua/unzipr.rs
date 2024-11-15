use byte_unit::Byte;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use itertools::Itertools;
use lending_thread_pool::ThreadPool;
use std::{
	ffi::OsStr,
	fs::{self, read_to_string, ReadDir},
	mem,
	path::{Path, PathBuf},
	process::exit,
	time::Duration,
};
use tracing::{debug, error, warn, Level};
use unzipr::{
	compression::{extract, ExtractOpts, ExtractionError},
	indicatif_ext::IndicatifWriter,
	rust_ext::ResultExt,
};

fn process_dir<Queue: FnMut(PathBuf)>(
	dir: ReadDir,
	exploration_pb: &ProgressBar,
	queue_extraction: &mut Queue,
) {
	let entries = dir.into_iter().filter_map(Result::ok).collect_vec();
	exploration_pb.set_length(exploration_pb.length().unwrap_or(0) + entries.len() as u64);
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
			process_dir(nested, exploration_pb, queue_extraction);
		} else if entry_path.is_file() && entry_path.extension() == Some(OsStr::new("zip")) {
			exploration_pb.set_message(format!("extracting {entry_path:?}..."));
			queue_extraction(entry_path);
			exploration_pb.set_message("");
		}
	}
}

use clap::{ArgAction, Parser};

#[derive(Parser, Clone, Debug)]
#[command(version = env!("CARGO_PKG_VERSION"), about = "unzipr - recursively extract every zip file found in a directory and its subdirectories", long_about = None)]
#[command(next_line_help = true)]
#[allow(clippy::struct_excessive_bools)]
struct CliArgs {
	path: PathBuf,
	#[arg(short = 'q', long, help = "hide progress bars")]
	quiet: bool,
	#[arg(
		short = 't',
		long,
		help = "choose the number of threads to spawn to unzip files. By default, one thread per core is spawned to maximize parallelism"
	)]
	threads: Option<usize>,
	#[arg(
		short = 's',
		long,
		help = "skip checksum verification. The feature is enabled by default and it's meant to verify that extracted files match the originals from each archive using SHA256 as a hashing function"
	)]
	skip_checksum: bool,
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
	#[arg(short = 'v', long, action = clap::ArgAction::Count, help = "select the log level by passing this flag multiple times. The min log level is 0 (no flag), max is 4 (-vvvv)")]
	verbose: u8,
	#[arg(
		short = 'b',
		long = "block",
		help = "block size for the copy operation. The unit can be omitted (implies bytes) or any of the usual MB/MiB/KB/KiB",
		default_value = "8KiB"
	)]
	block_size: Byte,
	#[arg(
		short = 'p',
		long = "password",
		help = "each password that should be tried when dealing with encrypted archive",
		action = ArgAction::Append
	)]
	passwords: Vec<String>,
	#[arg(
		short = 'P',
		long = "password-file",
		help = "the path to a file that contains each password that should be tried when dealing with encrypted archive",
		action = ArgAction::Append
	)]
	passwords_path: Option<PathBuf>,
}

#[allow(clippy::too_many_lines)]
fn main() {
	let mut opts = CliArgs::parse();

	let multi_pb = MultiProgress::new();
	let exploration_pb = multi_pb.add(ProgressBar::no_length()).with_style(
		ProgressStyle::with_template(
			"[{bar:40.green/yellow}] {human_pos}/{human_len} {spinner} {wide_msg}",
		)
		.expect("a valid progress template template")
		.progress_chars("=>-"),
	);
	exploration_pb.enable_steady_tick(Duration::from_millis(100));

	if opts.quiet {
		exploration_pb.set_draw_target(ProgressDrawTarget::hidden());
	}

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

	let mut all_passwords = mem::take(&mut opts.passwords);
	if let Some(ref passwords_path) = opts.passwords_path {
		match read_to_string(passwords_path) {
			Ok(str) => {
				let lines = str.lines().map(ToOwned::to_owned);
				all_passwords.extend(lines);
			}
			Err(err) => {
				error!(
					?err,
					"unable to read passwords from file {passwords_path:?}"
				);
				exit(2);
			}
		}
	}

	let cores = opts.threads.unwrap_or_else(num_cpus::get);

	let dir = Path::new(&opts.path);
	let Ok(dir) = fs::read_dir(dir).inspect_err(|err| {
		error!(?err, "unable to scan dir {:?}", dir);
	}) else {
		return;
	};

	let Ok(block_size) = usize::try_from(opts.block_size).inspect_err(|err| {
		error!(
			?err,
			"block size doesn't fit usize, which is {} bits on this machine",
			usize::BITS
		);
	}) else {
		return;
	};

	debug!("extracting zip files found in {dir:?}...");

	let workers_progress_bars = (0..cores).map(|i| {
		let pb = multi_pb.add(ProgressBar::no_length()
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
			if opts.quiet {
				pb.set_draw_target(ProgressDrawTarget::hidden());
			}
			pb.set_message("idle");
			pb
	}).collect_vec();

	let mut pool = ThreadPool::new(
		workers_progress_bars
			.iter()
			.map(|pb| (all_passwords.clone(), opts.clone(), pb.clone()))
			.collect_vec(),
	);

	process_dir(dir, &exploration_pb, &mut |path| {
		pool.enqueue(move |(all_passwords, opts, extraction_pb)| {
			let result = extract(&ExtractOpts {
				verify_checksum: !opts.skip_checksum,
				zip_root: &opts.path,
				outdir: opts.outdir.as_ref().unwrap_or(&opts.path),
				zip_path: &path,
				block_size,
				extraction_pb: Some(extraction_pb),
				unwrap: opts.unwrap,
				overwrite: opts.overwrite,
				passwords: all_passwords,
			});
			if let Err(ExtractionError::WriteFailed) = result {
				error!("unrecoverable error");
				exit(1);
			}
			extraction_pb.reset();
			extraction_pb.unset_length();
			extraction_pb.set_message("idle");
		});
	});

	exploration_pb.set_message("waiting for workers to finish...");
	pool.join();
	multi_pb
		.clear()
		.if_err(|err| warn!(?err, "unable to clear multi progress bar"));

	debug!("extraction completed");
}
