import { useMemo, useState } from "react";

import { SplitPane } from "@/components/SplitPane";
import { diffSummary, lineDiff } from "@/lib/diff";

export function ComparerPage() {
  const [a, setA] = useState("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<h1>Hello</h1>");
  const [b, setB] = useState("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nX-Cache: HIT\r\n\r\n<h1>Hello world</h1>");

  const diff = useMemo(() => lineDiff(a, b), [a, b]);
  const summary = useMemo(() => diffSummary(diff), [diff]);

  return (
    <SplitPane
      storageKey="comparer"
      direction="vertical"
      initialSize={0.5}
      first={
        <SplitPane
          storageKey="comparer-top"
          initialSize={0.5}
          first={
            <Pane label="Sample 1" value={a} onChange={setA} />
          }
          second={
            <Pane label="Sample 2" value={b} onChange={setB} />
          }
        />
      }
      second={
        <div className="panel" style={{ height: "100%", border: "none", borderRadius: 0 }}>
          <div className="panel-header">
            Diff{" "}
            <span style={{ marginLeft: 12, color: "var(--success)" }}>+{summary.added}</span>
            <span style={{ marginLeft: 12, color: "var(--danger)" }}>-{summary.removed}</span>
            <span style={{ marginLeft: 12, color: "var(--text-muted)" }}>= {summary.unchanged}</span>
          </div>
          <div className="panel-body" style={{ overflow: "auto", padding: 10 }}>
            <pre className="code" style={{ margin: 0 }}>
              {diff.map((line, i) => {
                const cls =
                  line.op === "add"
                    ? "diff-add"
                    : line.op === "del"
                    ? "diff-del"
                    : "diff-eq";
                const prefix = line.op === "add" ? "+ " : line.op === "del" ? "- " : "  ";
                return (
                  <div key={i} className={cls}>
                    {prefix}
                    {line.text}
                  </div>
                );
              })}
            </pre>
          </div>
        </div>
      }
    />
  );
}

function Pane({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (next: string) => void;
}) {
  return (
    <div className="panel" style={{ height: "100%", border: "none", borderRadius: 0 }}>
      <div className="panel-header">{label}</div>
      <div className="panel-body" style={{ padding: 8 }}>
        <textarea
          className="code-input"
          style={{ flex: 1, minHeight: 200 }}
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
      </div>
    </div>
  );
}
