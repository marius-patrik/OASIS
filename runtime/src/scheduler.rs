//! Fixed-interval, server-authoritative simulation scheduler. Native modules
//! remain responsible for their own game logic; the scheduler only advances
//! each independently hosted world instance.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use oasis_contracts::{ContractError, ContractResult, Id};
use crate::universe::Universe;

pub struct WorldRunner {
    stop: Arc<AtomicBool>,
    last_error: Arc<Mutex<Option<ContractError>>>,
    handle: Option<JoinHandle<()>>,
}

impl WorldRunner {
    pub fn start(
        universe: Arc<Mutex<Universe>>, world_id: Id, interval: Duration,
    ) -> ContractResult<Self> {
        if interval.is_zero() || interval.as_nanos() > u128::from(u64::MAX) {
            return Err(ContractError::InvalidData("invalid tick interval".into()));
        }
        {
            let core = universe.lock()
                .map_err(|_| ContractError::Internal("universe lock poisoned".into()))?;
            if !core.has_world(world_id) {
                return Err(ContractError::NotFound(world_id));
            }
        }
        let stop = Arc::new(AtomicBool::new(false));
        let last_error = Arc::new(Mutex::new(None));
        let done = Arc::clone(&stop);
        let errors = Arc::clone(&last_error);
        let handle = thread::spawn(move || {
            let mut next = Instant::now() + interval;
            while !done.load(Ordering::Relaxed) {
                let now = Instant::now();
                if now < next {
                    thread::sleep((next - now).min(Duration::from_millis(5)));
                    continue;
                }
                let result = match universe.lock() {
                    Ok(mut core) => core.step_world(world_id, interval.as_nanos() as u64),
                    Err(_) => Err(ContractError::Internal("universe lock poisoned".into())),
                };
                if let Err(error) = result {
                    if let Ok(mut status) = errors.lock() { *status = Some(error); }
                    break;
                }
                next += interval;
                // We do not replay unbounded backlog after host stalls.
                if next < Instant::now() { next = Instant::now() + interval; }
            }
        });
        Ok(Self { stop, last_error, handle:Some(handle) })
    }

    pub fn last_error(&self) -> Option<ContractError> {
        self.last_error.lock().ok().and_then(|error| error.clone())
    }
}

impl Drop for WorldRunner {
    fn drop(&mut self) {
        self.stop.store(true,Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
