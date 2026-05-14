import { useEffect, useMemo, useState } from "react";

import {
  GraphQLApi,
  type GraphQLAttackCase,
  type GraphQLSchema,
} from "@/tauri/api";
import { useAppStore } from "@/state/store";

export function GraphQLPage() {
  const toast = useAppStore((s) => s.toast);
  const [endpoints, setEndpoints] = useState<string[]>([]);
  const [introspectionBody, setIntrospectionBody] = useState<string>("");
  const [schema, setSchema] = useState<GraphQLSchema | null>(null);
  const [plan, setPlan] = useState<GraphQLAttackCase[]>([]);
  const [introspectionQuery, setIntrospectionQuery] = useState<string>("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    void (async () => {
      try {
        const [urls, q] = await Promise.all([
          GraphQLApi.listEndpoints(),
          GraphQLApi.introspectionQuery(),
        ]);
        setEndpoints(urls);
        setIntrospectionQuery(q);
      } catch (err) {
        toast("error", `GraphQL load failed: ${err}`);
      }
    })();
  }, [toast]);

  async function parse(): Promise<void> {
    setBusy(true);
    try {
      const s = await GraphQLApi.parseIntrospection(introspectionBody);
      setSchema(s);
      toast("info", `Schema parsed — ${s.types.length} types loaded`);
    } catch (err) {
      toast("error", `Schema parse failed: ${err}`);
    } finally {
      setBusy(false);
    }
  }

  async function buildPlan(): Promise<void> {
    setBusy(true);
    try {
      const p = await GraphQLApi.buildAttackPlan(schema ?? undefined);
      setPlan(p);
      toast("info", `Attack plan ready — ${p.length} cases generated`);
    } catch (err) {
      toast("error", `Attack plan failed: ${err}`);
    } finally {
      setBusy(false);
    }
  }

  const summary = useMemo(() => {
    if (!schema) return null;
    return {
      query: schema.query_type,
      mutation: schema.mutation_type,
      subscription: schema.subscription_type,
      typeCount: schema.types.length,
    };
  }, [schema]);

  return (
    <div className="flex h-full flex-col gap-4 p-4">
      <header className="flex items-baseline justify-between">
        <h1 className="text-xl font-semibold">GraphQL</h1>
        <span className="text-xs text-muted-foreground">Feature R</span>
      </header>

      <section className="rounded border border-border bg-card p-3">
        <h2 className="mb-2 text-sm font-medium">Detected endpoints</h2>
        {endpoints.length === 0 ? (
          <p className="text-xs text-muted-foreground">
            No GraphQL traffic seen yet. Send a `query` or `mutation` request
            through the proxy and it will appear here.
          </p>
        ) : (
          <ul className="space-y-1 text-xs">
            {endpoints.map((u) => (
              <li key={u} className="font-mono">
                {u}
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="rounded border border-border bg-card p-3">
        <h2 className="mb-2 text-sm font-medium">Introspection</h2>
        <p className="mb-2 text-xs text-muted-foreground">
          Send this query through Repeater. Paste the JSON response below and
          press "Parse schema".
        </p>
        <pre className="mb-2 overflow-x-auto rounded bg-muted p-2 text-xs">
          {introspectionQuery}
        </pre>
        <textarea
          value={introspectionBody}
          onChange={(e) => setIntrospectionBody(e.target.value)}
          className="h-32 w-full rounded border border-border bg-background p-2 font-mono text-xs"
          placeholder='{"data":{"__schema":{...}}}'
        />
        <div className="mt-2 flex items-center gap-2">
          <button
            type="button"
            className="rounded bg-primary px-3 py-1 text-xs font-medium text-primary-foreground disabled:opacity-50"
            onClick={() => void parse()}
            disabled={busy || introspectionBody.length === 0}
          >
            Parse schema
          </button>
          {summary ? (
            <span className="text-xs text-muted-foreground">
              Query: <code>{summary.query ?? "—"}</code>
              {" "}· Mutation: <code>{summary.mutation ?? "—"}</code>
              {" "}· Types: <code>{summary.typeCount}</code>
            </span>
          ) : null}
        </div>
      </section>

      <section className="flex-1 overflow-auto rounded border border-border bg-card p-3">
        <div className="mb-2 flex items-center justify-between">
          <h2 className="text-sm font-medium">Attack plan</h2>
          <button
            type="button"
            className="rounded bg-primary px-3 py-1 text-xs font-medium text-primary-foreground disabled:opacity-50"
            onClick={() => void buildPlan()}
            disabled={busy}
          >
            Build attack plan
          </button>
        </div>
        {plan.length === 0 ? (
          <p className="text-xs text-muted-foreground">
            Press "Build attack plan" to generate the 5 standard GraphQL
            test cases: introspection, alias overload, batched queries, deep
            nesting, field-suggestion leak.
          </p>
        ) : (
          <table className="w-full text-xs">
            <thead>
              <tr className="text-left text-muted-foreground">
                <th className="pb-1">Kind</th>
                <th className="pb-1">Name</th>
                <th className="pb-1">Method</th>
                <th className="pb-1">Notes</th>
              </tr>
            </thead>
            <tbody>
              {plan.map((c) => (
                <tr key={c.name} className="border-t border-border">
                  <td className="py-1 font-mono">{c.kind}</td>
                  <td className="py-1">{c.name}</td>
                  <td className="py-1">{c.method}</td>
                  <td className="py-1 text-muted-foreground">{c.notes}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
    </div>
  );
}
