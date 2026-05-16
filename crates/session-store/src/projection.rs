use sqlx::PgPool; use uuid::Uuid;
use openforce_domain::error::{DomainError, DomainResult};

pub struct ProjectionBuilder;

impl ProjectionBuilder {
    pub async fn rebuild(pool: &PgPool, session_id: Uuid) -> DomainResult<()> {
        sqlx::query("SELECT rebuild_session_projection($1)")
            .bind(session_id).execute(pool).await
            .map_err(|e| DomainError::ValidationFailed {
                detail: format!("rebuild session: {e}")
            })?;
        sqlx::query("SELECT rebuild_task_projection($1)")
            .bind(session_id).execute(pool).await
            .map_err(|e| DomainError::ValidationFailed {
                detail: format!("rebuild task: {e}")
            })?;
        Ok(())
    }
}
