use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use dashmap::DashMap;

use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

use super::pool::{EngineLease, PoolError, SharedPool};

pub struct Session {
    pub id: String,
    pub engine: Mutex<Option<EngineLease>>,
    capacity: Mutex<Option<OwnedSemaphorePermit>>,
    pub created_at: Instant,
    last_active: Mutex<Instant>,
}

impl Session {
    pub async fn touch(&self) {
        *self.last_active.lock().await = Instant::now();
    }

    pub async fn last_active(&self) -> Instant {
        *self.last_active.lock().await
    }
}

pub struct SessionManager {
    sessions: DashMap<String, Arc<Session>>,
    pool: SharedPool,
    capacity: Arc<Semaphore>,
    idle_timeout: Duration,
}

impl SessionManager {
    pub fn new(pool: SharedPool, max_sessions: usize, idle_timeout: Duration) -> Self {
        Self {
            sessions: DashMap::new(),
            pool,
            capacity: Arc::new(Semaphore::new(max_sessions)),
            idle_timeout,
        }
    }

    pub async fn create_session(&self) -> Result<Arc<Session>, SessionError> {
        let capacity = Arc::clone(&self.capacity)
            .try_acquire_owned()
            .map_err(|_| SessionError::MaxSessionsReached)?;
        let engine = self.pool.acquire().await.map_err(SessionError::Pool)?;
        let id = generate_session_id();
        let now = Instant::now();

        let session = Arc::new(Session {
            id: id.clone(),
            engine: Mutex::new(Some(engine)),
            capacity: Mutex::new(Some(capacity)),
            created_at: now,
            last_active: Mutex::new(now),
        });

        self.sessions.insert(id, Arc::clone(&session));
        Ok(session)
    }

    pub async fn destroy_session(&self, id: &str) {
        if let Some((_, session)) = self.sessions.remove(id) {
            let engine = session.engine.lock().await.take();
            drop(engine);
            session.capacity.lock().await.take();
        }
    }

    pub fn get_session(&self, id: &str) -> Option<Arc<Session>> {
        self.sessions.get(id).map(|entry| Arc::clone(entry.value()))
    }

    pub async fn cleanup_idle(&self) {
        let now = Instant::now();
        let mut expired = Vec::new();

        // Never retain a DashMap shard guard over an await: removal may need
        // the same shard while another task is updating a session.
        let sessions: Vec<_> = self
            .sessions
            .iter()
            .map(|entry| Arc::clone(entry.value()))
            .collect();
        for session in sessions {
            let last = session.last_active().await;
            if now.saturating_duration_since(last) > self.idle_timeout {
                expired.push(session.id.clone());
            }
        }

        for id in expired {
            self.destroy_session(&id).await;
        }
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }
}

#[derive(Debug)]
pub enum SessionError {
    MaxSessionsReached,
    Pool(PoolError),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MaxSessionsReached => write!(f, "maximum sessions reached"),
            Self::Pool(e) => write!(f, "pool error: {e}"),
        }
    }
}

impl std::error::Error for SessionError {}

fn generate_session_id() -> String {
    use std::time::SystemTime;
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    format!("sess-{ts:x}-{sequence:x}")
}

pub type SharedSessionManager = Arc<SessionManager>;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn session_capacity_is_reserved_before_engine_initialization() {
        let pool = super::super::pool::model_free_pool(2);
        let manager = SessionManager::new(pool, 1, Duration::from_secs(60));
        let (first, second) = tokio::join!(manager.create_session(), manager.create_session());
        assert_ne!(first.is_ok(), second.is_ok());
        let retained = first.or(second).unwrap();
        manager.destroy_session(&retained.id).await;
        // An external reader holding an expired session cannot retain capacity.
        assert!(retained.engine.lock().await.is_none());
        let next = manager.create_session().await.unwrap();
        manager.destroy_session(&next.id).await;
        drop(next);
        drop(retained);
        drop(manager);
    }
}
