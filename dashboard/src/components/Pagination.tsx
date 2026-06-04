export function Pagination({
  hasNext,
  page,
  totalPages,
  onPageChange,
}: {
  hasNext?: boolean;
  page: number;
  totalPages?: number;
  onPageChange: (page: number) => void;
}) {
  const knownTotal = typeof totalPages === "number";
  const canPrev = page > 0;
  const canNext = knownTotal ? page < Math.max(1, totalPages) - 1 : Boolean(hasNext);
  if (!canPrev && !canNext && (!knownTotal || totalPages <= 1)) return null;
  return (
    <div className="pagination">
      <button
        onClick={() => onPageChange(Math.max(0, page - 1))}
        disabled={page === 0}
        type="button"
      >
        Prev
      </button>
      <span className="muted pagination-label">
        Page {page + 1}{knownTotal ? ` of ${Math.max(1, totalPages)}` : ""}
      </span>
      <button
        onClick={() => onPageChange(knownTotal ? Math.min(Math.max(1, totalPages) - 1, page + 1) : page + 1)}
        disabled={!canNext}
        type="button"
      >
        Next
      </button>
    </div>
  );
}
