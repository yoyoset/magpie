use magpie_core::EventId;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// A stored value no longer decodes, e.g. written by a newer version.
    #[error("corrupt {what} in database: {detail}")]
    Corrupt { what: &'static str, detail: String },
    #[error("event {0} not found")]
    EventNotFound(EventId),
    #[error("database schema v{found} is newer than this build supports (v{supported})")]
    SchemaTooNew { found: u32, supported: u32 },
}
