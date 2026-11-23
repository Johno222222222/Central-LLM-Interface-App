export function PlaceholderScreen({ kicker, title, body }: { kicker: string; title: string; body: string }) {
  return (
    <main className="page">
      <div className="placeholder">
        <p className="eyebrow">{kicker}</p>
        <h1>{title}</h1>
        <p className="muted">{body}</p>
      </div>
    </main>
  );
}
