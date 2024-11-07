use std::{
	fmt::Debug,
	mem::{self, replace},
	sync::{Arc, Condvar, Mutex},
	thread::{self, JoinHandle},
};

use itertools::Itertools;
use tracing::debug;

pub struct ThreadPool<WorkerCtx: Send + 'static = ()> {
	inner: Arc<ThreadPoolShared<WorkerCtx>>,
	workers: Vec<JoinHandle<()>>,
}

enum PoolQueueSlot<WorkerCtx: Send + 'static> {
	Done,
	Empty,
	Todo(Box<dyn FnOnce(&mut WorkerCtx) + Send>),
}
impl<WorkerCtx: Send + 'static> Debug for PoolQueueSlot<WorkerCtx> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Done => write!(f, "Done"),
			Self::Empty => write!(f, "Empty"),
			Self::Todo(_) => write!(f, "Todo"),
		}
	}
}

impl<WorkerCtx: Send + 'static> PoolQueueSlot<WorkerCtx> {
	fn take(&mut self) -> PoolQueueSlot<WorkerCtx> {
		match self {
			PoolQueueSlot::Todo(_) => replace(self, PoolQueueSlot::Empty),
			PoolQueueSlot::Done => PoolQueueSlot::Done,
			PoolQueueSlot::Empty => PoolQueueSlot::Empty,
		}
	}
}

pub struct ThreadPoolShared<WorkerCtx: Send + 'static> {
	workers_condvar: Condvar,
	pool_condvar: Condvar,
	pending_task: Mutex<PoolQueueSlot<WorkerCtx>>,
}

impl<WorkerCtx: Send + 'static> ThreadPool<WorkerCtx> {
	pub fn new(worker_ctxs: Vec<WorkerCtx>) -> Self {
		let inner = Arc::new(ThreadPoolShared {
			workers_condvar: Default::default(),
			pool_condvar: Default::default(),
			pending_task: Mutex::new(PoolQueueSlot::Empty),
		});
		let workers = worker_ctxs
			.into_iter()
			.enumerate()
			.map(|(i, mut ctx)| {
				let inner_clone = inner.clone();
				thread::Builder::new()
					.name(format!("w({i})"))
					.spawn(move || loop {
						let ThreadPoolShared {
							pending_task,
							workers_condvar,
							pool_condvar,
							..
						} = &*inner_clone;
						let mut guard = pending_task.lock().unwrap();
						while matches!(*guard, PoolQueueSlot::Empty) {
							debug!("waiting for tasks...");
							guard = workers_condvar.wait(guard).unwrap();
						}
						if let PoolQueueSlot::Todo(task_fn) = guard.take() {
							pool_condvar.notify_all();
							drop(guard);
							debug!("running task...");
							(task_fn)(&mut ctx);
						} else {
							debug!("quitting...");
							break;
						}
					})
					.expect("thread to be spawned")
			})
			.collect_vec();

		Self { inner, workers }
	}

	pub fn send_blocking<Task: FnOnce(&mut WorkerCtx) + Send + 'static>(&mut self, task: Task) {
		let mut guard = self.inner.pending_task.lock().unwrap();
		while matches!(*guard, PoolQueueSlot::Todo(_)) {
			debug!("waiting for available workers...");
			guard = self.inner.pool_condvar.wait(guard).unwrap();
		}
		if matches!(*guard, PoolQueueSlot::Empty) {
			*guard = PoolQueueSlot::Todo(Box::new(task));
			self.inner.workers_condvar.notify_one();
			debug!("added pending task");
		}
	}

	pub fn join(&mut self) {
		let mut guard = self.inner.pending_task.lock().unwrap();
		while matches!(*guard, PoolQueueSlot::Todo(_)) {
			debug!("waiting for idle...");
			guard = self.inner.pool_condvar.wait(guard).unwrap();
		}
		debug!("sending stop request...");
		*guard = PoolQueueSlot::Done;
		drop(guard);
		self.inner.workers_condvar.notify_all();
		debug!("joining...");
		let workers = mem::replace(&mut self.workers, vec![]);
		for w in workers {
			w.join().unwrap();
		}
	}
}

impl<WorkerCtx: Send + 'static> Drop for ThreadPool<WorkerCtx> {
	fn drop(&mut self) {
		self.join();
	}
}
