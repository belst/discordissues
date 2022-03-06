use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use clap::Parser;
use config::Config;
use futures::StreamExt;
use octocrab::Octocrab;
use state::State;
use twilight_cache_inmemory::InMemoryCache;
use twilight_gateway::Event;
use twilight_http::Client;
use twilight_model::{channel::ReactionType, id::Id};

mod config;
mod discord;
mod github;
mod state;

#[derive(Debug, Parser)]
struct Args {
    #[clap(long, short, default_value = "config.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing::trace!("Starting program");
    tracing_subscriber::fmt().init();

    let args = Args::parse();

    let config = Arc::new(Config::from_file(&args.config)?);

    let state = Arc::new(state::State::new(config.database_url()).await?);

    let github = octocrab::OctocrabBuilder::new()
        .personal_token(config.github_token().into())
        .build()?;

    let (mut events, cache) = discord::run(config.discord_token().into()).await?;

    let discord = Arc::new(Client::new(config.discord_token().into()));

    while let Some(e) = events.next().await {
        let state = state.clone();
        let discord = discord.clone();
        let github = github.clone();
        let config = config.clone();
        let cache = cache.clone();
        tokio::spawn(async move {
            handle_discord_event(e, state, discord, cache, github, config).await
        });
    }

    Ok(())

    // loop {
    //     tokio::select! {
    //         event = events.next() => {
    //             if let Some(e) {
    //                 handle_discord_event(e, state.clone())
    //             } else {
    //                 break;
    //             }
    //         }
    //     }
    // }
}

#[tracing::instrument]
async fn handle_discord_event(
    event: Event,
    state: Arc<State>,
    discord: Arc<Client>,
    cache: Arc<InMemoryCache>,
    github: Octocrab,
    config: Arc<Config>,
) -> Result<()> {
    match event {
        Event::MessageCreate(msg) if msg.thread.is_none() => {
            tracing::trace!(msg = msg.id.get(), "Message create event");
            if msg.author.bot {
                tracing::info!(msg = msg.id.get(), author = %msg.author.name, "Ignoring bot messages");
                return Ok(());
            }
            if let Some(issue_nr) = state.get_issue(msg.channel_id).await? {
                let (user, repo) = match config
                    .get_github_repo((msg.channel_id.get(), msg.guild_id.map(Id::get)))
                    .and_then(|s| s.split_once('/').map(|(l, r)| (l.to_owned(), r.to_owned())))
                {
                    Some(repo) => repo,
                    None => {
                        anyhow::bail!("No repository found or invalid repository string in config")
                    }
                };

                let commentstr = format!(
                    "New comment from @{}\n\n{}\n\n[Link](https://discord.com/channels/{}/{}/{})",
                    msg.author.name,
                    msg.content,
                    msg.guild_id.unwrap(), // does not work in private messages
                    msg.channel_id,
                    msg.id
                );

                github
                    .issues(&user, &repo)
                    .create_comment(issue_nr, commentstr)
                    .await?;
            }
        }
        Event::ReactionAdd(rct)
            if rct.emoji
                == ReactionType::Unicode {
                    name: "🐛".into()
                } =>
        {
            // TODO: replace with cache when CacheMessage contains `thread`
            let msg = discord
                .message(rct.channel_id, rct.message_id)
                .exec()
                .await?
                .model()
                .await?;

            if let Some(thread) = msg.thread {
                if let Some(issue_id) = state.get_issue(thread.id()).await? {
                    tracing::info!(
                        issue_id,
                        thread_id = thread.id().get(),
                        channel_id = rct.channel_id.get(),
                        message_id = rct.message_id.get(),
                        "Issue already created"
                    );
                    return Ok(());
                }
                // sync comments
                return Ok(());
            }

            let (user, repo) = match config
                .get_github_repo((rct.channel_id.get(), rct.guild_id.map(Id::get)))
                .and_then(|s| s.split_once('/').map(|(l, r)| (l.to_owned(), r.to_owned())))
            {
                Some(repo) => repo,
                None => {
                    tracing::info!(rct = ?rct, "Reaction on non tracked channel/guild");
                    return Ok(());
                }
            };

            let msg = discord
                .message(rct.channel_id, rct.message_id)
                .exec()
                .await?
                .model()
                .await?;

            let title = &msg.content[..30.min(msg.content.len())];
            let issue = github
                .issues(&user, &repo)
                .create(title)
                .body(&msg.content)
                .labels(vec!["discord".into()])
                .send()
                .await?;
            // create discord thread
            let thread = discord
                .create_thread_from_message(
                    rct.channel_id,
                    rct.message_id,
                    &format!("Github issue {} - {}", issue.number, title),
                )
                .unwrap() // channel name between 1 and 100 character
                .exec()
                .await?
                .model()
                .await?;

            discord
                .create_message(thread.id())
                .content(&format!(
                    "https://github.com/{user}/{repo}/issues/{}",
                    issue.number
                ))?
                .exec()
                .await?;

            state.add((thread.id(), issue.number as u64)).await?;
        }
        _ => {}
    }

    Ok(())
}
