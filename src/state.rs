use std::ops::Deref;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use crate::interval::Interval;
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
    state: Atom<WorkerState>,
    desired_ranges: Atom<[Interval]>
}


#[derive(Clone)]
struct WorkerState {
    url: Box<str>,
    available_ranges: Box<[Interval]>,
    last_ping: SystemTime
}


pub struct PingMessage {
    worker_id: Box<str>,
    worker_url: Box<str>,
    worker_state: Option<PingMessageWorkerState>
}


pub struct PingMessageWorkerState {
    dataset: Box<str>,
    ranges: Box<[Interval]>
}


pub struct PingResponse {
    dataset: Arc<str>,
    ranges: Arc<[Interval]>
}


pub struct StateManager {
    dataset: Arc<str>,
    workers: Atom<[Worker]>,
    intervals: Mutex<Vec<Interval>>
}


unsafe impl Send for StateManager {}
unsafe impl Sync for StateManager {}


impl StateManager {
    pub fn ping(&self, msg: PingMessage) -> PingResponse {
        let worker_id = WorkerId::from(msg.worker_id.deref());

        let state = Arc::new(WorkerState {
            url: msg.worker_url,
            available_ranges: match msg.worker_state {
                Some(s) if s.dataset.deref() == self.dataset.deref() => s.ranges,
                _ => Box::new([])
            },
            last_ping: SystemTime::now()
        });

        let mut desired_ranges: Option<Arc<[Interval]>> = None;

        self.workers.update(|workers| {
            if let Some(w) = workers.iter().find(|w| w.worker_id == worker_id) {
                w.state.set(state.clone());
                desired_ranges = Some(w.desired_ranges.get());
                None
            } else {
                let ranges = Arc::from_iter(state.available_ranges.iter().cloned());
                desired_ranges = Some(ranges.clone());
                let w = Worker {
                    worker_id,
                    state: Atom::new(state.clone()),
                    desired_ranges: Atom::new(ranges)
                };
                Some(Arc::from_iter(
                    workers.iter().cloned().chain(std::iter::once(w))
                ))
            }
        });

        PingResponse {
            dataset: self.dataset.clone(),
            ranges: desired_ranges.unwrap()
        }
    }
}
