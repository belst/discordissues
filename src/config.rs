use anyhow::Result;
use jsonwebtoken::EncodingKey;
use octocrab::models::AppId;
use serde::Deserialize;
use std::{
    collections::HashMap,
    path,
    sync::{
        atomic::{AtomicBool, Ordering},
        RwLock,
    },
};

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", content = "id")]
enum Target {
    Guild(u64),
    Channel(u64),
}
#[derive(Clone, Debug, Deserialize)]
struct DiscordTarget {
    target: Target,
    roles: Vec<u64>,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    discord_token: String,
    database_url: String,
    github_app_id: u64,
    github_private_key: path::PathBuf,
    mapping: HashMap<String, DiscordTarget>,
    #[serde(skip, default)]
    mapping_rev: RwLock<HashMap<Target, String>>,
    #[serde(skip, default)]
    initialized: AtomicBool,
    #[serde(skip, default)]
    // not part of initialize cause it might fail
    private_key_encoded: RwLock<Option<EncodingKey>>,
}

impl Config {
    pub fn from_file(path: &std::path::Path) -> Result<Self> {
        let s = std::fs::read_to_string(path)?;
        toml::from_str(&s).map_err(From::from)
    }

    fn initialize(&self) {
        if self.initialized.load(Ordering::Acquire) {
            return;
        }
        let mut mapping_rev = self.mapping_rev.write().unwrap();
        mapping_rev.clear();
        mapping_rev.extend(self.mapping.iter().map(|(k, v)| (v.target, k.clone())));
        self.initialized.store(true, Ordering::Release);
    }

    pub fn get_github_repo(&self, (channel_id, guild_id): (u64, Option<u64>)) -> Option<String> {
        self.initialize();

        self.mapping_rev
            .read()
            .unwrap()
            .get(&Target::Channel(channel_id))
            .cloned()
            .or_else(|| {
                guild_id.and_then(|id| {
                    self.mapping_rev
                        .read()
                        .unwrap()
                        .get(&Target::Guild(id))
                        .cloned()
                })
            })
    }

    pub fn check_permission(&self, repo: &str, role_id: u64) -> bool {
        self.mapping
            .get(repo)
            .map(|m| m.roles.contains(&role_id))
            .unwrap_or(false)
    }

    pub fn discord_token(&self) -> &str {
        &self.discord_token
    }

    pub fn database_url(&self) -> &str {
        &self.database_url
    }

    pub fn github_app_id(&self) -> AppId {
        self.github_app_id.into()
    }

    fn load_private_key(&self) -> anyhow::Result<()> {
        // all sync and no async cause only getting called once at init
        use std::fs::File;
        use std::io::Read;
        let mut f = File::open(&self.github_private_key)?;
        let mut buff = vec![];
        f.read_to_end(&mut buff)?;
        let mut lock = self
            .private_key_encoded
            .write()
            .expect("Poison error on write RwLock");

        *lock = Some(EncodingKey::from_rsa_pem(&buff)?);

        Ok(())
    }

    pub fn github_private_key(&self) -> anyhow::Result<EncodingKey> {
        let lock = self
            .private_key_encoded
            .read()
            .expect("Poison error on read RwLock");
        if lock.is_none() {
            drop(lock); // need to drop read lock so we can write
            self.load_private_key()?;
        }

        let lock = self
            .private_key_encoded
            .read()
            .expect("Poison error on read RwLock");
        let key = lock.as_ref();

        Ok(key.unwrap().clone())
    }
}
