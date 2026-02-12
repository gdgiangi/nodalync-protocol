import { useState, useCallback, useRef } from "react";

export default function SearchBar({ onSearch }) {
  const [query, setQuery] = useState("");
  const [focused, setFocused] = useState(false);
  const debounceRef = useRef(null);

  const handleChange = useCallback(
    (e) => {
      const value = e.target.value;
      setQuery(value);

      if (debounceRef.current) clearTimeout(debounceRef.current);
      debounceRef.current = setTimeout(() => {
        onSearch(value);
      }, 300);
    },
    [onSearch]
  );

  const handleKeyDown = (e) => {
    if (e.key === "Enter") {
      if (debounceRef.current) clearTimeout(debounceRef.current);
      onSearch(query);
    }
    if (e.key === "Escape") {
      setQuery("");
      onSearch("");
      e.target.blur();
    }
  };

  return (
    <div className="flex-1 max-w-md relative">
      {/* Search icon */}
      <svg
        className="absolute left-3 top-1/2 -translate-y-1/2 pointer-events-none transition-colors duration-150"
        width="13"
        height="13"
        viewBox="0 0 24 24"
        fill="none"
        stroke={focused ? 'rgba(92, 124, 250, 0.6)' : 'rgba(255, 255, 255, 0.2)'}
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <circle cx="11" cy="11" r="8" />
        <line x1="21" y1="21" x2="16.65" y2="16.65" />
      </svg>

      <input
        type="text"
        value={query}
        onChange={handleChange}
        onKeyDown={handleKeyDown}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        placeholder="Search entities..."
        className="input pl-8 pr-3 py-1.5 text-[12px]"
        style={{
          background: focused ? 'rgba(255, 255, 255, 0.04)' : 'rgba(255, 255, 255, 0.02)',
        }}
      />

      {/* Clear button */}
      {query && (
        <button
          onClick={() => {
            setQuery("");
            onSearch("");
          }}
          className="absolute right-2 top-1/2 -translate-y-1/2 w-5 h-5 flex items-center justify-center rounded hover:bg-[rgba(255,255,255,0.06)] transition-colors duration-150"
          style={{ color: 'var(--text-ghost)' }}
        >
          <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      )}

      {/* Keyboard hint */}
      {!focused && !query && (
        <div
          className="absolute right-2.5 top-1/2 -translate-y-1/2 pointer-events-none"
        >
          <kbd
            className="mono text-[9px] px-1.5 py-0.5 rounded"
            style={{
              color: 'var(--text-ghost)',
              background: 'rgba(255, 255, 255, 0.03)',
              border: '1px solid rgba(255, 255, 255, 0.06)',
            }}
          >
            /
          </kbd>
        </div>
      )}
    </div>
  );
}
