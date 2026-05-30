import { useState } from "react";

export function usePaginatedSearch<T>(
  items: T[],
  searchFields: (keyof T)[],
  pageSize = 25,
) {
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(0);

  const q = search.toLowerCase();
  const filtered = q
    ? items.filter((item) =>
        searchFields.some((f) => String(item[f]).toLowerCase().includes(q)),
      )
    : items;
  const totalPages = Math.max(1, Math.ceil(filtered.length / pageSize));
  const pageItems = filtered.slice(page * pageSize, (page + 1) * pageSize);

  return { search, setSearch, page, setPage, filtered, pageItems, totalPages };
}
