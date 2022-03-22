use std::sync::Arc;

use actix_web::{post, web, App, HttpResponse, HttpServer, Responder};
use anyhow::Result;
use futures::{
    channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender},
    SinkExt,
};
use serde::{Deserialize, Serialize};
use twilight_model::id::{marker::ChannelMarker, Id};

use crate::state::State;
use actix_files as fs;
use octocrab::{models::{
    issues::{Comment, Issue},
    Repository,
}, Octocrab};

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
    body: IssueCommentWebhook,
    octo: &Octocrab,
) -> Result<HttpResponse> {
    if body.action != "created" {
        tracing::info!(webhook = ?body, "No creation event, Ignoring for now");
    }

    let repo = body.repository.full_name.clone().ok_or(anyhow::anyhow!(
        "Github API Error, missing `full_name` field in repository"
    ))?;

    let me = octo.current().user().await?;

    if body.comment.user.id == me.id {
        tracing::info!("Ignoring self comment");
        return Ok(HttpResponse::Ok().finish());
    }

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
    body: web::Json<IssueCommentWebhook>,
    octo: web::Data<Octocrab>
) -> impl Responder {
    let mut tx = tx.into_inner();
    let mut tx = Arc::make_mut(&mut tx);
    let body = body.into_inner();
    let state = state.into_inner();
    let octo = octo.into_inner();

    match webhook_impl(&mut tx, &state, body, &octo).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(e = %e, "Error processing webhook");
            HttpResponse::BadRequest().finish()
        }
    }
}

async fn frontend_index() -> std::io::Result<fs::NamedFile> {
    tracing::info!("Redirecting to frontend SPA");
    fs::NamedFile::open("./frontend/dist/index.html")
}

pub async fn run(state: Arc<State>, octo: Octocrab) -> Result<UnboundedReceiver<Msg>> {
    let (tx, rx) = unbounded();
    tokio::spawn(
        HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(tx.clone()))
                .app_data(web::Data::new(state.clone()))
                .app_data(web::Data::new(octo.clone()))
                .service(webhook)
                .service(fs::Files::new("/", "./frontend/dist/").index_file("index.html"))
                .default_service(web::get().to(frontend_index))
        })
        .bind("0.0.0.0:8080")?
        .run(),
    );

    Ok(rx)
}
