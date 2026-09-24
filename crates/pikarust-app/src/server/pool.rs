use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use pikarust_core::engine::{Engine, EngineError};
use pikarust_core::nnue::Network;

#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub max_engines: usize,
    pub threads_per_engine: usize,
    pub hash_mb_per_engine: usize,
    /// An explicit model path fails closed when missing or invalid.
    pub nnue_file: Option<PathBuf>,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_engines: 8,
            threads_per_engine: 1,
            hash_mb_per_engine: 16,
            nnue_file: None,
        }
    }
}

#[derive(Default)]
struct PoolState {
    available: Vec<Engine>,
    /// Includes reserved engines whose initialization is still in progress.
    total: usize,
}

pub struct EnginePool {
    state: Mutex<PoolState>,
    config: PoolConfig,
    network: OnceLock<Result<Option<Arc<Network>>, String>>,
}

impl EnginePool {
    pub fn new(config: PoolConfig) -> Self {
        Self {
            state: Mutex::new(PoolState::default()),
            config,
            network: OnceLock::new(),
        }
    }

    /// Reserve capacity atomically and return a lease that releases on every
    /// exit path, including errors, task cancellation, and panic unwinding.
    pub async fn acquire(self: &Arc<Self>) -> Result<EngineLease, PoolError> {
        let engine = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(engine) = state.available.pop() {
                drop(state);
                Some(Box::new(engine))
            } else if state.total < self.config.max_engines {
                state.total += 1;
                drop(state);
                None
            } else {
                return Err(PoolError::Exhausted);
            }
        };
        let mut lease = EngineLease {
            engine,
            pool: Arc::clone(self),
        };
        // Model loading and joining a previous worker must not block Tokio.
        tokio::task::spawn_blocking(move || {
            if lease.engine.is_none() {
                let network = lease
                    .pool
                    .network
                    .get_or_init(|| {
                        let engine = lease
                            .pool
                            .config
                            .nnue_file
                            .as_ref()
                            .map_or_else(Engine::new, Engine::with_nnue_file);
                        engine.map(|e| e.network()).map_err(|e| e.to_string())
                    })
                    .as_ref()
                    .map_err(|message| PoolError::Initialization(message.clone()))?;
                lease.engine = Some(Box::new(
                    network
                        .as_ref()
                        .map_or_else(Engine::without_nnue, |network| {
                            Engine::with_network(Arc::clone(network))
                        })
                        .map_err(PoolError::Engine)?,
                ));
            }
            lease.new_game().map_err(PoolError::Engine)?;
            let threads = lease.pool.config.threads_per_engine.to_string();
            let hash = lease.pool.config.hash_mb_per_engine.to_string();
            for (name, value) in [
                ("Threads", threads.as_str()),
                ("Hash", hash.as_str()),
                ("MultiPV", "1"),
                ("Move Overhead", "10"),
                ("Ponder", "false"),
                ("UCI_ShowWDL", "false"),
            ] {
                lease.set_option(name, value).map_err(PoolError::Engine)?;
            }
            Ok(lease)
        })
        .await
        .map_err(|error| PoolError::Initialization(error.to_string()))?
    }

    pub fn active_count(&self) -> usize {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.total - state.available.len()
    }

    pub fn available_count(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .available
            .len()
    }
}

/// Exclusive engine ownership. Dropping the lease requests cancellation and
/// returns capacity immediately; its next owner joins and resets old workers.
pub struct EngineLease {
    engine: Option<Box<Engine>>,
    pool: Arc<EnginePool>,
}

impl Deref for EngineLease {
    type Target = Engine;

    fn deref(&self) -> &Self::Target {
        self.engine.as_deref().expect("initialized engine lease")
    }
}

impl DerefMut for EngineLease {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.engine
            .as_deref_mut()
            .expect("initialized engine lease")
    }
}

impl Drop for EngineLease {
    fn drop(&mut self) {
        let mut state = self
            .pool
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(engine) = self.engine.take() {
            engine.stop();
            state.available.push(*engine);
        } else {
            state.total -= 1;
        }
    }
}

#[derive(Debug)]
pub enum PoolError {
    Exhausted,
    Engine(EngineError),
    Initialization(String),
}

impl std::fmt::Display for PoolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exhausted => write!(f, "engine pool exhausted"),
            Self::Engine(e) => write!(f, "engine error: {e}"),
            Self::Initialization(message) => write!(f, "engine initialization: {message}"),
        }
    }
}

impl std::error::Error for PoolError {}

pub type SharedPool = Arc<EnginePool>;

#[cfg(test)]
pub(super) fn model_free_pool(max_engines: usize) -> SharedPool {
    let pool = Arc::new(EnginePool::new(PoolConfig {
        max_engines,
        ..PoolConfig::default()
    }));
    assert!(pool.network.set(Ok(None)).is_ok());
    pool
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pool() -> SharedPool {
        model_free_pool(1)
    }

    #[tokio::test]
    async fn capacity_is_atomic_and_leases_restore_capacity() {
        let pool = test_pool();
        let (a, b) = tokio::join!(pool.acquire(), pool.acquire());
        assert_ne!(a.is_ok(), b.is_ok());
        assert_eq!(pool.active_count(), 1);
        drop(a);
        drop(b);
        assert_eq!(pool.active_count(), 0);
        let mut lease = pool.acquire().await.unwrap();
        assert!(lease.set_position("invalid", &[]).is_err());
        drop(lease);
        assert!(pool.acquire().await.is_ok());
    }

    #[tokio::test]
    async fn invalid_configuration_does_not_leak_capacity() {
        let pool = Arc::new(EnginePool::new(PoolConfig {
            max_engines: 1,
            nnue_file: Some("__missing_model__.nnue".into()),
            ..PoolConfig::default()
        }));
        assert!(pool.acquire().await.is_err());
        assert_eq!(pool.active_count(), 0);
        assert!(pool.acquire().await.is_err());
        assert_eq!(pool.active_count(), 0);
    }

    #[tokio::test]
    async fn releasing_a_search_cancels_and_resets_before_reuse() {
        let pool = test_pool();
        let mut engine = pool.acquire().await.unwrap();
        let handle = engine.go(&pikarust_core::engine::SearchLimits {
            infinite: true,
            ..Default::default()
        });
        drop(engine);
        let mut next = pool.acquire().await.unwrap();
        let result = next
            .go(&pikarust_core::engine::SearchLimits {
                depth: Some(1),
                ..Default::default()
            })
            .wait();
        drop(handle);
        assert_ne!(result.best_move, pikarust_core::types::Move::NONE);
    }
}
