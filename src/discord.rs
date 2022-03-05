use std::sync::Arc;

use anyhow::Result;
use futures::channel::mpsc;
use futures::SinkExt;
use futures::{channel::mpsc::UnboundedReceiver, stream::StreamExt};
use twilight_cache_inmemory::InMemoryCache;
use twilight_gateway::{
    cluster::{Cluster, ShardScheme},
    Event,
};
use twilight_model::gateway::Intents;

pub async fn run() -> Result<(UnboundedReceiver<Event>, Arc<InMemoryCache>)> {
    let token = std::env::var("DISCORD_TOKEN")?;

    let scheme = ShardScheme::Auto;

    let intents = Intents::GUILDS | Intents::GUILD_MESSAGES | Intents::GUILD_MESSAGE_REACTIONS;

    let (cluster, mut events) = Cluster::builder(token.to_owned(), intents)
        .shard_scheme(scheme)
        .build()
        .await?;

    let cluster = Arc::new(cluster);
    let cluster_spawn = Arc::clone(&cluster);

    tokio::spawn(async move {
        cluster_spawn.up().await;
    });

    let cache = Arc::new(InMemoryCache::new());
    let cache_prime = cache.clone();
    let (tx, rx) = mpsc::unbounded();
    tokio::spawn(async move {
        while let Some((shard_id, event)) = events.next().await {
            cache_prime.update(&event);

            let mut txprime = tx.clone();
            match txprime.send(event).await {
                Err(e) => tracing::error!(shard_id, "Error sending event to channel: {}", e),
                _ => {}
            }
        }
    });

    Ok((rx, cache))
}
