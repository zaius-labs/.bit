// bit-lang-store — content-addressable store for .bit documents

pub mod anomaly;
pub mod auto_index;
pub mod autocomplete;
pub mod btree;
pub mod classify;
pub mod collapse;
pub mod compression;
pub mod diff;
pub mod drift;
pub mod embeddings;
pub mod evolution;
pub mod expand;
pub mod infer;
pub mod linking;
pub mod mutate_engine;
pub mod nl_query;
pub mod page;
pub mod pager;
pub mod patterns;
pub mod pipeline;
pub mod query_engine;
pub mod scoring;
pub mod search;
pub mod store;
pub mod table;
pub mod vector_search;

// Re-exports for convenience
pub use anomaly::{AnomalousField, AnomalyDetector, AnomalyResult};
pub use auto_index::{IndexAction, IndexAdvisor, IndexRecommendation};
pub use autocomplete::{AutocompleteIndex, Suggestion};
pub use classify::{Classification, NaiveBayesClassifier};
pub use collapse::collapse;
pub use compression::{compress_entities, CompressionOptions, CompressionResult};
pub use diff::{status, DiffResult};
pub use drift::{DriftAlert, DriftBaseline, DriftType};
pub use embeddings::{cosine_similarity, simple_embed};
pub use evolution::{propose_migration, render_migration, MigrationProposal, SchemaChange};
pub use expand::expand;
pub use infer::{
    infer_schema, render_inferred_schema, InferredField, InferredSchema, InferredType,
};
pub use linking::{EntityLinker, LinkMethod, ResolvedLink};
pub use mutate_engine::{store_delete, store_insert, store_update, store_upsert};
pub use nl_query::{parse_nl_query, NlQueryResult, SchemaContext};
pub use patterns::{DetectedPattern, PatternConfig, PatternDetector};
pub use pipeline::{IntelligentStore, PipelineEvent};
pub use query_engine::{execute_query, parse_query, QueryTarget, StoreQuery};
pub use scoring::{rank_entities, score_entity, ScoringConfig};
pub use search::SearchIndex;
pub use store::{BitStore, ContextWindow, ContextWindowOptions, StoreError, StoreInfo};
pub use vector_search::VectorIndex;
