//! The in-memory store behaves like the SQL stores.

use opensharing_provider::store::memory::InMemoryStore;
use opensharing_provider::testing::round_trips;

#[tokio::test]
async fn memory_store_round_trips() {
    round_trips(&InMemoryStore::new()).await;
}
