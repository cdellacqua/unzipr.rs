pub trait ResultExt<E> {
	fn if_err<F, O: FnOnce(E) -> F>(self, op: O);
}

impl<T, E> ResultExt<E> for Result<T, E> {
	fn if_err<F, O: FnOnce(E) -> F>(self, op: O) {
		if let Err(err) = self {
			op(err);
		}
	}
}
