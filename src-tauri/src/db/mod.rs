pub mod schema;
pub mod conversation_repo;
pub mod message_repo;
pub mod project_repo;
pub mod memory_repo;
pub mod migration;
pub mod swarm_repo;

use anyhow::Result;
use rusqlite::Connection;
use std::path::PathBuf;
use parking_lot::Mutex;

pub struct DbManager {
    // parking_lot 的 Mutex 无 poisoning：闭包内 panic 不会永久锁死整个 DB 层
    conn: Mutex<Connection>,
}

impl DbManager {
    pub fn new(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn init(&self) -> Result<()> {
        self.with_conn(|conn| conn.execute_batch(schema::SCHEMA_SQL))??;
        // Run V2 memory migration (adds memory_type, importance columns + FTS5)
        let _ = self.with_conn(|conn| migration::migrate_memory_v2(conn));
        // Run V3 memory migration (adds vector embedding table)
        let _ = self.with_conn(|conn| migration::migrate_memory_v3(conn));
        Ok(())
    }

    pub fn with_conn<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&Connection) -> R,
    {
        let guard = self.conn.lock();
        Ok(f(&guard))
    }
}

