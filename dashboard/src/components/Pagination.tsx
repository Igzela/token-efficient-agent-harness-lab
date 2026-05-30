export function Pagination({
  page,
  totalPages,
  onPageChange,
}: {
  page: number;
  totalPages: number;
  onPageChange: (page: number) => void;
}) {
  if (totalPages <= 1) return null;
  return (
    <div className="flex-row" style={{ justifyContent: "center", marginTop: 8 }}>
      <button
        onClick={() => onPageChange(Math.max(0, page - 1))}
        disabled={page === 0}
        type="button"
      >
        Prev
      </button>
      <span className="muted" style={{ fontSize: 12, alignSelf: "center" }}>
        Page {page + 1} of {totalPages}
      </span>
      <button
        onClick={() => onPageChange(Math.min(totalPages - 1, page + 1))}
        disabled={page >= totalPages - 1}
        type="button"
      >
        Next
      </button>
    </div>
  );
}
