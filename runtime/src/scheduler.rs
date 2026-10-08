//! Fixed-interval server tick loop with durable authority admission.
//! Games execute native simulation; the platform only enforces who may tick.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use oasis_contracts::{ContractError, ContractResult, Id};
use crate::universe::{SimulationLease, Universe};

/// A trusted, fail-closed lease oracle. The implementation must verify the
/// *current* persistent lease, including expiration, holder and fencing epoch.
/// It must fail on storage/network errors, not treat them as authorization.
pub trait TickAuthorizer: Send + Sync + 'static {
    fn authorize(&self, world: Id, lease: SimulationLease) -> ContractResult<()>;
}

pub struct WorldRunner {
    stop: Arc<AtomicBool>,
    last_error: Arc<Mutex<Option<ContractError>>>,
    handle: Option<JoinHandle<()>>,
}

impl WorldRunner {
    /// The externally usable scheduler always consults durable authority on
    /// EVERY tick. An invalid lease halts the entire shard before simulation.
    pub fn start(
        universe: Arc<Mutex<Universe>>,
        world_id: Id,
        interval: Duration,
        authority: Arc<dyn TickAuthorizer>,
    ) -> ContractResult<Self> {
        Self::start_inner(universe, world_id, interval, Some(authority))
    }

    #[cfg(test)]
    pub(crate) fn start_unchecked_for_test(
        universe: Arc<Mutex<Universe>>, world: Id, interval: Duration,
    ) -> ContractResult<Self> {
        Self::start_inner(universe, world, interval, None)
    }

    fn start_inner(
        universe: Arc<Mutex<Universe>>, world_id: Id, interval: Duration,
        authority: Option<Arc<dyn TickAuthorizer>>,
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
                // Keep the in-memory shard locked while validating and
                // simulating. Concurrent logins/travel may not change its
                // claim set between authorization and the native step.
                let result = match universe.lock() {
                    Ok(mut core) => {
                        let claims=core.simulation_leases(world_id);
                        match claims {
                            Ok(claims) => {
                                let checked=claims.iter().try_for_each(|claim| {
                                    authority.as_ref().map_or(Ok(()), |guard|
                                        guard.authorize(world_id,*claim))
                                });
                                checked.and_then(|()| core.step_world(
                                    world_id,interval.as_nanos() as u64
                                ))
                            },
                            Err(error)=>Err(error),
                        }
                    },
                    Err(_) => Err(ContractError::Internal("universe lock poisoned".into())),
                };
                if let Err(error) = result {
                    if let Ok(mut status) = errors.lock() { *status = Some(error); }
                    break;
                }
                next += interval;
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
