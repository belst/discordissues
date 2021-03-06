use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use clap::Parser;
use config::Config;
use futures::StreamExt;
use state::State;
use twilight_cache_inmemory::InMemoryCache;
use twilight_gateway::Event;
use twilight_http::{request::channel::reaction::RequestReactionType, Client};
use twilight_model::{
    channel::ReactionType,
    id::{marker::ChannelMarker, Id},
};
use webserver::IssueCommentWebhook;

mod config;
mod discord;
mod state;
mod webserver;

#[derive(Debug, Parser)]
struct Args {
    #[clap(long, short, default_value = "config.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();
    tracing::trace!("Starting program");

    let args = Args::parse();

    let config = Arc::new(Config::from_file(&args.config)?);

    let state = Arc::new(state::State::new(config.database_url()).await?);

    let github =
        octocrab::OctocrabBuilder::new().app(config.github_app_id(), config.github_private_key()?);
    octocrab::initialise(github)?;
    let app = octocrab::instance().current().app().await?;
    tracing::info!(app = ?app.slug, "Github Initialized");

    let (mut events, cache) = discord::run(config.discord_token().into()).await?;

    let discord = Arc::new(Client::new(config.discord_token().into()));

    let mut github_events = webserver::run(state.clone()).await?;

    loop {
        let state = state.clone();
        let discord = discord.clone();
        let config = config.clone();
        let cache = cache.clone();

        tokio::select! {
            Some(event) = events.next() => {
                tokio::spawn(async move {
                    if let Err(e) = handle_discord_event(event, state, discord, cache, config).await {
                        tracing::error!(error = %e, "Error Handling Discord event");
                    }
                });
            },
            Some((thread_id, issue_comment)) = github_events.next() => {
                tokio::spawn(async move {
                    if let Err(e) = handle_github_event(discord, thread_id, issue_comment).await {
                        tracing::error!(error = %e, "Error Handling Github event");
                    }
                });
            }
        }
    }
}

#[tracing::instrument]
async fn handle_github_event(
    discord: Arc<Client>,
    thread_id: Id<ChannelMarker>,
    comment: IssueCommentWebhook,
) -> Result<()> {
    let msg = format!(
        "New comment on Github from {}\n\n{}",
        comment.comment.user.login,
        comment.comment.body.unwrap_or_default()
    );

    discord
        .create_message(thread_id)
        .content(&msg)?
        .exec()
        .await?;

    Ok(())
}

#[tracing::instrument]
async fn handle_discord_event(
    event: Event,
    state: Arc<State>,
    discord: Arc<Client>,
    cache: Arc<InMemoryCache>,
    config: Arc<Config>,
) -> Result<()> {
    match event {
        Event::MessageCreate(msg) if msg.thread.is_none() => {
            tracing::trace!(msg = msg.id.get(), "Message create event");
            if msg.author.bot {
                tracing::info!(msg = msg.id.get(), author = %msg.author.name, "Ignoring bot messages");
                return Ok(());
            }
            if let Some((issue_nr, repo)) = state.get_issue(msg.channel_id).await? {
                let (user, repo) = repo
                    .split_once('/')
                    .map(|(l, r)| (l.to_owned(), r.to_owned()))
                    .ok_or(anyhow::anyhow!("No repository found or invalid repository string in config"))?;

                let commentstr = format!(
                    "New comment from {}\n\n{}\n\n[Link](https://discord.com/channels/{}/{}/{})",
                    msg.author.name,
                    msg.content,
                    msg.guild_id.unwrap(), // does not work in private messages
                    msg.channel_id,
                    msg.id
                );

                let installation = octocrab::instance()
                    .apps()
                    .get_repository_installation(&user, &repo)
                    .await?;

                let github = octocrab::instance().installation(installation.id);

                github
                    .issues(&user, &repo)
                    .create_comment(issue_nr, commentstr)
                    .await?;
            }
        }
        Event::ReactionAdd(rct)
            if rct.emoji
                == ReactionType::Unicode {
                    name: "????".into()
                } =>
        {
            let msg = match cache.message(rct.message_id) {
                Some(msg) => {
                    tracing::info!(msg = ?msg, "Using Cached Message");
                    msg.value().clone()
                }
                None => {
                    tracing::info!("Message not in cache, fetching from api");
                    let msg = discord
                        .message(rct.channel_id, rct.message_id)
                        .exec()
                        .await?
                        .model()
                        .await?;

                    msg.into()
                }
            };

            if let Some(thread_id) = msg.thread_id() {
                if let Some((issue_id, repo)) = state.get_issue(thread_id).await? {
                    tracing::info!(
                        issue_id,
                        %repo,
                        thread_id = thread_id.get(),
                        channel_id = rct.channel_id.get(),
                        message_id = rct.message_id.get(),
                        "Issue already created"
                    );
                    return Ok(());
                }
                // TODO: create issue with complete thread history
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

            let member = rct
                .member
                .as_ref()
                .map(Result::Ok)
                .unwrap_or_else(|| Err(anyhow::anyhow!("Only works in Guilds")))?;
            if !member
                .roles
                .iter()
                .any(|r| config.check_permission(&format!("{user}/{repo}"), r.get()))
            {
                tracing::info!(user = %member.user.name, channel = ?rct.channel_id, "Invalid Permissions to create issue");
                let emoji = match &rct.emoji {
                    ReactionType::Custom { id, name, .. } => RequestReactionType::Custom {
                        id: *id,
                        name: name.as_deref(),
                    },
                    ReactionType::Unicode { ref name } => RequestReactionType::Unicode { name },
                };
                discord
                    .delete_reaction(rct.channel_id, rct.message_id, &emoji, rct.user_id)
                    .exec()
                    .await?;
                return Ok(());
            }

            let title = &msg.content()[..30.min(msg.content().len())];

            let installation = octocrab::instance()
                .apps()
                .get_repository_installation(&user, &repo)
                .await?;

            let github = octocrab::instance().installation(installation.id);

            let issue = github
                .issues(&user, &repo)
                .create(title)
                .body(msg.content())
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
                .create_message(thread.id)
                .content(&format!(
                    "https://github.com/{user}/{repo}/issues/{}",
                    issue.number
                ))?
                .exec()
                .await?;

            state
                .add((thread.id, issue.number as u64, format!("{user}/{repo}")))
                .await?;
        }
        _ => {}
    }

    Ok(())
}
