export default function StatsBar({ stats, graphData, viewMode }) {
  return (
    <div
      className="h-7 flex items-center px-4 gap-6 flex-shrink-0"
      style={{
        borderTop: '1px solid var(--border-subtle)',
        background: 'rgba(6, 6, 10, 0.6)',
        backdropFilter: 'blur(12px)',
      }}
    >
      {graphData && (
        <div className="flex items-center gap-2">
          <span
            className="status-dot"
            style={{
              width: 5, height: 5,
              background: viewMode === "subgraph"
                ? 'rgba(92, 124, 250, 0.7)'
                : 'rgba(74, 222, 128, 0.6)',
              boxShadow: viewMode === "subgraph"
                ? '0 0 6px rgba(92, 124, 250, 0.3)'
                : '0 0 6px rgba(74, 222, 128, 0.2)',
            }}
          />
          <span className="mono text-[10px]" style={{ color: 'var(--text-tertiary)' }}>
            {viewMode === "subgraph" ? "Subgraph" : "Full graph"}
          </span>
          <span className="mono text-[10px]" style={{ color: 'var(--text-ghost)' }}>
            {graphData.nodes.length} nodes · {graphData.links.length} edges
          </span>
        </div>
      )}

      {stats && (
        <div className="flex items-center gap-2">
          <span style={{ color: 'rgba(255,255,255,0.06)' }}>|</span>
          <span className="mono text-[10px]" style={{ color: 'var(--text-ghost)' }}>
            DB: {stats.entity_count} entities · {stats.relationship_count} rels
          </span>
        </div>
      )}

      <div className="ml-auto flex items-center gap-3">
        <span className="label-xs" style={{ color: 'var(--text-ghost)', letterSpacing: '1px' }}>
          scroll to zoom · drag to pan · click to select
        </span>
      </div>
    </div>
  );
}
