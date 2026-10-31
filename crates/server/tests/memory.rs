//! The in-memory store behaves like the SQL stores.

use opensharing_server::store::memory::InMemoryStore;

#[path = "store/conformance.rs"]
mod conformance;

#[tokio::test]
async fn memory_store_round_trips() {
    conformance::round_trips(&InMemoryStore::new()).await;
}
