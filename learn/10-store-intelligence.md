# Store Intelligence

bit-lang's store isn't just a database — it understands your data.

Every feature in this lesson ships with the base `bit-lang-store` crate. No external models, no Python, no network calls. The optional features (vector search, anomaly detection, auto-classification) are behind feature flags.

---

## Schema Inference

Don't write schemas by hand. Let the store figure it out:

```rust
use bit_store::BitStore;

let mut store = BitStore::open("project.bitstore")?;
let schema = store.infer_entity_schema("User")?;
println!("{}", bit_store::render_inferred_schema(&schema));
// Output:
// define:@User
//     name: ""!
//     email: ""!
//     role: :admin/:editor/:viewer!
//     active: false?
//     login_count: 0#
```

The inference engine examines every record of the given entity type. It determines:

- **Field types** from observed values (string, int, float, bool, timestamp)
- **Required vs optional** — fields present in >95% of records are marked required (`!`)
- **Enum variants** — string fields with a small set of distinct values become enums (`:a/:b/:c`)

CLI:

```sh
bit infer store.bitstore @User
```

---

## Predictive Autocomplete

The store learns from your data:

```rust
use bit_store::AutocompleteIndex;

let mut index = AutocompleteIndex::new();
// After observing many User inserts...
let suggestions = index.suggest("User", "role", 3);
// [Suggestion { value: "editor", confidence: 0.72 },
//  Suggestion { value: "admin", confidence: 0.21 },
//  Suggestion { value: "viewer", confidence: 0.07 }]
```

The autocomplete index tracks value frequencies per entity type and field. Confidence is the observed probability of each value. This works for any field — string, enum, integer ranges.

CLI:

```sh
bit suggest store.bitstore @User role
```

---

## Drift Detection

Know when your data changes character:

```rust
use bit_store::DriftBaseline;

let baseline = DriftBaseline::build("User", &old_records);
let alerts = baseline.detect("User", &new_records);
// [DriftAlert { type: DistributionShift, field: "role",
//   description: "Distribution shift on 'role' (PSI=0.34)" }]
```

Drift detection compares the distribution of field values between a baseline snapshot and current data. It uses Population Stability Index (PSI) for categorical fields and Kolmogorov-Smirnov tests for numeric fields. Alerts fire when the shift exceeds configurable thresholds.

CLI:

```sh
bit drift store.bitstore
```

---

## Natural Language Queries

Skip the query syntax:

```rust
use bit_store::{parse_nl_query, SchemaContext};

let mut ctx = SchemaContext::default();
ctx.entities.insert("User".into(), vec!["name".into(), "role".into()]);
ctx.value_aliases.insert("active".into(), ("active".into(), "true".into()));

let result = parse_nl_query("show me active users sorted by name", &ctx);
// interpretation: "@User where active=true sort:name"
// confidence: 0.95
```

The NL parser uses a keyword-matching approach with schema awareness. It resolves entity names, field names, and value aliases from the `SchemaContext`. No LLM required — just pattern matching against your schema.

CLI:

```sh
bit query store.bitstore "active admins"
```

---

## Entity Linking

Resolve fuzzy references:

```rust
use bit_store::EntityLinker;

let mut linker = EntityLinker::new();
linker.register_entity("User", "alice");
linker.register_entity("Team", "engineering");
linker.build_aliases();

linker.resolve("alce");  // → @User:alice (fuzzy, confidence: 0.85)
linker.resolve("alice's team");  // → @User:alice (pattern match)
```

Entity linking runs automatically inside queries. When a query references an unqualified name, the linker tries:

1. **Exact match** — `alice` → `@User:alice`
2. **Fuzzy match** — `alce` → `@User:alice` (Levenshtein distance)
3. **Pattern match** — `alice's team` → extracts `alice`, resolves to `@User:alice`

---

## Schema Evolution

Automatic migration proposals:

```rust
use bit_store::evolution::propose_migration;

let declared = HashMap::from([
    ("name".into(), "string".into()),
    ("email".into(), "string".into()),
]);
let proposal = propose_migration("User", &declared, &current_records);
// @User migration proposal (confidence: 94%):
//   + add field 'phone': string (seen in 94% of records)
//   + add field 'avatar_url': string (seen in 87% of records)
```

Schema evolution compares the declared schema against actual data. When fields appear consistently in records but aren't in the schema, it proposes additions. When declared fields are rarely populated, it suggests making them optional.

CLI:

```sh
bit evolve store.bitstore @User
```

---

## BM25 Search

Full-text keyword search over entity fields:

```rust
let results = store.search("auth error", 10)?;
// Returns entities ranked by BM25 relevance score
```

BM25 search tokenizes entity field values and builds an inverted index. Queries are ranked by term frequency and inverse document frequency. No external search engine needed.

CLI:

```sh
bit search store.bitstore "auth error"
```

---

## Pattern Detection

Spot trends in write patterns:

```rust
use bit_store::PatternDetector;

let mut detector = PatternDetector::with_defaults();
// After observing writes...
let patterns = detector.observe("Error", "e5", &error_record);
// [DetectedPattern { type: "frequency", description: "@Error makes up 80% of recent writes" }]
```

The pattern detector tracks:

- **Frequency spikes** — one entity type dominating recent writes
- **Duplicate detection** — records with near-identical field values
- **Value clustering** — fields converging toward a small set of values

CLI:

```sh
bit patterns store.bitstore
```

---

## Self-Organizing Indexes

No configuration needed. The store tracks which fields appear in `where` clauses and automatically creates B-tree indexes on frequently-filtered fields. Cold indexes are dropped to save space.

This is fully automatic — no CLI command or API call required.

---

## Composite Scoring

The `context_window` query uses composite scoring to rank results:

- **Recency** — newer records score higher (configurable decay)
- **Importance** — entities with more relations or references rank higher
- **Relevance** — BM25 score when a text query is provided

Scoring is built into the query engine and used automatically by `context_window`.

---

## Template Compression

Collapse similar entities into summaries:

```rust
let compressed = store.compress_templates("Error", 0.8)?;
// Groups similar @Error entities and produces summary templates
// e.g., "47 @Error entities matching pattern: auth timeout on service X"
```

Template compression clusters entities by field similarity and produces human-readable summaries. Useful for reducing context size when many entities share the same structure.

---

## Anomaly Detection (feature flag: `ml`)

Spot unusual records:

```rust
use bit_store::AnomalyDetector;

let mut detector = AnomalyDetector::new();
detector.train(&normal_records);
let result = detector.score("outlier-1", &suspicious_record);
// anomaly_score: 0.87
// anomalous_fields: [{ field: "login_count", z_score: 4.2 }]
```

Requires `cargo add bit-lang-store --features ml`. Uses z-score analysis for numeric fields and isolation forest for multi-dimensional outlier detection. Adds ~1MB to binary size.

---

## Vector Search (feature flag: `embeddings`)

Find semantically similar entities:

```rust
let index = store.build_vector_index()?;
let results = index.search("authentication error", 5);
// [("@Error:auth-fail-1", 0.92), ("@Error:token-expired", 0.85), ...]
```

Requires `cargo add bit-lang-store --features embeddings`. Uses MiniLM embeddings for semantic similarity. Adds ~23MB to binary size (the embedded model weights).

---

## Auto-Classification (feature flag: `ml`)

Automatically tag entities on insert:

```rust
let classifier = store.build_classifier("Issue", "priority")?;
let predicted = classifier.predict(&new_issue);
// "high" (confidence: 0.78)
```

Requires `cargo add bit-lang-store --features ml`. Uses Naive Bayes trained on existing labeled data. Adds ~1MB to binary size.

---

## Feature Flags Summary

```sh
# Base: all zero-dep features included
cargo add bit-lang-store

# With ML classification + anomaly detection
cargo add bit-lang-store --features ml

# With semantic embeddings
cargo add bit-lang-store --features embeddings

# Everything
cargo add bit-lang-store --features full
```

Each feature works independently. They also compose — the `context_window` query uses scoring, TTL expiry, and priority ranking together. The NL query parser uses entity linking. Drift detection uses schema inference as its baseline.
