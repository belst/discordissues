use anyhow::Result;
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        RwLock,
    },
};

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", content = "id")]
enum DiscordTarget {
    Guild(u64),
    Channel(u64),
}

#[derive(Debug, Deserialize)]
pub struct Config {
    discord_token: String,
    database_url: String,
    github_token: String,
    mapping: HashMap<String, DiscordTarget>,
    #[serde(skip, default)]
    mapping_rev: RwLock<HashMap<DiscordTarget, String>>,
    #[serde(skip, default)]
    initialized: AtomicBool,
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
        mapping_rev.extend(self.mapping.iter().map(|(k, &v)| (v, k.clone())));
        self.initialized.store(true, Ordering::Release);
    }

    pub fn get_github_repo(&self, (channel_id, guild_id): (u64, Option<u64>)) -> Option<String> {
        self.initialize();

        self.mapping_rev
            .read()
            .unwrap()
            .get(&DiscordTarget::Channel(channel_id))
            .cloned()
            .or_else(|| {
                guild_id.and_then(|id| {
                    self.mapping_rev
                        .read()
                        .unwrap()
                        .get(&DiscordTarget::Guild(id))
                        .cloned()
                })
            })
    }

    pub fn discord_token(&self) -> &str {
        &self.discord_token
    }

    pub fn github_token(&self) -> &str {
        &self.github_token
    }

    pub fn database_url(&self) -> &str {
        &self.database_url
    }
}
