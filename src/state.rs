use std::ops::Deref;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use crate::interval::Interval;
use crate::util::Atom;

pub type WorkerId = u128;


#[derive(Clone)]
struct Worker {
    worker_id: WorkerId,
    state: Atom<WorkerState>,
    desired_ranges: Atom<Box<[Interval]>>
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
    ranges: Arc<Box<[Interval]>>
}


pub struct StateManager {
    dataset: Arc<str>,
    workers: Atom<Box<[Worker]>>,
    intervals: Mutex<Vec<Interval>>
}


unsafe impl Send for StateManager {}
unsafe impl Sync for StateManager {}


impl StateManager {
    pub fn ping(&self, msg: PingMessage) -> PingResponse {
        let worker_id = 0u128;

        let state = Arc::new(WorkerState {
            url: msg.worker_url,
            available_ranges: match msg.worker_state {
                Some(s) if s.dataset.deref() == self.dataset.deref() => s.ranges,
                _ => Box::new([])
            },
            last_ping: SystemTime::now()
        });

        let mut desired_ranges: Option<Arc<Box<[Interval]>>> = None;

        self.workers.update(|workers| {
            if let Some(w) = workers.iter().find(|w| w.worker_id == worker_id) {
                w.state.set(state.clone());
                desired_ranges = Some(w.desired_ranges.get());
                None
            } else {
                let ranges = Arc::new(state.available_ranges.clone());
                desired_ranges = Some(ranges.clone());
                let w = Worker {
                    worker_id,
                    state: Atom::new(state.clone()),
                    desired_ranges: Atom::new(ranges)
                };
                let mut new_list: Vec<Worker> = Vec::with_capacity(workers.len() + 1);
                new_list.extend(workers.iter().map(|w| w.clone()));
                new_list.push(w);
                Some(Arc::new(new_list.into_boxed_slice()))
            }
        });

        PingResponse {
            dataset: self.dataset.clone(),
            ranges: desired_ranges.unwrap()
        }
    }
}
