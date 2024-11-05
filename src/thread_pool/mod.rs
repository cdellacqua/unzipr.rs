use std::{collections::VecDeque, sync::{Arc, Condvar, Mutex}, thread::{self, JoinHandle}};

use itertools::Itertools;
use tracing::debug;

pub struct ThreadPool<WorkerCtx: Send + 'static = ()> {
	inner: Arc<ThreadPoolShared<WorkerCtx>>,
	num_threads: usize,
	workers: Vec<JoinHandle<()>>,
}

pub struct ThreadPoolShared<WorkerCtx: Send + 'static> {
	workers_condvar: Condvar,
	pool_condvar: Condvar,
	task_queue: Mutex<Option<VecDeque<Box<dyn FnOnce(&mut WorkerCtx) + Send>>>>,
}

impl<WorkerCtx: Send + 'static> ThreadPool<WorkerCtx> {
	pub fn new(worker_ctxs: Vec<WorkerCtx>) -> Self {
		let inner = Arc::new(ThreadPoolShared {
			workers_condvar: Default::default(),
			pool_condvar: Default::default(),
			task_queue: Mutex::new(Some(VecDeque::with_capacity(worker_ctxs.len()))),
		});
		let workers = worker_ctxs
			.into_iter()
			.map(|mut ctx| {
				let inner_clone = inner.clone();
				thread::spawn(move || loop {
					let ThreadPoolShared {
						task_queue,
						workers_condvar,
						pool_condvar,
						..
					} = &*inner_clone;
					let mut guard = task_queue.lock().unwrap();
					while guard.as_ref().is_some_and(|x| x.len() == 0) {
						debug!("[worker] waiting...");
						guard = workers_condvar.wait(guard).unwrap();
					}
					if let Some(ref mut queue) = &mut *guard {
						debug!("[worker] running task...");
						let task_fn = queue.pop_front().unwrap();
						pool_condvar.notify_all();
						drop(guard);
						(task_fn)(&mut ctx);
					} else {
						debug!("[worker] quitting...");
						break;
					}
				})
			})
			.collect_vec();

		Self {
			inner,
			num_threads: workers.len(),
			workers,
		}
	}

	pub fn send_blocking<Task: FnOnce(&mut WorkerCtx) + Send + 'static>(&mut self, task: Task) {
		let mut guard = self.inner.task_queue.lock().unwrap();
		while guard.as_ref().is_some_and(|x| x.len() >= self.num_threads) {
			debug!("[send_blocking] waiting...");
			guard = self.inner.pool_condvar.wait(guard).unwrap();
		}
		if let Some(ref mut queue) = &mut *guard {
			debug!("[send_blocking] pushing...");
			queue.push_back(Box::new(task));
			self.inner.workers_condvar.notify_one();
		}
	}

	pub fn join(self) {
		let mut guard = self.inner.task_queue.lock().unwrap();
		while guard.as_ref().is_some_and(|x| x.len() > 0) {
			debug!("[join] waiting...");
			guard = self.inner.pool_condvar.wait(guard).unwrap();
		}
		debug!("[join] taking...");
		guard.take();
		drop(guard);
		self.inner.workers_condvar.notify_all();
		debug!("[join] joining...");
		for w in self.workers {
			w.join().unwrap();
		}
	}
}
