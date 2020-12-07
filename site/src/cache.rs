use anyhow::Result;
use std::{
    future::Future,
    ops::Deref,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, RwLock};

pub struct Cache<T> {
    lifetime: Duration,
    data: RwLock<Option<Cached<T>>>,
    write_data: Mutex<Option<Cached<T>>>,
}

pub struct Cached<T> {
    eol: Instant,
    version: u64,
    data: Arc<T>,
}

impl<T> Clone for Cached<T> {
    fn clone(&self) -> Self {
        Self {
            eol: self.eol,
            version: self.version,
            data: self.data.clone(),
        }
    }
}

impl<T> Cached<T> {
    pub fn max_age(&self) -> u32 {
        let now = Instant::now();
        if self.eol < now {
            0
        } else {
            (self.eol - now).as_secs() as u32
        }
    }
}

impl<T> Deref for Cached<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T> Cache<T> {
    pub fn new(lifetime: Duration) -> Self {
        Self {
            lifetime,
            data: RwLock::new(None),
            write_data: Mutex::new(None),
        }
    }

    pub async fn get<G, F>(&self, f: F) -> Result<Cached<T>>
    where
        F: FnOnce() -> G,
        G: Future<Output = Result<T>>,
    {
        let data = self.data.read().await.clone();
        let now = Instant::now();
        match data {
            None => self.reload_data(f, None).await,
            Some(d) if d.eol <= now => self.reload_data(f, Some(d.version)).await,
            Some(d) => Ok(d),
        }
    }

    async fn reload_data<G, F>(
        &self,
        f: F,
        expected_version: Option<u64>,
    ) -> Result<Cached<T>>
    where
        F: FnOnce() -> G,
        G: Future<Output = Result<T>>,
    {
        let mut lock = self.write_data.lock().await;
        let actual_version = lock.as_ref().map(|l| l.version);
        if actual_version != expected_version {
            return Ok(lock.as_ref().unwrap().clone());
        }
        let next_version = actual_version.unwrap_or(0) + 1;
        let cached = Cached {
            data: Arc::new(f().await?),
            eol: Instant::now() + self.lifetime,
            version: next_version,
        };
        *lock = Some(cached.clone());
        *self.data.write().await = Some(cached.clone());
        Ok(cached)
    }
}
