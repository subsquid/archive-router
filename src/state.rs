use std::borrow::Borrow;
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
    desired_ranges: Atom<Arc<[Interval]>>
}


#[derive(Clone)]
struct WorkerState {
    url: Arc<str>,
    available_ranges: Arc<[Interval]>,
    last_ping: SystemTime
}


pub struct PingMessage {
    worker_id: Box<str>,
    worker_url: Arc<str>,
    worker_state: Option<PingMessageWorkerState>
}


pub struct PingMessageWorkerState {
    dataset: Box<str>,
    ranges: Arc<[Interval]>
}


pub struct PingResponse {
    dataset: Arc<str>,
    ranges: Arc<[Interval]>
}


pub struct StateManager {
    dataset: Arc<str>,
    workers: Atom<Box<[Worker]>>,
    intervals: Mutex<Vec<Interval>>,
    empty_interval_list: Arc<[Interval]>
}


impl StateManager {
    pub fn ping(&self, msg: &PingMessage) -> PingResponse {
        let worker_id = 0u128;

        let state = Arc::new(WorkerState {
            url: msg.worker_url.clone(),
            available_ranges: match &msg.worker_state {
                Some(s) if s.dataset.deref() == self.dataset.deref() => s.ranges.clone(),
                _ => self.empty_interval_list.clone()
            },
            last_ping: SystemTime::now()
        });

        let mut desired_ranges: Option<Arc<[Interval]>> = None;

        self.workers.update(|workers| {
            if let Some(w) = workers.iter().find(|w| w.worker_id == worker_id) {
                w.state.set(state.clone());
                desired_ranges = Some(w.desired_ranges.get().deref().clone());
                None
            } else {
                let ranges = state.available_ranges.clone();
                desired_ranges = Some(ranges.clone());
                let w = Worker {
                    worker_id,
                    state: Atom::new(state.clone()),
                    desired_ranges: Atom::new(Arc::new(ranges))
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
