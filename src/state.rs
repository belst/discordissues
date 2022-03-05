use anyhow::Result;
use twilight_model::id::{
    marker::{ChannelMarker, MessageMarker},
    Id,
};

use sqlx::sqlite::SqlitePool;

type ThreadId = Id<ChannelMarker>;
type IssueNr = u64;
type Mapping = (ThreadId, IssueNr);

#[derive(Debug)]
pub struct State {
    pool: SqlitePool,
}

impl State {
    pub async fn new() -> Result<Self> {
        let pool =
            SqlitePool::connect(&std::env::var("DATABASE_URL").unwrap_or("sqlite::memory:".into()))
                .await?;
        Ok(Self { pool })
    }

    pub async fn add(&self, mapping: Mapping) -> Result<()> {
        let (thread_id, issue_nr) = (mapping.0.get() as i64, mapping.1 as i64);
        sqlx::query!(
            "insert into mapping (thread_id, issue_nr) values ($1, $2)",
            thread_id,
            issue_nr,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
    pub async fn get_issue(&self, thrd: ThreadId) -> Result<Option<IssueNr>> {
        let thrd = thrd.get() as i64;
        let issue_nr: Option<i64> =
            sqlx::query_scalar!("select issue_nr from mapping where thread_id = $1", thrd,)
                .fetch_optional(&self.pool)
                .await?;

        Ok(issue_nr.map(|n| n as u64))
    }

    pub async fn get_thread(&self, issue_nr: IssueNr) -> Result<Option<ThreadId>> {
        let issue_nr = issue_nr as i64;
        let msg = sqlx::query!(
            "select thread_id from mapping where issue_nr = $2",
            issue_nr,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(msg.map(|s| Id::new(s.thread_id as u64)))
    }
}
