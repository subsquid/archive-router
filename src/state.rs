use std::ops::Deref;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use crate::interval::{Interval, Range};
use crate::util::Atom;


#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct WorkerId([u8; 32]);


impl From<&str> for WorkerId {
    fn from(s: &str) -> Self {
        WorkerId(blake3::hash(s.as_bytes()).into())
    }
}


#[derive(Clone)]
struct Worker {
    worker_id: WorkerId,
    desired_range: Arc<Range>,
    state: Atom<WorkerState>,
}


#[derive(Clone)]
struct WorkerState {
    url: Arc<str>,
    available_range: Range,
    last_ping: SystemTime
}


pub struct PingMessage {
    worker_id: Box<str>,
    worker_url: Box<str>,
    worker_state: Option<PingMessageWorkerState>
}


pub struct PingMessageWorkerState {
    dataset: Box<str>,
    range: Range
}


pub struct PingResponse {
    dataset: Arc<str>,
    range: Arc<Range>
}


pub struct StateManager {
    dataset: Arc<str>,
    workers: Atom<Vec<Worker>>,
    intervals: Mutex<Vec<Interval>>,
    replication: usize
}


unsafe impl Send for StateManager {}
unsafe impl Sync for StateManager {}


impl StateManager {
    pub fn get_worker(&self, first_block: u32) -> Option<Arc<str>> {
        let now = SystemTime::now();

        let candidates: Vec<_> = self.workers.get()
            .iter()
            .filter_map(|w| {
                let state = w.state.get();
                let since_last_ping = now.duration_since(state.last_ping).unwrap_or(Duration::from_secs(0));

                let is_suitable = since_last_ping < Duration::from_secs(30)
                    && state.available_range.has(first_block)
                    && w.desired_range.has(first_block);

                if is_suitable {
                    Some(state.url.clone())
                } else {
                    None
                }
            }).collect();

        if candidates.len() == 0 {
            None
        } else {
            let i: usize = rand::random();
            Some(candidates[i % candidates.len()].clone())
        }
    }

    pub fn ping(&self, msg: PingMessage) -> PingResponse {
        let worker_id = WorkerId::from(msg.worker_id.deref());

        let state = Arc::new(WorkerState {
            url: msg.worker_url.into(),
            available_range: match msg.worker_state {
                Some(s) if s.dataset.deref() == self.dataset.deref() => s.range,
                _ => Range::empty()
            },
            last_ping: SystemTime::now()
        });

        let mut desired_range: Option<Arc<Range>> = None;

        self.workers.update(|workers| {
            if let Some(w) = workers.iter().find(|w| w.worker_id == worker_id) {
                w.state.set(state.clone());
                desired_range = Some(w.desired_range.clone());
                None
            } else {
                let range = Arc::new(state.available_range.clone());
                desired_range = Some(range.clone());
                let w = Worker {
                    worker_id,
                    state: Atom::new(state.clone()),
                    desired_range: range
                };
                Some(Arc::new(
                    workers.iter().cloned().chain(std::iter::once(w)).collect()
                ))
            }
        });

        PingResponse {
            dataset: self.dataset.clone(),
            range: desired_range.unwrap()
        }
    }

    pub fn schedule(&self) {
        let now = SystemTime::now();
        let lock = self.intervals.lock().unwrap();
        let intervals = lock.deref();
        let mut workers = self.workers.get().deref().clone();

        // remove dead workers
        workers.retain(|w| {
            let since_last_ping = now.duration_since(w.state.get().last_ping).unwrap_or(Duration::from_secs(0));
            since_last_ping < Duration::from_secs(10 * 60)
        });

        let mut desired_ranges = workers
            .iter()
            .map(|w| w.desired_range.deref().clone())
            .collect::<Vec<_>>();

        for &interval in intervals.iter() {
            let holders: Vec<usize> = desired_ranges
                .iter()
                .enumerate()
                .filter_map(|(idx, r)| {
                    if r.has(interval.begin()) {
                        Some(idx)
                    } else {
                        None
                    }
                })
                .collect();
        }
    }
}
