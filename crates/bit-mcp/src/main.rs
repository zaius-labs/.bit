// bit-mcp — MCP server for .bit language store access
//
// JSON-RPC 2.0 over stdio. Implements the Model Context Protocol (MCP) with
// 11 tools for reading, writing, and managing .bit stores.

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use bit_core::{parse_source, SchemaRegistry, validate_doc};
use bit_store::{
    collapse, execute_query, expand, parse_query, status, store_delete, store_insert, store_update,
    BitStore,
};

// ---------------------------------------------------------------------------
// Store discovery
// ---------------------------------------------------------------------------

/// Find a .bitstore file starting from the given directory.
/// Checks: project.bitstore, .bitstore, then any *.bitstore in the dir.
fn discover_store(dir: &Path) -> Option<PathBuf> {
    let candidates = ["project.bitstore", ".bitstore"];
    for name in &candidates {
        let p = dir.join(name);
        if p.exists() {
            return Some(p);
        }
    }
    // Scan for any .bitstore file
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "bitstore") {
                return Some(path);
            }
        }
    }
    None
}

fn open_store() -> Result<(BitStore, PathBuf), String> {
    let cwd = std::env::current_dir().map_err(|e| format!("cannot get cwd: {e}"))?;
    let store_path =
        discover_store(&cwd).ok_or_else(|| "no .bitstore found in current directory".to_string())?;
    let store = BitStore::open(&store_path).map_err(|e| format!("cannot open store: {e}"))?;
    Ok((store, store_path))
}

// ---------------------------------------------------------------------------
// Tool definitions
// ---------------------------------------------------------------------------

fn tool_definitions() -> Value {
    json!([
        {
            "name": "bit_query",
            "description": "Query entities from the .bit store. Supports filtering, sorting, and limiting results.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "entity": {
                        "type": "string",
                        "description": "Entity type to query (e.g. '@Rule', '@Memory', 'tasks', 'flows', 'schemas')"
                    },
                    "filter": {
                        "type": "string",
                        "description": "Filter expression (e.g. 'role=admin', 'priority>3')"
                    },
                    "sort": {
                        "type": "string",
                        "description": "Sort field, append '-' for descending (e.g. 'name', 'priority-')"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return"
                    }
                },
                "required": ["entity"]
            }
        },
        {
            "name": "bit_search",
            "description": "Full-text BM25 search across all entities in the .bit store.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query text"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results (default: 10)"
                    }
                },
                "required": ["query"]
            }
        },
        {
            "name": "bit_rules",
            "description": "List all @Rule entities from the .bit store. Convenience shortcut for bit_query with entity='@Rule'.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "bit_info",
            "description": "Get summary statistics about the .bit store (entity count, task count, etc).",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "bit_insert",
            "description": "Insert a new entity into the .bit store.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "entity": {
                        "type": "string",
                        "description": "Entity type (e.g. '@Memory', '@Rule')"
                    },
                    "id": {
                        "type": "string",
                        "description": "Unique identifier for the entity"
                    },
                    "fields": {
                        "type": "object",
                        "description": "Key-value fields for the entity"
                    }
                },
                "required": ["entity", "id", "fields"]
            }
        },
        {
            "name": "bit_update",
            "description": "Update fields on an existing entity in the .bit store (merge semantics).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "entity": {
                        "type": "string",
                        "description": "Entity type (e.g. '@Memory', '@Rule')"
                    },
                    "id": {
                        "type": "string",
                        "description": "Entity identifier to update"
                    },
                    "fields": {
                        "type": "object",
                        "description": "Fields to merge into the entity"
                    }
                },
                "required": ["entity", "id", "fields"]
            }
        },
        {
            "name": "bit_delete",
            "description": "Delete an entity from the .bit store.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "entity": {
                        "type": "string",
                        "description": "Entity type (e.g. '@Memory', '@Rule')"
                    },
                    "id": {
                        "type": "string",
                        "description": "Entity identifier to delete"
                    }
                },
                "required": ["entity", "id"]
            }
        },
        {
            "name": "bit_validate",
            "description": "Validate .bit text without writing it to the store. Returns parse errors or validation results.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "The .bit text to validate"
                    }
                },
                "required": ["content"]
            }
        },
        {
            "name": "bit_collapse",
            "description": "Collapse .bit files from a directory into the .bitstore database.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "dir": {
                        "type": "string",
                        "description": "Source directory containing .bit files (default: current directory)"
                    }
                }
            }
        },
        {
            "name": "bit_expand",
            "description": "Expand the .bitstore back to .bit files on disk.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "dir": {
                        "type": "string",
                        "description": "Target directory to write .bit files (default: current directory)"
                    }
                }
            }
        },
        {
            "name": "bit_drift",
            "description": "Check drift between .bit files on disk and the .bitstore. Shows added, modified, and deleted files.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }
    ])
}

// ---------------------------------------------------------------------------
// Tool handlers
// ---------------------------------------------------------------------------

fn handle_bit_query(args: &Value) -> Result<String, String> {
    let entity = args["entity"]
        .as_str()
        .ok_or("missing required param: entity")?;

    // Build query string
    let mut query_str = entity.to_string();
    if let Some(filter) = args["filter"].as_str() {
        query_str.push_str(&format!(" where {filter}"));
    }
    if let Some(sort) = args["sort"].as_str() {
        query_str.push_str(&format!(" sort:{sort}"));
    }
    if let Some(limit) = args["limit"].as_u64() {
        query_str.push_str(&format!(" limit:{limit}"));
    }

    let parsed = parse_query(&query_str).map_err(|e| format!("query parse error: {e}"))?;
    let (mut store, _) = open_store()?;
    let results = execute_query(&mut store, &parsed).map_err(|e| format!("query error: {e}"))?;
    Ok(serde_json::to_string_pretty(&results).unwrap_or_default())
}

fn handle_bit_search(args: &Value) -> Result<String, String> {
    let query = args["query"]
        .as_str()
        .ok_or("missing required param: query")?;
    let limit = args["limit"].as_u64().unwrap_or(10) as usize;

    let (mut store, _) = open_store()?;
    let index = store
        .build_search_index()
        .map_err(|e| format!("index error: {e}"))?;
    let results = index.search(query);
    let limited: Vec<_> = results.into_iter().take(limit).collect();

    // Fetch full records for results
    let mut output = Vec::new();
    for (key, score) in &limited {
        // key format is "@Entity:id"
        if let Some((entity, id)) = key.strip_prefix('@').and_then(|k| k.split_once(':')) {
            if let Ok(Some(record)) = store.get_entity(entity, id) {
                output.push(json!({
                    "key": key,
                    "score": score,
                    "record": record
                }));
            } else {
                output.push(json!({ "key": key, "score": score }));
            }
        } else {
            output.push(json!({ "key": key, "score": score }));
        }
    }
    Ok(serde_json::to_string_pretty(&output).unwrap_or_default())
}

fn handle_bit_rules() -> Result<String, String> {
    let args = json!({"entity": "@Rule"});
    handle_bit_query(&args)
}

fn handle_bit_info() -> Result<String, String> {
    let (mut store, store_path) = open_store()?;
    let info = store.info().map_err(|e| format!("info error: {e}"))?;
    let result = json!({
        "store_path": store_path.display().to_string(),
        "page_count": info.page_count,
        "entity_count": info.entity_count,
        "task_count": info.task_count,
        "flow_count": info.flow_count,
        "schema_count": info.schema_count,
        "blob_count": info.blob_count,
    });
    Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
}

fn handle_bit_insert(args: &Value) -> Result<String, String> {
    let entity_raw = args["entity"]
        .as_str()
        .ok_or("missing required param: entity")?;
    let id = args["id"]
        .as_str()
        .ok_or("missing required param: id")?;
    let fields = args["fields"]
        .as_object()
        .ok_or("missing required param: fields (must be object)")?;

    // Strip leading @ if present
    let entity = entity_raw.strip_prefix('@').unwrap_or(entity_raw);

    let (mut store, _) = open_store()?;

    // Convert fields to &[(&str, &str)] for store_insert
    let field_pairs: Vec<(String, String)> = fields
        .iter()
        .map(|(k, v)| {
            let val = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            (k.clone(), val)
        })
        .collect();
    let field_refs: Vec<(&str, &str)> = field_pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    store_insert(&mut store, entity, id, &field_refs)
        .map_err(|e| format!("insert error: {e}"))?;
    store.flush().map_err(|e| format!("flush error: {e}"))?;

    Ok(format!("inserted @{entity}:{id}"))
}

fn handle_bit_update(args: &Value) -> Result<String, String> {
    let entity_raw = args["entity"]
        .as_str()
        .ok_or("missing required param: entity")?;
    let id = args["id"]
        .as_str()
        .ok_or("missing required param: id")?;
    let fields = args["fields"]
        .as_object()
        .ok_or("missing required param: fields (must be object)")?;

    let entity = entity_raw.strip_prefix('@').unwrap_or(entity_raw);

    let (mut store, _) = open_store()?;

    let field_pairs: Vec<(String, String)> = fields
        .iter()
        .map(|(k, v)| {
            let val = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            (k.clone(), val)
        })
        .collect();
    let field_refs: Vec<(&str, &str)> = field_pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let updated = store_update(&mut store, entity, id, &field_refs)
        .map_err(|e| format!("update error: {e}"))?;
    store.flush().map_err(|e| format!("flush error: {e}"))?;

    if updated {
        Ok(format!("updated @{entity}:{id}"))
    } else {
        Ok(format!("@{entity}:{id} not found"))
    }
}

fn handle_bit_delete(args: &Value) -> Result<String, String> {
    let entity_raw = args["entity"]
        .as_str()
        .ok_or("missing required param: entity")?;
    let id = args["id"]
        .as_str()
        .ok_or("missing required param: id")?;

    let entity = entity_raw.strip_prefix('@').unwrap_or(entity_raw);

    let (mut store, _) = open_store()?;
    let deleted = store_delete(&mut store, entity, id)
        .map_err(|e| format!("delete error: {e}"))?;
    store.flush().map_err(|e| format!("flush error: {e}"))?;

    if deleted {
        Ok(format!("deleted @{entity}:{id}"))
    } else {
        Ok(format!("@{entity}:{id} not found"))
    }
}

fn handle_bit_validate(args: &Value) -> Result<String, String> {
    let content = args["content"]
        .as_str()
        .ok_or("missing required param: content")?;

    match parse_source(content) {
        Ok(doc) => {
            let registry = SchemaRegistry::new();
            let result = validate_doc(&doc, &registry);
            let node_count = doc.nodes.len();
            Ok(json!({
                "valid": result.errors.is_empty(),
                "node_count": node_count,
                "errors": result.errors.iter().map(|e| format!("{e:?}")).collect::<Vec<_>>(),
                "warnings": result.warnings.iter().map(|w| format!("{w:?}")).collect::<Vec<_>>(),
            })
            .to_string())
        }
        Err(e) => Ok(json!({
            "valid": false,
            "parse_error": format!("{e}"),
        })
        .to_string()),
    }
}

fn handle_bit_collapse(args: &Value) -> Result<String, String> {
    let dir = args["dir"].as_str().unwrap_or(".");
    let cwd = std::env::current_dir().map_err(|e| format!("cwd error: {e}"))?;
    let source_dir = cwd.join(dir);
    let output = source_dir.join("project.bitstore");

    let store = collapse(&source_dir, &output).map_err(|e| format!("collapse error: {e}"))?;
    // Get info before drop
    let mut store = store;
    let info = store.info().map_err(|e| format!("info error: {e}"))?;

    Ok(json!({
        "output": output.display().to_string(),
        "entity_count": info.entity_count,
        "task_count": info.task_count,
        "flow_count": info.flow_count,
        "schema_count": info.schema_count,
        "blob_count": info.blob_count,
    })
    .to_string())
}

fn handle_bit_expand(args: &Value) -> Result<String, String> {
    let dir = args["dir"].as_str().unwrap_or(".");
    let cwd = std::env::current_dir().map_err(|e| format!("cwd error: {e}"))?;
    let target_dir = cwd.join(dir);

    let (_, store_path) = open_store()?;
    let count = expand(&store_path, &target_dir).map_err(|e| format!("expand error: {e}"))?;

    Ok(json!({
        "target": target_dir.display().to_string(),
        "files_written": count,
    })
    .to_string())
}

fn handle_bit_drift() -> Result<String, String> {
    let cwd = std::env::current_dir().map_err(|e| format!("cwd error: {e}"))?;
    let (_, store_path) = open_store()?;

    let diff = status(&store_path, &cwd).map_err(|e| format!("drift error: {e}"))?;

    if diff.added.is_empty() && diff.modified.is_empty() && diff.deleted.is_empty() {
        Ok("no drift detected — files and store are in sync".to_string())
    } else {
        Ok(json!({
            "added": diff.added,
            "modified": diff.modified,
            "deleted": diff.deleted,
        })
        .to_string())
    }
}

// ---------------------------------------------------------------------------
// JSON-RPC dispatch
// ---------------------------------------------------------------------------

fn dispatch_tool(name: &str, args: &Value) -> Value {
    let result = match name {
        "bit_query" => handle_bit_query(args),
        "bit_search" => handle_bit_search(args),
        "bit_rules" => handle_bit_rules(),
        "bit_info" => handle_bit_info(),
        "bit_insert" => handle_bit_insert(args),
        "bit_update" => handle_bit_update(args),
        "bit_delete" => handle_bit_delete(args),
        "bit_validate" => handle_bit_validate(args),
        "bit_collapse" => handle_bit_collapse(args),
        "bit_expand" => handle_bit_expand(args),
        "bit_drift" => handle_bit_drift(),
        _ => Err(format!("unknown tool: {name}")),
    };

    match result {
        Ok(text) => json!({
            "content": [{ "type": "text", "text": text }]
        }),
        Err(e) => json!({
            "content": [{ "type": "text", "text": e }],
            "isError": true
        }),
    }
}

fn handle_request(req: &Value) -> Value {
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let method = req["method"].as_str().unwrap_or("");

    let result = match method {
        "initialize" => json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "bit-store",
                "version": env!("CARGO_PKG_VERSION")
            }
        }),
        "notifications/initialized" => {
            // Client acknowledgment — no response needed
            return Value::Null;
        }
        "tools/list" => json!({
            "tools": tool_definitions()
        }),
        "tools/call" => {
            let params = &req["params"];
            let name = params["name"].as_str().unwrap_or("");
            let empty = json!({});
            let args = params.get("arguments").unwrap_or(&empty);
            dispatch_tool(name, args)
        }
        _ => {
            return json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": format!("method not found: {method}")
                }
            });
        }
    };

    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

// ---------------------------------------------------------------------------
// Main loop
// ---------------------------------------------------------------------------

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let err = json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": -32700,
                        "message": format!("parse error: {e}")
                    }
                });
                let _ = writeln!(out, "{}", err);
                let _ = out.flush();
                continue;
            }
        };

        let response = handle_request(&req);
        // Null means no response needed (notification)
        if response.is_null() {
            continue;
        }
        let _ = writeln!(out, "{}", response);
        let _ = out.flush();
    }
}
