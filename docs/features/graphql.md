# GraphQL native support (Feature R)

NyxProxy detects GraphQL traffic, parses introspection responses, and
generates a deterministic attack plan covering the five GraphQL-specific
abuse classes that no REST-aware proxy catches.

## Detection

`is_graphql_request` recognises a captured `HttpFlow` as GraphQL when:

- The method is `POST` and the body parses as JSON with a top-level
  `query`, `operationName`, or array (batched-queries) field.
- The method is `GET` and the URL contains a `query=` parameter.

Path patterns are **not** required to be `/graphql` — many APIs use
`/api/graphql`, `/v1/gql`, or `/internal/__graphql`.

Detected endpoints appear in the **GraphQL** page (sidebar → Tools).

## Introspection

`graphql_introspection_query` returns the canonical `__schema` query.
Paste it into Repeater and POST it at the target. The JSON response
goes into the GraphQL page's "Introspection" textarea — press *Parse
schema* to extract:

- query / mutation / subscription type names
- every type's kind (OBJECT, SCALAR, INPUT_OBJECT, …)
- top-level fields on every OBJECT type

## Attack plan

`graphql_build_attack_plan(schema)` produces five test cases. Each
case is a fully-formed request body you can fire from Intruder.

| Kind | What it does | Defence |
|---|---|---|
| `introspection-enabled` | Sends the canonical introspection query. If `data.__schema` is non-empty, introspection is enabled in production. | Disable introspection in prod via Apollo's `introspection: false` (or equivalent). |
| `alias-overload` | One query, 1000 aliased copies of a top-level field. Servers without alias-count limits execute every alias serially. | Cap alias count per request at the gateway. |
| `batched-queries` | Array-shaped POST with 100 identical queries. Same DoS class but uses the batching protocol path. | Reject arrays > N or disable batching. |
| `deep-nesting` | 15-level deep `ofType` chain inside `__schema.types.fields.type`. | Cap query depth at 7. |
| `field-suggestion-leak` | Queries an obviously-bogus field. If the error message includes `did you mean: …`, the server is leaking schema details even with introspection disabled. | Turn off field-suggestion in production. |

The plan uses an actual query-type field from the introspected schema
for `alias-overload`; if no schema is available it falls back to
`__typename`, which every server supports.

## Programmatic access

```rust
use nyxproxy_core::graphql::{introspection_query, parse_introspection, build_attack_plan};

let plan = build_attack_plan(None);
for case in plan { println!("{}: {}", case.name, case.notes); }
```

## Tested

```
cargo test -p nyxproxy-core --lib graphql
```

6 tests covering detection, GET/POST shapes, REST rejection,
introspection parsing, plan emission, and schema-driven alias overload.
