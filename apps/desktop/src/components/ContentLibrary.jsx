import { useMemo, useState } from "react";
import "./studio-content.css";

const KIND_LABELS = { L0: "Original", L1: "Extraction", L2: "Entity graph", L3: "Synthesis" };

function formatSize(value) {
  const bytes = Number(value) || 0;
  if (bytes < 1024) return bytes + " B";
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KB";
  return (bytes / (1024 * 1024)).toFixed(1) + " MB";
}

function DocumentIcon() {
  return (
    <svg width="21" height="21" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
      <path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z" />
      <path d="M14 3v6h6M8 13h8M8 17h5" />
    </svg>
  );
}

export default function ContentLibrary({ items = [], onSelect, loading, error, onCreate, onImport }) {
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState({ field: "title", direction: "asc" });
  const visibleItems = useMemo(() => {
    const term = query.trim().toLowerCase();
    return items
      .filter((item) => !term || [item.title, item.hash, KIND_LABELS[item.content_type], item.visibility]
        .some((value) => String(value || "").toLowerCase().includes(term)))
      .slice()
      .sort((a, b) => {
        const aValue = sort.field === "size" ? Number(a.size) || 0 : String(a[sort.field] || "").toLowerCase();
        const bValue = sort.field === "size" ? Number(b.size) || 0 : String(b[sort.field] || "").toLowerCase();
        const order = typeof aValue === "number" ? aValue - bValue : aValue.localeCompare(bValue);
        return (order || String(a.hash).localeCompare(String(b.hash))) * (sort.direction === "asc" ? 1 : -1);
      });
  }, [items, query, sort]);

  function changeSort(field) {
    setSort((current) => ({
      field,
      direction: current.field === field && current.direction === "asc" ? "desc" : "asc",
    }));
  }

  function column(label, field, className) {
    const active = sort.field === field;
    return (
      <th className={className} scope="col" aria-sort={active ? (sort.direction === "asc" ? "ascending" : "descending") : "none"}>
        <button className="sc-sort-button" onClick={() => changeSort(field)}>
          {label}
          <span aria-hidden="true" className={active ? "sc-sort-active" : ""}>{active && sort.direction === "desc" ? "↓" : "↑"}</span>
        </button>
      </th>
    );
  }

  return (
    <section className="sc-library" aria-label="Content library" aria-busy={Boolean(loading)}>
      <div className="sc-library-toolbar">
        <div className="sc-search">
          <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" aria-hidden="true">
            <circle cx="10.5" cy="10.5" r="6.5" /><path d="m16 16 4.5 4.5" />
          </svg>
          <input type="search" aria-label="Search your library" placeholder="Search by title or content hash" value={query} onChange={(event) => setQuery(event.target.value)} />
        </div>
        <span className="sc-library-count" aria-live="polite">
          {items.length} {items.length === 1 ? "item" : "items"} in your library
        </span>
      </div>

      {error && <div className="sc-message sc-message-error" role="alert">{String(error)}</div>}

      {loading && items.length === 0 ? (
        <div className="sc-library-empty" role="status"><span className="sc-spinner" /> Loading your library…</div>
      ) : visibleItems.length === 0 ? (
        <div className="sc-library-empty">
          <div className="sc-empty-icon"><DocumentIcon /></div>
          <h2>{query.trim() ? "No matching content" : error ? "Your library is unavailable" : "Start with something you know"}</h2>
          <p>{query.trim()
            ? "Try another title, visibility, or content hash."
            : error
              ? "Reconnect to your local node to see your saved content."
              : "Write a note or import a text file. Your originals stay private on this node."}</p>
          <div className="sc-actions">
            {query.trim() ? <button className="sc-button" onClick={() => setQuery("")}>Clear search</button> : !error && (
              <>
                <button className="sc-button sc-button-primary" onClick={onCreate}>Write a note</button>
                <button className="sc-button" onClick={onImport}>Import files</button>
              </>
            )}
          </div>
        </div>
      ) : (
        <div className="sc-table-scroll">
          <table className="sc-content-table">
            <caption className="sc-sr-only">Saved content. Select a title to view its details.</caption>
            <thead>
              <tr>
                {column("Title", "title", "sc-title-column")}
                {column("Type", "content_type")}
                {column("Visibility", "visibility")}
                {column("Size", "size", "sc-size-column")}
                <th scope="col" className="sc-open-column"><span className="sc-sr-only">Open content</span></th>
              </tr>
            </thead>
            <tbody>
              {visibleItems.map((item) => (
                <tr key={item.hash}>
                  <td>
                    <button className="sc-content-title" onClick={() => onSelect?.(item)}>
                      <span className="sc-document-icon"><DocumentIcon /></span>
                      <span className="sc-title-text">
                        <span className="sc-item-title">{item.title || "Untitled"}</span>
                        <span className="sc-item-hash" title={item.hash}>{String(item.hash || "").slice(0, 12)}…</span>
                      </span>
                    </button>
                  </td>
                  <td className="sc-muted">{KIND_LABELS[item.content_type] || item.content_type || "Content"}</td>
                  <td><span className={"sc-visibility " + (String(item.visibility).toLowerCase() === "private" ? "sc-private" : "")}>{item.visibility || "Unknown"}</span></td>
                  <td className="sc-size-column sc-muted">{formatSize(item.size)}</td>
                  <td>
                    <button className="sc-icon-button" aria-label={"Open " + (item.title || "untitled content")} onClick={() => onSelect?.(item)}>
                      <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true"><path d="m9 5 7 7-7 7" /></svg>
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {visibleItems.length > 0 && <div className="sc-library-footer">{query.trim() ? visibleItems.length + " matching " + (visibleItems.length === 1 ? "item" : "items") : "Originals and knowledge saved on this node"}</div>}
    </section>
  );
}
