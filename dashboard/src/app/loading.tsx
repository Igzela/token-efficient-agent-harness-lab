export default function Loading() {
  return (
    <main>
      <div className="shell" style={{ display: "flex", justifyContent: "center", marginTop: 80 }}>
        <p className="muted">
          <span className="spinner spinner-lg" />
          Loading dashboard…
        </p>
      </div>
    </main>
  );
}
