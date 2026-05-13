// Tiny line-diff implementation (Myers-ish, but we cheat with LCS on lines).
// Good enough for response-vs-response comparison shown in the Comparer tab.

export type DiffOp = "eq" | "add" | "del";

export interface DiffLine {
  op: DiffOp;
  text: string;
}

export function lineDiff(a: string, b: string): DiffLine[] {
  const aLines = a.split(/\r?\n/);
  const bLines = b.split(/\r?\n/);
  const m = aLines.length;
  const n = bLines.length;
  const dp: number[][] = Array.from({ length: m + 1 }, () =>
    new Array<number>(n + 1).fill(0)
  );
  for (let i = m - 1; i >= 0; i--) {
    for (let j = n - 1; j >= 0; j--) {
      const row = dp[i]!;
      const nextRow = dp[i + 1]!;
      if (aLines[i] === bLines[j]) row[j] = nextRow[j + 1]! + 1;
      else row[j] = Math.max(nextRow[j]!, row[j + 1]!);
    }
  }
  const out: DiffLine[] = [];
  let i = 0;
  let j = 0;
  while (i < m && j < n) {
    const ai = aLines[i]!;
    const bj = bLines[j]!;
    if (ai === bj) {
      out.push({ op: "eq", text: ai });
      i++;
      j++;
    } else if (dp[i + 1]![j]! >= dp[i]![j + 1]!) {
      out.push({ op: "del", text: ai });
      i++;
    } else {
      out.push({ op: "add", text: bj });
      j++;
    }
  }
  while (i < m) out.push({ op: "del", text: aLines[i++]! });
  while (j < n) out.push({ op: "add", text: bLines[j++]! });
  return out;
}

export function diffSummary(diff: DiffLine[]): { added: number; removed: number; unchanged: number } {
  let added = 0;
  let removed = 0;
  let unchanged = 0;
  for (const line of diff) {
    if (line.op === "add") added++;
    else if (line.op === "del") removed++;
    else unchanged++;
  }
  return { added, removed, unchanged };
}
