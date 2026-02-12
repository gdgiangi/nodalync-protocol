import { useState, useCallback } from "react";

export default function SearchBar({ onSearch }) {
  const [query, setQuery] = useState("");
  const [debounceTimer, setDebounceTimer] = useState(null);

  const handleChange = useCallback(
    (e) => {
      const value = e.target.value;
      setQuery(value);

      // Debounce search
      if (debounceTimer) clearTimeout(debounceTimer);
      const timer = setTimeout(() => {
        onSearch(value);
      }, 300);
      setDebounceTimer(timer);
    },
    [onSearch, debounceTimer]
  );

  const handleKeyDown = (e) => {
    if (e.key === "Enter") {
      if (debounceTimer) clearTimeout(debounceTimer);
      onSearch(query);
    }
    if (e.key === "Escape") {
      setQuery("");
      onSearch("");
    }
  };

  return (
    <div className="flex-1 max-w-md">
      <input
        type="text"
        value={query}
        onChange={handleChange}
        onKeyDown={handleKeyDown}
        placeholder="Search entities..."
        className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200 placeholder-gray-500 focus:outline-none focus:border-nodalync-500 transition-colors"
      />
    </div>
  );
}
