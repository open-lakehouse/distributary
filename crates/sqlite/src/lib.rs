//! A SQLite [`SharingStore`](opensharing_provider::store::SharingStore) (sqlx), with embedded
//! migrations.
//!
//! For single-node deployments, demos and tests that want state to survive a
//! restart without running Postgres. As with Postgres, migrations run only
//! through [`migrate`] (the `migrate` subcommand), and
//! [`ensure_schema_current`] makes `serve` refuse to start on a schema that
//! does not match this binary.
//!
//! [`connect`] creates the database file if it is missing, enables foreign
//! keys (the schema cascades deletes through them) and uses WAL journaling. An
//! in-memory URL (`sqlite::memory:`) gives each pooled connection its own
//! database, so use a file.

use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;
use sqlx::error::ErrorKind;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteRow,
};
use sqlx::{Row, migrate::Migrator};

use crate::sql::{APPLIED_MIGRATIONS, AppliedMigration, check_applied};
use opensharing_provider::store::{
    CredentialStore, ExternalLocationRecord, RecipientRecord, RecipientStore, SealedCredential,
    ShareObject, ShareReader, ShareRecord, ShareStore, TokenReader, TokenRecord, now_millis,
};
use opensharing_provider::{Error, Result};

mod sql;

/// The embedded migrations.
pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// Connect a pool to `url` (`sqlite://path/to/sharing.db`), creating the
/// database file if needed.
pub async fn connect(url: &str, max_connections: u32) -> Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str(url)
        .map_err(|e| Error::invalid_argument(format!("invalid sqlite url: {e}")))?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5));
    SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect_with(options)
        .await
        .map_err(|e| Error::generic(format!("connecting to sqlite: {e}")))
}

/// Apply all pending migrations.
pub async fn migrate(pool: &SqlitePool) -> Result<()> {
    MIGRATOR
        .run(pool)
        .await
        .map_err(|e| Error::generic(format!("running migrations: {e}")))
}

/// Fail unless the database schema is exactly what this binary expects.
pub async fn ensure_schema_current(pool: &SqlitePool) -> Result<()> {
    let applied: Vec<AppliedMigration> =
        match sqlx::query_as(APPLIED_MIGRATIONS).fetch_all(pool).await {
            Ok(rows) => rows,
            // No migrations table yet: nothing is applied.
            Err(sqlx::Error::Database(e)) if e.message().contains("no such table") => Vec::new(),
            Err(e) => return Err(db(e)),
        };
    check_applied(&MIGRATOR, &applied)
}

/// A SQLite [`SharingStore`](opensharing_provider::store::SharingStore).
#[derive(Clone)]
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

/// Map a database error, turning unique violations into "already exists".
fn db(e: sqlx::Error) -> Error {
    match &e {
        sqlx::Error::RowNotFound => Error::NotFound,
        sqlx::Error::Database(d) if d.kind() == ErrorKind::UniqueViolation => {
            Error::invalid_argument("the resource already exists")
        }
        sqlx::Error::Database(d) if d.kind() == ErrorKind::ForeignKeyViolation => {
            Error::invalid_argument(format!(
                "a referenced resource does not exist: {}",
                d.message()
            ))
        }
        _ => Error::generic(format!("database error: {e}")),
    }
}

fn affected(rows: u64) -> Result<()> {
    if rows == 0 {
        Err(Error::NotFound)
    } else {
        Ok(())
    }
}

const SHARE_COLUMNS: &str = "id, name, comment, objects, created_at, updated_at";

fn share(row: SqliteRow) -> Result<ShareRecord> {
    let objects: String = row.try_get("objects").map_err(db)?;
    Ok(ShareRecord {
        id: row.try_get("id").map_err(db)?,
        name: row.try_get("name").map_err(db)?,
        comment: row.try_get("comment").map_err(db)?,
        objects: serde_json::from_str(&objects)?,
        created_at: row.try_get("created_at").map_err(db)?,
        updated_at: row.try_get("updated_at").map_err(db)?,
    })
}

fn recipient(row: &SqliteRow) -> Result<RecipientRecord> {
    Ok(RecipientRecord {
        id: row.try_get("id").map_err(db)?,
        name: row.try_get("name").map_err(db)?,
        comment: row.try_get("comment").map_err(db)?,
        created_at: row.try_get("created_at").map_err(db)?,
    })
}

const TOKEN_COLUMNS: &str = "t.id, t.recipient_id, t.token_hash, t.sealed_token, t.activation_code, t.expires_at, t.created_at";

fn token(row: &SqliteRow) -> Result<TokenRecord> {
    Ok(TokenRecord {
        id: row.try_get("id").map_err(db)?,
        recipient_id: row.try_get("recipient_id").map_err(db)?,
        token_hash: row.try_get("token_hash").map_err(db)?,
        sealed_token: row.try_get("sealed_token").map_err(db)?,
        activation_code: row.try_get("activation_code").map_err(db)?,
        expires_at: row.try_get("expires_at").map_err(db)?,
        created_at: row.try_get("created_at").map_err(db)?,
    })
}

#[async_trait]
impl ShareReader for SqliteStore {
    async fn get_share(&self, name: &str) -> Result<ShareRecord> {
        let row = sqlx::query(&format!(
            "SELECT {SHARE_COLUMNS} FROM shares WHERE name = ?"
        ))
        .bind(name)
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        share(row)
    }

    async fn shares_of(&self, recipient_id: &str) -> Result<Vec<ShareRecord>> {
        sqlx::query(&format!(
            "SELECT {} FROM shares s JOIN grants g ON g.share_id = s.id \
             WHERE g.recipient_id = ? ORDER BY s.name",
            SHARE_COLUMNS
                .split(", ")
                .map(|c| format!("s.{c}"))
                .collect::<Vec<_>>()
                .join(", ")
        ))
        .bind(recipient_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?
        .into_iter()
        .map(share)
        .collect()
    }

    async fn is_granted(&self, share: &str, recipient_id: &str) -> Result<bool> {
        let row = sqlx::query(
            "SELECT EXISTS (SELECT 1 FROM grants g JOIN shares s ON s.id = g.share_id \
             WHERE s.name = ? AND g.recipient_id = ?) AS granted",
        )
        .bind(share)
        .bind(recipient_id)
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        row.try_get("granted").map_err(db)
    }
}

#[async_trait]
impl ShareStore for SqliteStore {
    async fn create_share(&self, name: &str, comment: Option<String>) -> Result<ShareRecord> {
        let now = now_millis();
        let row = sqlx::query(&format!(
            "INSERT INTO shares (id, name, comment, objects, created_at, updated_at) \
             VALUES (?1, ?2, ?3, '[]', ?4, ?4) RETURNING {SHARE_COLUMNS}"
        ))
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(name)
        .bind(comment)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        share(row)
    }

    async fn list_shares(&self) -> Result<Vec<ShareRecord>> {
        sqlx::query(&format!("SELECT {SHARE_COLUMNS} FROM shares ORDER BY name"))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .into_iter()
            .map(share)
            .collect()
    }

    async fn update_share(
        &self,
        name: &str,
        objects: Vec<ShareObject>,
        comment: Option<String>,
    ) -> Result<ShareRecord> {
        let row = sqlx::query(&format!(
            "UPDATE shares SET objects = ?2, comment = ?3, updated_at = ?4 \
             WHERE name = ?1 RETURNING {SHARE_COLUMNS}"
        ))
        .bind(name)
        .bind(serde_json::to_string(&objects)?)
        .bind(comment)
        .bind(now_millis())
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        share(row)
    }

    async fn delete_share(&self, name: &str) -> Result<()> {
        let done = sqlx::query("DELETE FROM shares WHERE name = ?")
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        affected(done.rows_affected())
    }

    async fn grant(&self, share: &str, recipient: &str) -> Result<()> {
        let done = sqlx::query(
            "INSERT OR IGNORE INTO grants (share_id, recipient_id) \
             SELECT s.id, r.id FROM shares s, recipients r \
             WHERE s.name = ? AND r.name = ?",
        )
        .bind(share)
        .bind(recipient)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        if done.rows_affected() == 0 {
            // Either already granted, or the share / recipient does not exist.
            self.get_share(share).await?;
            self.get_recipient(recipient).await?;
        }
        Ok(())
    }

    async fn revoke(&self, share: &str, recipient: &str) -> Result<()> {
        sqlx::query(
            "DELETE FROM grants \
             WHERE share_id = (SELECT id FROM shares WHERE name = ?) \
             AND recipient_id = (SELECT id FROM recipients WHERE name = ?)",
        )
        .bind(share)
        .bind(recipient)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn recipients_of(&self, share: &str) -> Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT r.name FROM grants g \
             JOIN shares s ON s.id = g.share_id \
             JOIN recipients r ON r.id = g.recipient_id \
             WHERE s.name = ? ORDER BY r.name",
        )
        .bind(share)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(|row| row.try_get("name").map_err(db))
            .collect()
    }
}

#[async_trait]
impl TokenReader for SqliteStore {
    async fn token_by_hash(&self, token_hash: &[u8]) -> Result<(TokenRecord, RecipientRecord)> {
        let row = sqlx::query(&format!(
            "SELECT {TOKEN_COLUMNS}, r.name, r.comment, r.created_at AS recipient_created_at \
             FROM recipient_tokens t JOIN recipients r ON r.id = t.recipient_id \
             WHERE t.token_hash = ?"
        ))
        .bind(token_hash)
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        let t = token(&row)?;
        let r = RecipientRecord {
            id: t.recipient_id.clone(),
            name: row.try_get("name").map_err(db)?,
            comment: row.try_get("comment").map_err(db)?,
            created_at: row.try_get("recipient_created_at").map_err(db)?,
        };
        Ok((t, r))
    }
}

#[async_trait]
impl RecipientStore for SqliteStore {
    async fn create_recipient(
        &self,
        name: &str,
        comment: Option<String>,
    ) -> Result<RecipientRecord> {
        let row = sqlx::query(
            "INSERT INTO recipients (id, name, comment, created_at) \
             VALUES (?, ?, ?, ?) RETURNING id, name, comment, created_at",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(name)
        .bind(comment)
        .bind(now_millis())
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        recipient(&row)
    }

    async fn get_recipient(&self, name: &str) -> Result<RecipientRecord> {
        let row =
            sqlx::query("SELECT id, name, comment, created_at FROM recipients WHERE name = ?")
                .bind(name)
                .fetch_one(&self.pool)
                .await
                .map_err(db)?;
        recipient(&row)
    }

    async fn list_recipients(&self) -> Result<Vec<RecipientRecord>> {
        sqlx::query("SELECT id, name, comment, created_at FROM recipients ORDER BY name")
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .iter()
            .map(recipient)
            .collect()
    }

    async fn update_recipient(
        &self,
        name: &str,
        comment: Option<String>,
    ) -> Result<RecipientRecord> {
        let row = sqlx::query(
            "UPDATE recipients SET comment = ?2 WHERE name = ?1 \
             RETURNING id, name, comment, created_at",
        )
        .bind(name)
        .bind(comment)
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        recipient(&row)
    }

    async fn delete_recipient(&self, name: &str) -> Result<()> {
        // Tokens and grants cascade.
        let done = sqlx::query("DELETE FROM recipients WHERE name = ?")
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        affected(done.rows_affected())
    }

    async fn insert_token(&self, t: TokenRecord) -> Result<()> {
        sqlx::query(
            "INSERT INTO recipient_tokens \
             (id, recipient_id, token_hash, sealed_token, activation_code, expires_at, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(t.id)
        .bind(t.recipient_id)
        .bind(t.token_hash)
        .bind(t.sealed_token)
        .bind(t.activation_code)
        .bind(t.expires_at)
        .bind(t.created_at)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn token_by_activation_code(&self, code: &str) -> Result<TokenRecord> {
        let row = sqlx::query(&format!(
            "SELECT {TOKEN_COLUMNS} FROM recipient_tokens t WHERE t.activation_code = ?"
        ))
        .bind(code)
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        token(&row)
    }

    async fn mark_activated(&self, token_id: &str) -> Result<()> {
        let done = sqlx::query(
            "UPDATE recipient_tokens SET sealed_token = NULL, activation_code = NULL \
             WHERE id = ?",
        )
        .bind(token_id)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        affected(done.rows_affected())
    }

    async fn tokens_for_recipient(&self, recipient_id: &str) -> Result<Vec<TokenRecord>> {
        sqlx::query(&format!(
            "SELECT {TOKEN_COLUMNS} FROM recipient_tokens t \
             WHERE t.recipient_id = ? ORDER BY t.created_at DESC"
        ))
        .bind(recipient_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?
        .iter()
        .map(token)
        .collect()
    }

    async fn delete_tokens(&self, recipient_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM recipient_tokens WHERE recipient_id = ?")
            .bind(recipient_id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn sealed_tokens(&self) -> Result<Vec<TokenRecord>> {
        sqlx::query(&format!(
            "SELECT {TOKEN_COLUMNS} FROM recipient_tokens t WHERE t.sealed_token IS NOT NULL"
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?
        .iter()
        .map(token)
        .collect()
    }

    async fn update_sealed_token(&self, token_id: &str, sealed: Vec<u8>) -> Result<()> {
        let done = sqlx::query("UPDATE recipient_tokens SET sealed_token = ?2 WHERE id = ?1")
            .bind(token_id)
            .bind(sealed)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        affected(done.rows_affected())
    }
}

#[async_trait]
impl CredentialStore for SqliteStore {
    async fn put_credential(&self, c: SealedCredential) -> Result<()> {
        sqlx::query(
            "INSERT INTO credentials (name, sealed) VALUES (?, ?) \
             ON CONFLICT (name) DO UPDATE SET sealed = excluded.sealed",
        )
        .bind(c.name)
        .bind(c.sealed)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn get_credential(&self, name: &str) -> Result<SealedCredential> {
        let row = sqlx::query("SELECT name, sealed FROM credentials WHERE name = ?")
            .bind(name)
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
        Ok(SealedCredential {
            name: row.try_get("name").map_err(db)?,
            sealed: row.try_get("sealed").map_err(db)?,
        })
    }

    async fn list_credentials(&self) -> Result<Vec<SealedCredential>> {
        sqlx::query("SELECT name, sealed FROM credentials ORDER BY name")
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .iter()
            .map(|row| {
                Ok(SealedCredential {
                    name: row.try_get("name").map_err(db)?,
                    sealed: row.try_get("sealed").map_err(db)?,
                })
            })
            .collect()
    }

    async fn delete_credential(&self, name: &str) -> Result<()> {
        let done = sqlx::query("DELETE FROM credentials WHERE name = ?")
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        affected(done.rows_affected())
    }

    async fn put_external_location(&self, l: ExternalLocationRecord) -> Result<()> {
        sqlx::query(
            "INSERT INTO external_locations (name, url, credential_name) VALUES (?, ?, ?) \
             ON CONFLICT (name) DO UPDATE SET url = excluded.url, credential_name = excluded.credential_name",
        )
        .bind(l.name)
        .bind(l.url)
        .bind(l.credential_name)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn list_external_locations(&self) -> Result<Vec<ExternalLocationRecord>> {
        sqlx::query("SELECT name, url, credential_name FROM external_locations ORDER BY name")
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .iter()
            .map(|row| {
                Ok(ExternalLocationRecord {
                    name: row.try_get("name").map_err(db)?,
                    url: row.try_get("url").map_err(db)?,
                    credential_name: row.try_get("credential_name").map_err(db)?,
                })
            })
            .collect()
    }

    async fn delete_external_location(&self, name: &str) -> Result<()> {
        let done = sqlx::query("DELETE FROM external_locations WHERE name = ?")
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        affected(done.rows_affected())
    }
}
