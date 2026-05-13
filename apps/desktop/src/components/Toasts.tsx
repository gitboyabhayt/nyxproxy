import { useAppStore } from "@/state/store";

export function Toasts() {
  const toasts = useAppStore((s) => s.toasts);
  const dismiss = useAppStore((s) => s.dismissToast);

  if (toasts.length === 0) return null;

  return (
    <div
      style={{
        position: "fixed",
        right: 16,
        bottom: 32,
        display: "flex",
        flexDirection: "column",
        gap: 8,
        zIndex: 9999,
      }}
    >
      {toasts.map((t) => (
        <div
          key={t.id}
          className={`banner ${t.level}`}
          style={{
            minWidth: 240,
            maxWidth: 420,
            borderRadius: 4,
            border: "1px solid var(--border)",
            background: "var(--bg-2)",
            cursor: "pointer",
          }}
          onClick={() => dismiss(t.id)}
        >
          <strong style={{ textTransform: "capitalize", marginRight: 8 }}>
            {t.level}:
          </strong>
          {t.message}
        </div>
      ))}
    </div>
  );
}
