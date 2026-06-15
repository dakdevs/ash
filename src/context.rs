use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use diesel::{dsl::count_star, prelude::*, sqlite::SqliteConnection};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

use crate::{error::Result, schema::context_events, session::PromptMode, shell::ExecutionResult};

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextEvent {
    pub kind: String,
    pub body: String,
}

impl ContextEvent {
    #[must_use]
    pub fn agent_prompt(prompt: &str) -> Self {
        Self::new("agent.prompt", prompt)
    }

    #[must_use]
    pub fn agent_response(response: &str) -> Self {
        Self::new("agent.response", response)
    }

    #[must_use]
    pub fn command_input(command: &str) -> Self {
        Self::new("command.input", command)
    }

    #[must_use]
    pub fn command_result(result: &ExecutionResult) -> Self {
        Self::new(
            "command.result",
            format!(
                "status={} stdout={} stderr={}",
                result.status, result.stdout, result.stderr
            ),
        )
    }

    #[must_use]
    pub fn mode_changed(mode: PromptMode) -> Self {
        Self::new("mode.changed", mode.prompt())
    }

    fn new(kind: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            body: body.into(),
        }
    }
}

pub trait ContextStore {
    fn record(&mut self, event: ContextEvent) -> Result<()>;
}

#[derive(Debug, Default)]
pub struct InMemoryContextStore {
    events: Vec<ContextEvent>,
}

impl InMemoryContextStore {
    #[must_use]
    pub fn events(&self) -> &[ContextEvent] {
        &self.events
    }
}

impl ContextStore for InMemoryContextStore {
    fn record(&mut self, event: ContextEvent) -> Result<()> {
        self.events.push(event);
        Ok(())
    }
}

pub struct SqliteContextStore {
    path: PathBuf,
    connection: RefCell<SqliteConnection>,
}

impl SqliteContextStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let database_url = path.display().to_string();
        let mut connection = SqliteConnection::establish(&database_url)?;
        connection.run_pending_migrations(MIGRATIONS)?;

        Ok(Self {
            path,
            connection: RefCell::new(connection),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn count(&self) -> Result<usize> {
        let mut connection = self.connection.borrow_mut();
        let count = context_events::table
            .select(count_star())
            .first::<i64>(&mut *connection)?;
        Ok(usize::try_from(count).unwrap_or(usize::MAX))
    }
}

impl ContextStore for SqliteContextStore {
    fn record(&mut self, event: ContextEvent) -> Result<()> {
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| {
                i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
            });

        let record = NewContextEvent {
            created_at_ms: created_at,
            kind: &event.kind,
            body: &event.body,
        };

        let mut connection = self.connection.borrow_mut();
        diesel::insert_into(context_events::table)
            .values(&record)
            .execute(&mut *connection)?;

        Ok(())
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = context_events)]
struct NewContextEvent<'a> {
    created_at_ms: i64,
    kind: &'a str,
    body: &'a str,
}

#[cfg(test)]
mod tests {
    use diesel::{Connection, connection::SimpleConnection, sqlite::SqliteConnection};
    use tempfile::tempdir;

    use super::{ContextEvent, ContextStore, SqliteContextStore};

    #[test]
    fn sqlite_store_records_events() {
        let dir = tempdir().expect("tempdir");
        let db = dir.path().join("ash.db");
        let mut store = SqliteContextStore::open(&db).expect("store");

        store
            .record(ContextEvent::agent_prompt("hello"))
            .expect("record");

        assert_eq!(store.count().expect("count"), 1);
    }

    #[test]
    fn sqlite_store_opens_legacy_database_with_existing_table() {
        let dir = tempdir().expect("tempdir");
        let db = dir.path().join("ash.db");
        let database_url = db.display().to_string();
        let mut connection = SqliteConnection::establish(&database_url).expect("connection");
        connection
            .batch_execute(
                "
                CREATE TABLE context_events (
                    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                    created_at_ms BIGINT NOT NULL,
                    kind TEXT NOT NULL,
                    body TEXT NOT NULL
                );
                ",
            )
            .expect("legacy table");

        let mut store = SqliteContextStore::open(&db).expect("store");
        store
            .record(ContextEvent::agent_prompt("hello"))
            .expect("record");

        assert_eq!(store.count().expect("count"), 1);
    }
}
