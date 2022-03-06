use std::sync::Arc;

use actix_web::{post, web, App, HttpResponse, HttpServer, Responder};
use anyhow::Result;
use futures::{
    channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender},
    SinkExt,
};
use serde::{Deserialize, Serialize};
use twilight_model::id::{Id, marker::ChannelMarker};

use crate::state::State;
use octocrab::models::{Repository, issues::{Issue, Comment}};

type Msg = (Id<ChannelMarker>, IssueCommentWebhook);

#[derive(Debug, Deserialize, Serialize)]
pub struct IssueCommentWebhook {
    pub action: String,
    pub comment: Comment,
    pub issue: Issue,
    pub repository: Repository,
}

async fn webhook_impl(
    tx: &mut UnboundedSender<Msg>,
    state: &State,
    body: IssueCommentWebhook
) -> Result<HttpResponse> {
    if body.action != "created" {
        tracing::info!(webhook = ?body, "No creation event, Ignoring for now");
    }

    let repo = body.repository.full_name.clone()
        .ok_or(anyhow::anyhow!("Github API Error, missing `full_name` field in repository"))?;
    
    let thread_id = match state.get_thread(body.issue.number as u64, &repo).await? {
        Some(thread_id) => thread_id,
        None => {
            tracing::info!(issue_nr = body.issue.number, repo = %repo, "No thread found");
            return Ok(HttpResponse::Ok().finish());
        } 
    };

    tx.send((thread_id, body)).await?;

    Ok(HttpResponse::Ok().finish())
}

#[post("/webhook")]
async fn webhook(
    tx: web::Data<UnboundedSender<Msg>>,
    state: web::Data<Arc<State>>,
    body: web::Json<IssueCommentWebhook>
) -> impl Responder {
    let mut tx = tx.into_inner();
    let mut tx = Arc::make_mut(&mut tx);
    let body = body.into_inner();
    let state = state.into_inner();

    match webhook_impl(&mut tx, &state, body).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(e = %e, "Error processing webhook");
            HttpResponse::BadRequest().finish()
        }
    }

}

pub async fn run(state: Arc<State>) -> Result<UnboundedReceiver<Msg>> {
    let (tx, rx) = unbounded();
    tokio::spawn(
        HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(tx.clone()))
                .app_data(web::Data::new(state.clone()))
                .service(webhook)
        })
        .bind("0.0.0.0:8080")?
        .run(),
    );

    Ok(rx)
}
