//! GraphQL native support (Feature R).
//!
//! Three pieces of value:
//!
//! 1. **Detection** — `is_graphql_request` recognises a captured
//!    [`HttpFlow`] whose body looks like a GraphQL query/mutation/
//!    subscription request, so the UI can surface "GraphQL" capabilities
//!    next to it.
//! 2. **Introspection** — `introspection_query` returns the canonical
//!    `__schema` query, and `parse_introspection` parses the response
//!    into a structured [`GraphQLSchema`] (types, queries, mutations,
//!    subscriptions).
//! 3. **Attack plan generator** — `build_attack_plan` produces a
//!    deterministic list of [`GraphQLAttackCase`] entries covering the
//!    well-known GraphQL-specific abuse classes:
//!    - introspection enabled in production
//!    - alias overloading (one query, N aliases of the same field)
//!    - batched queries (DoS via array-payload)
//!    - deeply nested introspection (CPU/parser DoS)
//!    - field suggestion leakage (misspelled field exposes neighbours)
//!
//! The plan only describes the requests — actually firing them is the
//! Intruder's responsibility, matching how Feature BB (OpenAPI tests)
//! integrates with the rest of the app.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::NyxResult;
use crate::model::HttpFlow;

/// Heuristic GraphQL detector. Returns `true` when the request is a
/// `POST` with a JSON body that contains a `query` field, or a `GET`
/// whose URL has a `?query=` parameter. Path is not required to be
/// `/graphql` — many APIs use `/api/graphql`, `/v1/gql`, etc.
pub fn is_graphql_request(flow: &HttpFlow) -> bool {
    let method = flow.request.method.to_uppercase();
    if method == "GET" {
        let q = flow.request.path.to_lowercase();
        return q.contains("?query=") || q.contains("&query=");
    }
    if method != "POST" {
        return false;
    }
    if flow.request.body_b64.is_empty() {
        return false;
    }
    let Ok(body_bytes) =
        base64::engine::Engine::decode(&base64::engine::general_purpose::STANDARD, &flow.request.body_b64)
    else {
        return false;
    };
    let Ok(text) = std::str::from_utf8(&body_bytes) else {
        return false;
    };
    if let Ok(json) = serde_json::from_str::<Value>(text) {
        return json.get("query").and_then(|v| v.as_str()).is_some()
            || json.get("operationName").is_some()
            || (json.is_array() && json.as_array().map(|a| !a.is_empty()).unwrap_or(false));
    }
    text.trim_start().starts_with("query ")
        || text.trim_start().starts_with("mutation ")
        || text.trim_start().starts_with("{")
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GraphQLSchema {
    pub query_type: Option<String>,
    pub mutation_type: Option<String>,
    pub subscription_type: Option<String>,
    pub types: Vec<GraphQLType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLType {
    pub name: String,
    pub kind: String,
    pub fields: Vec<String>,
}

/// The canonical introspection query — keep small enough for older
/// servers but covers the structural detail we use downstream.
pub fn introspection_query() -> &'static str {
    r#"query IntrospectionQuery {
  __schema {
    queryType { name }
    mutationType { name }
    subscriptionType { name }
    types {
      name
      kind
      fields { name }
    }
  }
}"#
}

/// Parse a `data.__schema` block from an introspection response.
pub fn parse_introspection(response_json: &Value) -> NyxResult<GraphQLSchema> {
    let schema_node = response_json
        .pointer("/data/__schema")
        .ok_or_else(|| crate::error::NyxError::BadRequest("missing data.__schema".into()))?;
    let mut schema = GraphQLSchema::default();
    schema.query_type = schema_node
        .pointer("/queryType/name")
        .and_then(|v| v.as_str())
        .map(String::from);
    schema.mutation_type = schema_node
        .pointer("/mutationType/name")
        .and_then(|v| v.as_str())
        .map(String::from);
    schema.subscription_type = schema_node
        .pointer("/subscriptionType/name")
        .and_then(|v| v.as_str())
        .map(String::from);
    if let Some(types) = schema_node.get("types").and_then(|v| v.as_array()) {
        for t in types {
            let Some(name) = t.get("name").and_then(|v| v.as_str()) else {
                continue;
            };
            let kind = t
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("UNKNOWN")
                .to_string();
            let fields = t
                .get("fields")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|f| f.get("name").and_then(|n| n.as_str()).map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            schema.types.push(GraphQLType {
                name: name.to_string(),
                kind,
                fields,
            });
        }
    }
    Ok(schema)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum GraphQLAttackKind {
    IntrospectionEnabled,
    AliasOverload,
    BatchedQueries,
    DeepNesting,
    FieldSuggestionLeak,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLAttackCase {
    pub kind: GraphQLAttackKind,
    pub name: String,
    pub method: String,
    pub body: String,
    pub repeat: u32,
    pub notes: String,
}

/// Produce an attack plan against an endpoint, given an optional
/// introspection result. The schema is only needed for alias overload
/// (we pick one query-type field name); without it we fall back to a
/// generic `__typename` alias chain that works against any server.
pub fn build_attack_plan(schema: Option<&GraphQLSchema>) -> Vec<GraphQLAttackCase> {
    let mut plan = Vec::new();

    plan.push(GraphQLAttackCase {
        kind: GraphQLAttackKind::IntrospectionEnabled,
        name: "introspection enabled".into(),
        method: "POST".into(),
        body: serde_json::json!({ "query": introspection_query() }).to_string(),
        repeat: 1,
        notes:
            "If response contains a non-empty `data.__schema`, introspection is enabled. Disable it in production unless you have a strong reason."
                .into(),
    });

    let query_field = schema
        .and_then(|s| {
            let qt = s.query_type.as_deref()?;
            s.types.iter().find(|t| t.name == qt)
        })
        .and_then(|t| t.fields.first().cloned())
        .unwrap_or_else(|| "__typename".to_string());

    let aliases: String = (0..1000)
        .map(|i| format!("a{i}: {query_field}"))
        .collect::<Vec<_>>()
        .join(" ");
    plan.push(GraphQLAttackCase {
        kind: GraphQLAttackKind::AliasOverload,
        name: "alias overload x1000".into(),
        method: "POST".into(),
        body: serde_json::json!({
            "query": format!("query AliasOverload {{ {aliases} }}"),
        })
        .to_string(),
        repeat: 1,
        notes:
            "1000 aliased copies of one query field in a single request. Servers without alias-count limits will execute every alias serially — measure response time and rate-limit if it grows linearly."
                .into(),
    });

    let batched = serde_json::Value::Array(
        (0..100)
            .map(|_| {
                serde_json::json!({ "query": format!("query {{ {query_field} }}") })
            })
            .collect(),
    );
    plan.push(GraphQLAttackCase {
        kind: GraphQLAttackKind::BatchedQueries,
        name: "batched-queries x100".into(),
        method: "POST".into(),
        body: batched.to_string(),
        repeat: 1,
        notes:
            "Array-shaped GraphQL POST executing 100 queries in one request. Same DoS class as alias overload but uses the batching protocol path — block at the gateway."
                .into(),
    });

    // Build a nested introspection query — every server supports __type/fields/type.
    let mut nested = String::from("query DeepNesting { __schema { types { fields { type ");
    for _ in 0..15 {
        nested.push_str("{ ofType ");
    }
    for _ in 0..15 {
        nested.push_str("} ");
    }
    nested.push_str("} } } }");
    plan.push(GraphQLAttackCase {
        kind: GraphQLAttackKind::DeepNesting,
        name: "deep-nesting x15".into(),
        method: "POST".into(),
        body: serde_json::json!({ "query": nested }).to_string(),
        repeat: 1,
        notes:
            "Deeply nested introspection. Servers without depth limits often timeout or OOM. Reject queries > 7 levels deep at the resolver."
                .into(),
    });

    plan.push(GraphQLAttackCase {
        kind: GraphQLAttackKind::FieldSuggestionLeak,
        name: "field-suggestion leak".into(),
        method: "POST".into(),
        body: serde_json::json!({
            "query": "query { neighbourFieldThatDoesntExist }"
        })
        .to_string(),
        repeat: 1,
        notes:
            "If the error message names other valid fields (`did you mean: …`), the server is leaking the schema even with introspection disabled. Turn off field suggestions in production."
                .into(),
    });

    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CapturedRequest;
    use base64::Engine;

    fn flow_with_body(method: &str, path: &str, body: &str) -> HttpFlow {
        let req = CapturedRequest {
            method: method.into(),
            url: format!("https://example.com{path}"),
            scheme: "https".into(),
            authority: "example.com".into(),
            path: path.into(),
            http_version: "HTTP/1.1".into(),
            headers: Vec::new(),
            body_b64: base64::engine::general_purpose::STANDARD.encode(body.as_bytes()),
            body_size: body.len(),
        };
        HttpFlow::new(req)
    }

    #[test]
    fn detects_post_graphql_query_body() {
        let flow = flow_with_body(
            "POST",
            "/graphql",
            r#"{"query":"query { me { id } }","variables":{}}"#,
        );
        assert!(is_graphql_request(&flow));
    }

    #[test]
    fn detects_get_with_query_param() {
        let mut flow = flow_with_body("GET", "/api/graphql?query={me{id}}", "");
        flow.request.body_b64 = String::new();
        flow.request.body_size = 0;
        assert!(is_graphql_request(&flow));
    }

    #[test]
    fn does_not_detect_rest_request() {
        let flow = flow_with_body("POST", "/users", r#"{"name":"jane"}"#);
        assert!(!is_graphql_request(&flow));
    }

    #[test]
    fn parse_introspection_extracts_types() {
        let resp = serde_json::json!({
            "data": {
                "__schema": {
                    "queryType": { "name": "Query" },
                    "mutationType": { "name": "Mutation" },
                    "subscriptionType": null,
                    "types": [
                        { "name": "Query", "kind": "OBJECT",
                          "fields": [{ "name": "me" }, { "name": "users" }] },
                        { "name": "Mutation", "kind": "OBJECT",
                          "fields": [{ "name": "login" }] },
                        { "name": "String", "kind": "SCALAR", "fields": null }
                    ]
                }
            }
        });
        let schema = parse_introspection(&resp).unwrap();
        assert_eq!(schema.query_type.as_deref(), Some("Query"));
        assert_eq!(schema.mutation_type.as_deref(), Some("Mutation"));
        assert!(schema.subscription_type.is_none());
        assert_eq!(schema.types.len(), 3);
        let query = schema.types.iter().find(|t| t.name == "Query").unwrap();
        assert_eq!(query.fields, vec!["me", "users"]);
    }

    #[test]
    fn build_attack_plan_emits_five_kinds() {
        let plan = build_attack_plan(None);
        assert_eq!(plan.len(), 5);
        assert!(plan.iter().any(|c| c.kind == GraphQLAttackKind::IntrospectionEnabled));
        assert!(plan.iter().any(|c| c.kind == GraphQLAttackKind::AliasOverload));
        assert!(plan.iter().any(|c| c.kind == GraphQLAttackKind::BatchedQueries));
        assert!(plan.iter().any(|c| c.kind == GraphQLAttackKind::DeepNesting));
        assert!(plan.iter().any(|c| c.kind == GraphQLAttackKind::FieldSuggestionLeak));
    }

    #[test]
    fn alias_overload_uses_schema_field_when_present() {
        let schema = GraphQLSchema {
            query_type: Some("Query".into()),
            mutation_type: None,
            subscription_type: None,
            types: vec![GraphQLType {
                name: "Query".into(),
                kind: "OBJECT".into(),
                fields: vec!["currentUser".into(), "other".into()],
            }],
        };
        let plan = build_attack_plan(Some(&schema));
        let alias = plan
            .iter()
            .find(|c| c.kind == GraphQLAttackKind::AliasOverload)
            .unwrap();
        assert!(alias.body.contains("currentUser"));
    }
}
