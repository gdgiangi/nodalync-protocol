import { useState } from "react";
import { getEntityColor, APP_VERSION } from "../lib/constants";

export default function Sidebar({
  stats,
  selectedEntity,
  searchResults,
  onEntitySelect,
  onShowFullGraph,
  viewMode,
}) {
  const [collapsed, setCollapsed] = useState(false);

  if (collapsed) {
    return (
      <div className="w-11 glass flex flex-col items-center pt-4 gap-3 border-r-0"
        style={{ borderRight: '1px solid var(--border-subtle)', background: 'rgba(6, 6, 10, 0.92)' }}
      >
        <button
          onClick={() => setCollapsed(false)}
          className="w-7 h-7 flex items-center justify-center rounded-md hover:bg-[rgba(255,255,255,0.06)] transition-all duration-200"
          title="Expand sidebar"
          style={{ color: 'var(--text-tertiary)' }}
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="9 18 15 12 9 6" />
          </svg>
        </button>

        {/* Mini entity type indicators when collapsed */}
        {stats?.type_breakdown?.slice(0, 5).map((t) => (
          <div
            key={t.entity_type}
            className="w-2 h-2 rounded-full opacity-50"
            style={{ backgroundColor: getEntityColor(t.entity_type) }}
            title={`${t.entity_type}: ${t.count}`}
          />
        ))}
      </div>
    );
  }

  return (
    <div
      className="w-72 flex flex-col overflow-hidden animate-fade-in"
      style={{
        background: 'rgba(6, 6, 10, 0.92)',
        backdropFilter: 'blur(24px)',
        WebkitBackdropFilter: 'blur(24px)',
        borderRight: '1px solid var(--border-subtle)',
      }}
    >
      {/* Header */}
      <div
        className="h-11 flex items-center justify-between px-3 flex-shrink-0"
        style={{ borderBottom: '1px solid var(--border-subtle)' }}
      >
        <span className="label-sm" style={{ color: 'var(--text-label)' }}>
          KNOWLEDGE GRAPH
        </span>
        <button
          onClick={() => setCollapsed(true)}
          className="w-6 h-6 flex items-center justify-center rounded hover:bg-[rgba(255,255,255,0.06)] transition-all duration-150"
          style={{ color: 'var(--text-ghost)' }}
        >
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="15 18 9 12 15 6" />
          </svg>
        </button>
      </div>

      {/* Quick Stats */}
      {stats ? (
        <div
          className="flex items-center px-3 py-2.5 flex-shrink-0"
          style={{ borderBottom: '1px solid var(--border-subtle)' }}
        >
          <StatItem label="ENTITIES" value={stats.entity_count} />
          <div className="w-px h-8 mx-3" style={{ background: 'var(--border-subtle)' }} />
          <StatItem label="RELATIONS" value={stats.relationship_count} />
          <div className="w-px h-8 mx-3" style={{ background: 'var(--border-subtle)' }} />
          <StatItem label="CONTENT" value={stats.content_count} />
        </div>
      ) : (
        <div className="px-3 py-3 flex-shrink-0" style={{ borderBottom: '1px solid var(--border-subtle)' }}>
          <div className="flex gap-3">
            <div className="skeleton h-10 flex-1 rounded" />
            <div className="skeleton h-10 flex-1 rounded" />
            <div className="skeleton h-10 flex-1 rounded" />
          </div>
        </div>
      )}

      {/* Entity Type Legend */}
      {stats?.type_breakdown && (
        <div
          className="px-3 py-2.5 overflow-y-auto max-h-44 flex-shrink-0"
          style={{ borderBottom: '1px solid var(--border-subtle)' }}
        >
          <h3 className="label-sm mb-2">ENTITY TYPES</h3>
          {stats.type_breakdown.map((t, i) => (
            <div
              key={t.entity_type}
              className="flex items-center justify-between py-0.5 animate-slide-up"
              style={{ animationDelay: `${i * 30}ms`, animationFillMode: 'both' }}
            >
              <div className="flex items-center gap-2">
                <span
                  className="inline-block w-[7px] h-[7px] rounded-full"
                  style={{ backgroundColor: getEntityColor(t.entity_type), opacity: 0.8 }}
                />
                <span className="text-[11px]" style={{ color: 'var(--text-secondary)' }}>
                  {t.entity_type}
                </span>
              </div>
              <span className="mono text-[10px]" style={{ color: 'var(--text-ghost)' }}>
                {t.count}
              </span>
            </div>
          ))}
        </div>
      )}

      {/* Search Results */}
      {searchResults.length > 0 && (
        <div className="flex-1 overflow-y-auto px-3 py-2.5">
          <h3 className="label-sm mb-2">
            SEARCH RESULTS
            <span className="ml-2 mono text-[9px]" style={{ color: 'var(--text-ghost)', letterSpacing: '0' }}>
              {searchResults.length}
            </span>
          </h3>
          {searchResults.map((r, i) => (
            <button
              key={r.id}
              onClick={() => onEntitySelect(r.id)}
              className="w-full text-left p-2 rounded-md mb-0.5 transition-all duration-200 animate-slide-up"
              style={{
                animationDelay: `${i * 40}ms`,
                animationFillMode: 'both',
                background: 'transparent',
                border: 'none',
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.background = 'var(--bg-hover)';
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.background = 'transparent';
              }}
            >
              <div className="flex items-center gap-2">
                <span
                  className="inline-block w-[7px] h-[7px] rounded-full flex-shrink-0"
                  style={{ backgroundColor: getEntityColor(r.entity_type), opacity: 0.8 }}
                />
                <span className="text-[12px] truncate" style={{ color: 'var(--text-primary)' }}>
                  {r.label}
                </span>
              </div>
              <div className="flex items-center gap-1.5 ml-[15px] mt-0.5">
                <span className="text-[10px]" style={{ color: 'var(--text-ghost)' }}>
                  {r.entity_type}
                </span>
                <span className="text-[10px]" style={{ color: 'rgba(255,255,255,0.1)' }}>·</span>
                <span className="mono text-[9px]" style={{ color: 'var(--text-ghost)' }}>
                  {r.source_count} sources
                </span>
              </div>
            </button>
          ))}
        </div>
      )}

      {/* Selected Entity Mini Card */}
      {selectedEntity && typeof selectedEntity === "object" && (
        <div
          className="px-3 py-2.5 animate-slide-up flex-shrink-0"
          style={{ borderTop: '1px solid var(--border-subtle)' }}
        >
          <h3 className="label-sm mb-2">SELECTED</h3>
          <div className="flex items-center gap-2">
            <span
              className="inline-block w-2 h-2 rounded-full flex-shrink-0"
              style={{ backgroundColor: getEntityColor(selectedEntity.entity_type), opacity: 0.9 }}
            />
            <span className="text-[12px] truncate" style={{ color: 'var(--text-primary)' }}>
              {selectedEntity.label}
            </span>
            <span className="mono text-[9px] ml-auto flex-shrink-0" style={{ color: 'var(--text-ghost)' }}>
              {selectedEntity.entity_type}
            </span>
          </div>
        </div>
      )}

      {/* Footer Navigation */}
      <div className="mt-auto px-3 py-2.5 flex-shrink-0" style={{ borderTop: '1px solid var(--border-subtle)' }}>
        {viewMode === "subgraph" ? (
          <button
            onClick={onShowFullGraph}
            className="btn w-full justify-center text-[10px]"
          >
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="15 18 9 12 15 6" />
            </svg>
            Show Full Graph
          </button>
        ) : (
          <div className="text-center">
            <span className="label-xs" style={{ color: 'var(--text-ghost)', letterSpacing: '2px' }}>
              NODALYNC STUDIO v{APP_VERSION}
            </span>
          </div>
        )}
      </div>
    </div>
  );
}

function StatItem({ label, value }) {
  return (
    <div className="flex-1 flex flex-col items-center gap-0.5">
      <span className="stat-value text-[15px]">
        {value != null ? value : '—'}
      </span>
      <span className="label-xs">{label}</span>
    </div>
  );
}
