//! Opens a database file (migrating it) and prints the schema version and row
//! counts. For checking a copy of a real user database before shipping a migration.
//! `cargo run -p magpie-store --example migrate_check -- <path-to-copy.db>`
fn main() {
    let path = std::env::args().nth(1).expect("path to a database copy");
    let store = magpie_store::Store::open(&path).expect("open + migrate");
    let events = store
        .list_events(&magpie_store::EventQuery {
            limit: 10_000,
            ..Default::default()
        })
        .expect("events readable");
    let reminders = store.reminders().expect("reminders readable");
    let items = store
        .list_items(&magpie_store::ItemQuery::default())
        .expect("items readable");
    println!(
        "after: v{} | events {} | reminders {} | items {}",
        store.schema_version().unwrap(),
        events.len(),
        reminders.len(),
        items.len()
    );
}
