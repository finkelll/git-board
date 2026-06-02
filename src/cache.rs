use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const OWNER_STALE_AFTER: Duration = Duration::from_secs(30);
const CACHE_WAIT: Duration = Duration::from_secs(30);
const CACHE_POLL: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub struct GlobalCacheSession {
    repo_dir: PathBuf,
    client_path: PathBuf,
    owner_path: PathBuf,
    is_owner: bool,
}

#[derive(Debug, Clone)]
pub struct GlobalCacheWorker {
    repo_dir: PathBuf,
    is_owner: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlobalCacheStatus {
    pub is_owner: bool,
    pub client_count: usize,
    pub cache_age: Option<Duration>,
}

impl GlobalCacheSession {
    pub fn register(repo: &str) -> Result<Self> {
        let repo_dir = cache_root().join(repo_key(repo));
        let clients_dir = repo_dir.join("clients");
        fs::create_dir_all(&clients_dir)
            .with_context(|| format!("failed to create cache dir {}", clients_dir.display()))?;

        let client_path = clients_dir.join(client_key());
        write_file(&client_path, "")?;

        let owner_path = repo_dir.join("owner");
        remove_stale_owner(&owner_path)?;
        let is_owner = claim_owner(&owner_path)?;

        Ok(Self {
            repo_dir,
            client_path,
            owner_path,
            is_owner,
        })
    }

    pub fn maintain(&mut self) -> Result<GlobalCacheStatus> {
        write_file(&self.client_path, "")?;

        if self.is_owner {
            write_file(&self.owner_path, "")?;
        } else {
            remove_stale_owner(&self.owner_path)?;
            self.is_owner = claim_owner(&self.owner_path)?;
        }

        self.status()
    }

    pub fn status(&self) -> Result<GlobalCacheStatus> {
        let clients_dir = self.repo_dir.join("clients");
        remove_stale_clients(&clients_dir)?;
        Ok(GlobalCacheStatus {
            is_owner: self.is_owner,
            client_count: client_count(&clients_dir)?,
            cache_age: modified_age(&self.repo_dir.join("cache.json"))?,
        })
    }

    pub fn worker(&self) -> GlobalCacheWorker {
        GlobalCacheWorker {
            repo_dir: self.repo_dir.clone(),
            is_owner: self.is_owner,
        }
    }
}

impl Drop for GlobalCacheSession {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.client_path);
        if self.is_owner {
            let _ = fs::remove_file(&self.owner_path);
        }

        let clients_dir = self.repo_dir.join("clients");
        let _ = remove_stale_clients(&clients_dir);
        if is_empty_dir(&clients_dir).unwrap_or(false) {
            let _ = fs::remove_dir_all(&self.repo_dir);
        }
    }
}

impl GlobalCacheWorker {
    pub fn is_owner(&self) -> bool {
        self.is_owner
    }

    pub fn read<T: DeserializeOwned>(&self) -> Result<T> {
        let path = self.repo_dir.join("cache.json");
        let deadline = SystemTime::now() + CACHE_WAIT;

        loop {
            if path.exists() {
                let text = fs::read_to_string(&path)
                    .with_context(|| format!("failed to read cache {}", path.display()))?;
                return serde_json::from_str(&text)
                    .with_context(|| format!("failed to parse cache {}", path.display()));
            }

            if SystemTime::now() >= deadline {
                anyhow::bail!("waiting for shared cache {}", path.display());
            }

            std::thread::sleep(CACHE_POLL);
        }
    }

    pub fn write<T: Serialize>(&self, value: &T) -> Result<()> {
        fs::create_dir_all(&self.repo_dir)
            .with_context(|| format!("failed to create cache dir {}", self.repo_dir.display()))?;
        let path = self.repo_dir.join("cache.json");
        let tmp = self.repo_dir.join(format!("cache.{}.tmp", unique_suffix()));
        let text = serde_json::to_string(value).context("failed to serialize cache")?;
        fs::write(&tmp, text)
            .with_context(|| format!("failed to write cache {}", tmp.display()))?;
        fs::rename(&tmp, &path).with_context(|| {
            format!(
                "failed to replace cache {} with {}",
                path.display(),
                tmp.display()
            )
        })?;
        Ok(())
    }
}

pub fn repo_client_pids(repo: &str) -> Result<Vec<u32>> {
    let repo_dir = cache_root().join(repo_key(repo));
    client_pids_in_repo_dir(&repo_dir)
}

pub fn all_client_pids() -> Result<Vec<u32>> {
    let Ok(entries) = fs::read_dir(cache_root()) else {
        return Ok(Vec::new());
    };

    let mut pids = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            pids.extend(client_pids_in_repo_dir(&entry.path())?);
        }
    }

    pids.sort_unstable();
    pids.dedup();
    Ok(pids)
}

fn client_pids_in_repo_dir(repo_dir: &PathBuf) -> Result<Vec<u32>> {
    let clients_dir = repo_dir.join("clients");
    remove_stale_clients(&clients_dir)?;
    let Ok(entries) = fs::read_dir(&clients_dir) else {
        return Ok(Vec::new());
    };

    let mut pids = Vec::new();
    for entry in entries {
        let entry = entry?;
        if let Some(pid) = client_pid_from_key(&entry.file_name().to_string_lossy()) {
            pids.push(pid);
        }
    }

    pids.sort_unstable();
    pids.dedup();
    Ok(pids)
}

fn cache_root() -> PathBuf {
    std::env::temp_dir().join("git-board-global-cache")
}

fn repo_key(repo: &str) -> String {
    format!(
        "{:016x}",
        fnv1a(repo.trim().to_ascii_lowercase().as_bytes())
    )
}

fn client_key() -> String {
    format!("{}-{}", std::process::id(), unique_suffix())
}

fn client_pid_from_key(key: &str) -> Option<u32> {
    key.split_once('-')
        .and_then(|(pid, _)| pid.parse::<u32>().ok())
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn claim_owner(owner_path: &PathBuf) -> Result<bool> {
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(owner_path)
    {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(error)
            .with_context(|| format!("failed to claim cache owner {}", owner_path.display())),
    }
}

fn remove_stale_owner(owner_path: &PathBuf) -> Result<()> {
    if modified_age(owner_path)?.is_some_and(|age| age > OWNER_STALE_AFTER) {
        let _ = fs::remove_file(owner_path);
    }
    Ok(())
}

fn remove_stale_clients(clients_dir: &PathBuf) -> Result<()> {
    let Ok(entries) = fs::read_dir(clients_dir) else {
        return Ok(());
    };

    for entry in entries {
        let entry = entry?;
        if modified_age(&entry.path())?.is_some_and(|age| age > OWNER_STALE_AFTER) {
            let _ = fs::remove_file(entry.path());
        }
    }

    Ok(())
}

fn client_count(clients_dir: &PathBuf) -> Result<usize> {
    let Ok(entries) = fs::read_dir(clients_dir) else {
        return Ok(0);
    };

    let mut count = 0;
    for entry in entries {
        entry?;
        count += 1;
    }

    Ok(count)
}

fn modified_age(path: &PathBuf) -> Result<Option<Duration>> {
    let Ok(metadata) = fs::metadata(path) else {
        return Ok(None);
    };
    let modified = metadata
        .modified()
        .with_context(|| format!("failed to read mtime {}", path.display()))?;
    Ok(SystemTime::now().duration_since(modified).ok())
}

fn is_empty_dir(path: &PathBuf) -> Result<bool> {
    Ok(fs::read_dir(path)
        .with_context(|| format!("failed to read dir {}", path.display()))?
        .next()
        .is_none())
}

fn write_file(path: &PathBuf, value: &str) -> Result<()> {
    fs::write(path, value).with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_client_pid_from_key() {
        assert_eq!(client_pid_from_key("123-456"), Some(123));
        assert_eq!(client_pid_from_key("bad-456"), None);
        assert_eq!(client_pid_from_key("123"), None);
    }
}
