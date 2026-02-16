pub mod bulk_runs_service;
pub mod putaway_service;
#[cfg(feature = "intelligence")]
pub mod ingredient_intelligence_service;

// Re-exports for putaway service and types (match public API used by handlers)
pub use putaway_service::PutawayService;
