use sqlx::PgPool;
use tracing::info;

/// Outbox dispatcher polls for pending effects and dispatches them to external executors.
/// Architecture doc section 24: Outbox/Inbox pattern.
pub struct OutboxDispatcher { pool: PgPool }

impl OutboxDispatcher {
    pub fn new(pool: PgPool) -> Self { Self { pool } }

    pub async fn run(&self) {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            if let Err(e) = self.process_batch().await {
                tracing::warn!("outbox dispatch error: {e}");
            }
        }
    }

    async fn process_batch(&self) -> Result<(), Box<dyn std::error::Error>> {
        let rows = sqlx::query_as::<_, (uuid::Uuid,)>(
            "SELECT effect_id FROM effect_outbox WHERE status = 'pending' LIMIT 10"
        ).fetch_all(&self.pool).await?;

        for (effect_id,) in rows {
            info!("Dispatching effect: {effect_id}");
            sqlx::query(
                "UPDATE effect_outbox SET status = 'dispatching' WHERE effect_id = $1"
            ).bind(effect_id).execute(&self.pool).await?;
        }
        Ok(())
    }
}
