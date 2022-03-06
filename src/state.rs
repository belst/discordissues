use anyhow::Result;
use twilight_model::id::{marker::ChannelMarker, Id};

use sqlx::sqlite::SqlitePool;

type ThreadId = Id<ChannelMarker>;
type IssueNr = u64;
type Repo = String;
type Mapping = (ThreadId, IssueNr, Repo);

#[derive(Debug)]
pub struct State {
    pool: SqlitePool,
}

impl State {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = SqlitePool::connect(database_url).await?;
        tracing::info!("Running Migrations");
        sqlx::migrate!().run(&pool).await?;

        Ok(Self { pool })
    }

    pub async fn add(&self, mapping: Mapping) -> Result<()> {
        let (thread_id, issue_nr, repo) = (mapping.0.get() as i64, mapping.1 as i64, mapping.2);
        sqlx::query!(
            "insert into mapping (thread_id, repo, issue_nr) values ($1, $2, $3)",
            thread_id,
            repo,
            issue_nr,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
    pub async fn get_issue(&self, thrd: ThreadId) -> Result<Option<(IssueNr, Repo)>> {
        let thrd = thrd.get() as i64;
        let issue = sqlx::query!(
            "select issue_nr, repo from mapping where thread_id = $1",
            thrd
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(issue.map(|n| (n.issue_nr as u64, n.repo)))
    }

    pub async fn get_thread(&self, issue_nr: IssueNr, repo: &str) -> Result<Option<ThreadId>> {
        let issue_nr = issue_nr as i64;
        let msg = sqlx::query!(
            "select thread_id from mapping where issue_nr = $1 and repo = $2",
            issue_nr,
            repo
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(msg.map(|s| Id::new(s.thread_id as u64)))
    }
}
