import { useState, useEffect, useRef, useCallback, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";

const TYPE_COLORS = {
  Person: "#e599f7",
  Organization: "#74c0fc",
  Concept: "#69db7c",
  Decision: "#ffd43b",
  Task: "#ff8787",
  Asset: "#a9e34b",
  Goal: "#f783ac",
  Pattern: "#66d9e8",
  Insight: "#b197fc",
  Value: "#ffa94d",
  Technology: "#20c997",
  Event: "#87ceeb",
  Location: "#dda0dd",
  Product: "#98d8c8",
  Work: "#fff176",
  Metric: "#ff7043",
  TimePoint: "#ab47bc",
};

function getColor(type) {
  return TYPE_COLORS[type] || "#868e96";
}

// Built-in actions
const ACTIONS = [
  {
    id: "action:new-content",
    kind: "action",
    label: "New Content",
    description: "Create a new L0 document",
    shortcut: "Ctrl+N",
    icon: "plus",
  },
  {
    id: "action:full-graph",
    kind: "action",
    label: "Show Full Graph",
    description: "Reset to full knowledge graph view",
    shortcut: "Ctrl+G",
    icon: "graph",
  },
  {
    id: "action:toggle-sidebar",
    kind: "action",
    label: "Toggle Sidebar",
    description: "Show or hide the sidebar panel",
    shortcut: "Ctrl+B",
    icon: "sidebar",
  },
  {
    id: "action:review-queue",
    kind: "action",
    label: "Review Queue",
    description: "Open the entity review queue",
    shortcut: "Ctrl+R",
    icon: "review",
  },
  {
    id: "action:settings",
    kind: "action",
    label: "Settings",
    description: "Open application settings",
    shortcut: "Ctrl+,",
    icon: "settings",
  },
  {
    id: "action:zoom-fit",
    kind: "action",
    label: "Fit Graph to View",
    description: "Reset zoom to show all nodes",
    shortcut: "Ctrl+0",
    icon: "fit",
  },
];

// Simple fuzzy match — scores how well query matches text
function fuzzyMatch(query, text) {
  if (!query) return { match: true, score: 0 };
  const q = query.toLowerCase();
  const t = text.toLowerCase();

  // Exact substring match
  if (t.includes(q)) {
    const idx = t.indexOf(q);
    // Bonus for start-of-word matches
    const startBonus = idx === 0 ? 100 : t[idx - 1] === " " ? 50 : 0;
    return { match: true, score: 200 + startBonus - idx };
  }

  // Fuzzy: all query chars must appear in order
  let qi = 0;
  let score = 0;
  let prevIdx = -2;
  for (let ti = 0; ti < t.length && qi < q.length; ti++) {
    if (t[ti] === q[qi]) {
      score += 10;
      // Consecutive chars bonus
      if (ti === prevIdx + 1) score += 15;
      // Start of word bonus
      if (ti === 0 || t[ti - 1] === " ") score += 20;
      prevIdx = ti;
      qi++;
    }
  }

  if (qi === q.length) return { match: true, score };
  return { match: false, score: 0 };
}

// Icon components
function ActionIcon({ type }) {
  const iconStyle = { color: "var(--text-ghost)" };
  const props = {
    width: 16,
    height: 16,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.5,
    strokeLinecap: "round",
    strokeLinejoin: "round",
    style: iconStyle,
  };

  switch (type) {
    case "plus":
      return (
        <svg {...props}>
          <line x1="12" y1="5" x2="12" y2="19" />
          <line x1="5" y1="12" x2="19" y2="12" />
        </svg>
      );
    case "graph":
      return (
        <svg {...props}>
          <circle cx="6" cy="6" r="3" />
          <circle cx="18" cy="18" r="3" />
          <circle cx="18" cy="6" r="3" />
          <line x1="8.5" y1="7.5" x2="15.5" y2="16.5" />
          <line x1="15.5" y1="7.5" x2="8.5" y2="7.5" />
        </svg>
      );
    case "sidebar":
      return (
        <svg {...props}>
          <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
          <line x1="9" y1="3" x2="9" y2="21" />
        </svg>
      );
    case "review":
      return (
        <svg {...props}>
          <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" />
          <polyline points="22 4 12 14.01 9 11.01" />
        </svg>
      );
    case "settings":
      return (
        <svg {...props}>
          <circle cx="12" cy="12" r="3" />
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" />
        </svg>
      );
    case "fit":
      return (
        <svg {...props}>
          <path d="M8 3H5a2 2 0 0 0-2 2v3m18 0V5a2 2 0 0 0-2-2h-3m0 18h3a2 2 0 0 0 2-2v-3M3 16v3a2 2 0 0 0 2 2h3" />
        </svg>
      );
    default:
      return (
        <svg {...props}>
          <circle cx="12" cy="12" r="2" />
        </svg>
      );
  }
}

export default function CommandPalette({
  isOpen,
  onClose,
  onAction,
  onEntitySelect,
}) {
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [entityResults, setEntityResults] = useState([]);
  const [searching, setSearching] = useState(false);
  const inputRef = useRef(null);
  const listRef = useRef(null);
  const debounceRef = useRef(null);

  // Reset on open
  useEffect(() => {
    if (isOpen) {
      setQuery("");
      setSelectedIndex(0);
      setEntityResults([]);
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [isOpen]);

  // Search entities when query changes
  useEffect(() => {
    if (!query.trim()) {
      setEntityResults([]);
      return;
    }

    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(async () => {
      try {
        setSearching(true);
        const results = await invoke("search_entities", {
          query: query.trim(),
          limit: 8,
        });
        setEntityResults(results || []);
      } catch {
        setEntityResults([]);
      } finally {
        setSearching(false);
      }
    }, 200);

    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [query]);

  // Build filtered results list
  const results = useMemo(() => {
    const items = [];

    // Filter actions by query
    const filteredActions = ACTIONS.map((action) => {
      const { match, score } = fuzzyMatch(
        query,
        action.label + " " + action.description
      );
      return { ...action, match, score };
    })
      .filter((a) => a.match)
      .sort((a, b) => b.score - a.score);

    // Entity results (from backend search)
    const entities = entityResults.map((e) => ({
      id: `entity:${e.id}`,
      kind: "entity",
      entityId: e.id,
      label: e.label || e.canonical_label,
      description: e.entity_type,
      entityType: e.entity_type,
      sourceCount: e.source_count || 0,
      score: 500, // Entities rank high when searching
    }));

    // Interleave: entities first if searching, then actions
    if (query.trim()) {
      items.push(...entities);
      items.push(...filteredActions);
    } else {
      items.push(...filteredActions);
    }

    return items;
  }, [query, entityResults]);

  // Reset index when results change
  useEffect(() => {
    setSelectedIndex(0);
  }, [results.length]);

  // Scroll selected item into view
  useEffect(() => {
    if (!listRef.current) return;
    const selected = listRef.current.children[selectedIndex];
    if (selected) {
      selected.scrollIntoView({ block: "nearest", behavior: "smooth" });
    }
  }, [selectedIndex]);

  const handleKeyDown = useCallback(
    (e) => {
      switch (e.key) {
        case "ArrowDown":
          e.preventDefault();
          setSelectedIndex((i) => Math.min(i + 1, results.length - 1));
          break;
        case "ArrowUp":
          e.preventDefault();
          setSelectedIndex((i) => Math.max(i - 1, 0));
          break;
        case "Enter":
          e.preventDefault();
          if (results[selectedIndex]) {
            executeItem(results[selectedIndex]);
          }
          break;
        case "Escape":
          e.preventDefault();
          onClose();
          break;
      }
    },
    [results, selectedIndex, onClose]
  );

  function executeItem(item) {
    onClose();
    if (item.kind === "entity") {
      onEntitySelect?.(item.entityId);
    } else if (item.kind === "action") {
      onAction?.(item.id);
    }
  }

  if (!isOpen) return null;

  return (
    <>
      {/* Backdrop */}
      <div
        className="fixed inset-0 z-[100]"
        style={{
          background: "rgba(0, 0, 0, 0.5)",
          backdropFilter: "blur(4px)",
          WebkitBackdropFilter: "blur(4px)",
          animation: "fade-in 150ms ease-out",
        }}
        onClick={onClose}
      />

      {/* Palette */}
      <div
        className="fixed z-[101] animate-scale-in"
        style={{
          top: "min(20%, 140px)",
          left: "50%",
          transform: "translateX(-50%)",
          width: "min(560px, calc(100vw - 48px))",
        }}
      >
        <div
          style={{
            background: "rgba(12, 12, 18, 0.95)",
            backdropFilter: "blur(32px)",
            WebkitBackdropFilter: "blur(32px)",
            border: "1px solid rgba(255, 255, 255, 0.08)",
            borderRadius: 12,
            boxShadow:
              "0 24px 80px rgba(0, 0, 0, 0.6), 0 0 0 1px rgba(255, 255, 255, 0.04)",
            overflow: "hidden",
          }}
        >
          {/* Search input */}
          <div
            className="flex items-center gap-3 px-4"
            style={{
              height: 52,
              borderBottom: "1px solid rgba(255, 255, 255, 0.06)",
            }}
          >
            {/* Search icon */}
            <svg
              width="16"
              height="16"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
              strokeLinejoin="round"
              style={{ color: "var(--text-ghost)", flexShrink: 0 }}
            >
              <circle cx="11" cy="11" r="8" />
              <line x1="21" y1="21" x2="16.65" y2="16.65" />
            </svg>

            <input
              ref={inputRef}
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Search entities and actions..."
              style={{
                flex: 1,
                background: "transparent",
                border: "none",
                outline: "none",
                fontSize: 14,
                fontWeight: 300,
                color: "var(--text-primary)",
                fontFamily:
                  "'SF Pro Display', -apple-system, 'Segoe UI', sans-serif",
                letterSpacing: "0.2px",
              }}
            />

            {/* Loading indicator */}
            {searching && (
              <div
                className="w-1.5 h-1.5 rounded-full"
                style={{
                  background: "var(--accent)",
                  animation: "pulse-subtle 1s ease-in-out infinite",
                  flexShrink: 0,
                }}
              />
            )}

            {/* Escape hint */}
            <kbd
              className="mono text-[9px] px-1.5 py-0.5 rounded flex-shrink-0"
              style={{
                color: "var(--text-ghost)",
                background: "rgba(255, 255, 255, 0.03)",
                border: "1px solid rgba(255, 255, 255, 0.06)",
              }}
            >
              ESC
            </kbd>
          </div>

          {/* Results */}
          <div
            ref={listRef}
            style={{
              maxHeight: 360,
              overflowY: "auto",
              padding: "4px 0",
            }}
          >
            {results.length === 0 && query.trim() && !searching && (
              <div
                className="px-4 py-6 text-center"
                style={{ color: "var(--text-ghost)", fontSize: 12 }}
              >
                No results for "{query}"
              </div>
            )}

            {/* Section labels */}
            {results.length > 0 && !query.trim() && (
              <div
                className="px-4 pt-2 pb-1"
                style={{
                  fontSize: 8,
                  letterSpacing: 3,
                  textTransform: "uppercase",
                  color: "var(--text-label)",
                }}
              >
                ACTIONS
              </div>
            )}

            {results.map((item, i) => {
              const isSelected = i === selectedIndex;

              // Section divider between entities and actions
              const showActionHeader =
                query.trim() &&
                item.kind === "action" &&
                i > 0 &&
                results[i - 1].kind === "entity";

              return (
                <div key={item.id}>
                  {showActionHeader && (
                    <div
                      className="px-4 pt-3 pb-1"
                      style={{
                        fontSize: 8,
                        letterSpacing: 3,
                        textTransform: "uppercase",
                        color: "var(--text-label)",
                      }}
                    >
                      ACTIONS
                    </div>
                  )}
                  {query.trim() && item.kind === "entity" && i === 0 && (
                    <div
                      className="px-4 pt-2 pb-1"
                      style={{
                        fontSize: 8,
                        letterSpacing: 3,
                        textTransform: "uppercase",
                        color: "var(--text-label)",
                      }}
                    >
                      ENTITIES
                    </div>
                  )}
                  <ResultItem
                    item={item}
                    isSelected={isSelected}
                    onClick={() => executeItem(item)}
                    onHover={() => setSelectedIndex(i)}
                  />
                </div>
              );
            })}
          </div>

          {/* Footer */}
          <div
            className="flex items-center justify-between px-4"
            style={{
              height: 32,
              borderTop: "1px solid rgba(255, 255, 255, 0.04)",
              background: "rgba(0, 0, 0, 0.15)",
            }}
          >
            <div className="flex items-center gap-3">
              <FooterHint keys={["↑", "↓"]} label="navigate" />
              <FooterHint keys={["↵"]} label="select" />
              <FooterHint keys={["esc"]} label="close" />
            </div>
            <span
              className="mono text-[8px]"
              style={{ color: "var(--text-ghost)", letterSpacing: 1 }}
            >
              {results.length} result{results.length !== 1 ? "s" : ""}
            </span>
          </div>
        </div>
      </div>
    </>
  );
}

// ─── Sub-components ──────────────────────────────────────────────────────────

function ResultItem({ item, isSelected, onClick, onHover }) {
  return (
    <button
      onClick={onClick}
      onMouseEnter={onHover}
      className="w-full text-left flex items-center gap-3 px-4 py-2 transition-colors"
      style={{
        background: isSelected ? "rgba(92, 124, 250, 0.08)" : "transparent",
        border: "none",
        cursor: "pointer",
        outline: "none",
        minHeight: 40,
      }}
    >
      {/* Icon / indicator */}
      <div
        className="w-8 h-8 rounded-lg flex items-center justify-center flex-shrink-0"
        style={{
          background:
            item.kind === "entity"
              ? getColor(item.entityType) + "12"
              : "rgba(255, 255, 255, 0.03)",
          border:
            item.kind === "entity"
              ? `1px solid ${getColor(item.entityType)}20`
              : "1px solid rgba(255, 255, 255, 0.04)",
        }}
      >
        {item.kind === "entity" ? (
          <div
            className="w-2.5 h-2.5 rounded-full"
            style={{
              background: getColor(item.entityType),
              opacity: 0.8,
            }}
          />
        ) : (
          <ActionIcon type={item.icon} />
        )}
      </div>

      {/* Label + description */}
      <div className="flex-1 min-w-0">
        <div
          className="text-[13px] truncate"
          style={{
            color: isSelected
              ? "var(--text-primary)"
              : "var(--text-secondary)",
            fontWeight: isSelected ? 400 : 300,
          }}
        >
          {item.label}
        </div>
        {item.description && (
          <div
            className="text-[10px] truncate mt-0.5"
            style={{ color: "var(--text-ghost)" }}
          >
            {item.kind === "entity"
              ? `${item.description} · ${item.sourceCount} source${item.sourceCount !== 1 ? "s" : ""}`
              : item.description}
          </div>
        )}
      </div>

      {/* Shortcut badge */}
      {item.shortcut && (
        <div className="flex items-center gap-1 flex-shrink-0">
          {item.shortcut.split("+").map((key, i) => (
            <kbd
              key={i}
              className="mono text-[9px] px-1.5 py-0.5 rounded"
              style={{
                color: "var(--text-ghost)",
                background: "rgba(255, 255, 255, 0.03)",
                border: "1px solid rgba(255, 255, 255, 0.06)",
                lineHeight: 1,
              }}
            >
              {key}
            </kbd>
          ))}
        </div>
      )}
    </button>
  );
}

function FooterHint({ keys, label }) {
  return (
    <div className="flex items-center gap-1.5">
      {keys.map((key, i) => (
        <kbd
          key={i}
          className="mono text-[8px] px-1 py-px rounded"
          style={{
            color: "var(--text-ghost)",
            background: "rgba(255, 255, 255, 0.03)",
            border: "1px solid rgba(255, 255, 255, 0.05)",
            lineHeight: 1.2,
          }}
        >
          {key}
        </kbd>
      ))}
      <span
        className="text-[8px]"
        style={{ color: "var(--text-ghost)", letterSpacing: 0.5 }}
      >
        {label}
      </span>
    </div>
  );
}
