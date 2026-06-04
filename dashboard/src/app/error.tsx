"use client";

export default function Error({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  return (
    <main>
      <div className="shell">
        <section className="card stack error-card">
          <h2>Something went wrong</h2>
          <p className="error-text">{error.message}</p>
          <button onClick={reset} type="button" className="button-primary">Try again</button>
        </section>
      </div>
    </main>
  );
}
