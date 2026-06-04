export function SearchBar({
  search,
  onSearchChange,
  resultCount,
  resultText,
  label,
  placeholder,
}: {
  search: string;
  onSearchChange: (value: string) => void;
  resultCount: number;
  resultText?: string;
  label: string;
  placeholder: string;
}) {
  return (
    <div className="flex-row">
      <input
        placeholder={placeholder}
        value={search}
        onChange={(e) => onSearchChange(e.target.value)}
        className="search-input"
        aria-label={placeholder}
      />
      <span className="muted result-count">
        {resultText ?? `${resultCount} ${label}${resultCount !== 1 ? "s" : ""}`}
      </span>
    </div>
  );
}
