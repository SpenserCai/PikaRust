use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use pikarust_core::engine::{Engine, EngineError};
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub max_engines: usize,
    pub threads_per_engine: usize,
    pub hash_mb_per_engine: usize,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_engines: 8,
            threads_per_engine: 1,
            hash_mb_per_engine: 16,
        }
    }
}

pub struct EnginePool {
    available: Mutex<Vec<Engine>>,
    config: PoolConfig,
    active_count: AtomicUsize,
}

impl EnginePool {
    pub fn new(config: PoolConfig) -> Self {
        Self {
            available: Mutex::new(Vec::new()),
            config,
            active_count: AtomicUsize::new(0),
        }
    }

    pub async fn acquire(&self) -> Result<Engine, PoolError> {
        let mut pool = self.available.lock().await;
        if let Some(engine) = pool.pop() {
            drop(pool);
            self.active_count.fetch_add(1, Ordering::Relaxed);
            return Ok(engine);
        }
        drop(pool);

        let total = self.active_count.load(Ordering::Relaxed) + self.available_count().await;
        if total >= self.config.max_engines {
            return Err(PoolError::Exhausted);
        }

        let mut engine = Engine::new().map_err(PoolError::Engine)?;
        engine
            .set_option("Threads", &self.config.threads_per_engine.to_string())
            .map_err(PoolError::Engine)?;
        engine
            .set_option("Hash", &self.config.hash_mb_per_engine.to_string())
            .map_err(PoolError::Engine)?;
        self.active_count.fetch_add(1, Ordering::Relaxed);
        Ok(engine)
    }

    pub async fn release(&self, mut engine: Engine) {
        let _ = engine.new_game();
        self.available.lock().await.push(engine);
        self.active_count.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn active_count(&self) -> usize {
        self.active_count.load(Ordering::Relaxed)
    }

    pub async fn available_count(&self) -> usize {
        self.available.lock().await.len()
    }
}

#[derive(Debug)]
pub enum PoolError {
    Exhausted,
    Engine(EngineError),
}

impl std::fmt::Display for PoolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exhausted => write!(f, "engine pool exhausted"),
            Self::Engine(e) => write!(f, "engine error: {e}"),
        }
    }
}

impl std::error::Error for PoolError {}

pub type SharedPool = Arc<EnginePool>;
