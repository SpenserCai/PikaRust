use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use pikarust_core::engine::Engine;
use tokio::sync::Mutex;

use super::pool::{PoolError, SharedPool};

pub struct Session {
    pub id: String,
    pub engine: Mutex<Option<Engine>>,
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
    max_sessions: usize,
    idle_timeout: Duration,
}

impl SessionManager {
    pub fn new(pool: SharedPool, max_sessions: usize, idle_timeout: Duration) -> Self {
        Self {
            sessions: DashMap::new(),
            pool,
            max_sessions,
            idle_timeout,
        }
    }

    pub async fn create_session(&self) -> Result<Arc<Session>, SessionError> {
        if self.sessions.len() >= self.max_sessions {
            return Err(SessionError::MaxSessionsReached);
        }

        let engine = self.pool.acquire().await.map_err(SessionError::Pool)?;
        let id = generate_session_id();
        let now = Instant::now();

        let session = Arc::new(Session {
            id: id.clone(),
            engine: Mutex::new(Some(engine)),
            created_at: now,
            last_active: Mutex::new(now),
        });

        self.sessions.insert(id, Arc::clone(&session));
        Ok(session)
    }

    pub async fn destroy_session(&self, id: &str) {
        if let Some((_, session)) = self.sessions.remove(id) {
            let engine = session.engine.lock().await.take();
            if let Some(engine) = engine {
                Box::pin(self.pool.release(engine)).await;
            }
        }
    }

    pub fn get_session(&self, id: &str) -> Option<Arc<Session>> {
        self.sessions.get(id).map(|entry| Arc::clone(entry.value()))
    }

    pub async fn cleanup_idle(&self) {
        let now = Instant::now();
        let mut expired = Vec::new();

        for entry in &self.sessions {
            let last = entry.value().last_active().await;
            if now.duration_since(last) > self.idle_timeout {
                expired.push(entry.key().clone());
            }
        }

        for id in expired {
            Box::pin(self.destroy_session(&id)).await;
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
    let ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("sess-{ts:x}")
}

pub type SharedSessionManager = Arc<SessionManager>;
