use std::io;

use itertools::Either;
use tracing_subscriber::fmt::MakeWriter;

#[derive(Debug, Clone)]
pub struct IndicatifWriter(Either<indicatif::ProgressBar, indicatif::MultiProgress>);

impl<'a> From<&'a indicatif::ProgressBar> for IndicatifWriter {
	fn from(value: &'a indicatif::ProgressBar) -> Self {
		Self(Either::Left(value.clone()))
	}
}

impl<'a> From<&'a indicatif::MultiProgress> for IndicatifWriter {
	fn from(value: &'a indicatif::MultiProgress) -> Self {
		Self(Either::Right(value.clone()))
	}
}

impl io::Write for IndicatifWriter {
	fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
		match self.0 {
			Either::Left(ref pb) => pb.suspend(|| io::stdout().write(buf)),
			Either::Right(ref mpb) => mpb.suspend(|| io::stdout().write(buf)),
		}
	}

	fn flush(&mut self) -> std::io::Result<()> {
		match self.0 {
			Either::Left(ref pb) => pb.suspend(|| io::stdout().flush()),
			Either::Right(ref mpb) => mpb.suspend(|| io::stdout().flush()),
		}
	}
}

impl<'a> MakeWriter<'a> for IndicatifWriter {
	type Writer = IndicatifWriter;

	fn make_writer(&'a self) -> Self::Writer {
		Self(self.0.clone())
	}
}
