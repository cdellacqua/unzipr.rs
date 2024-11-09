pub trait ResultExt<E> {
	fn if_err<F, O: FnOnce(E) -> F>(self, op: O);
}

impl<T, E> ResultExt<E> for Result<T, E> {
	/// Inspect the Error variant of a Result, consuming it.
	///
	/// This method can be used as an alternative to calling `inspect_err(F).ok();`.
	fn if_err<F, O: FnOnce(E) -> F>(self, op: O) {
		if let Err(err) = self {
			op(err);
		}
	}
}
