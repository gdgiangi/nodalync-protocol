import { useState } from "react";

const TYPE_COLORS = {
  Person: "#e599f7", Organization: "#74c0fc", Concept: "#69db7c",
  Decision: "#ffd43b", Task: "#ff8787", Asset: "#a9e34b",
  Goal: "#f783ac", Pattern: "#66d9e8", Insight: "#b197fc",
};

export default function Sidebar({
  stats, selectedEntity, searchResults, onEntitySelect, onShowFullGraph, viewMode
}) {
  const [collapsed, setCollapsed] = useState(false);

  if (collapsed) {
    return (
      <div className="w-10 bg-gray-900 border-r border-gray-800 flex flex-col items-center pt-3">
        <button onClick={() => setCollapsed(false)} className="text-gray-400 hover:text-white text-lg" title="Expand sidebar">
          →
        </button>
      </div>
    );
  }

  return (
    <div className="w-72 bg-gray-900 border-r border-gray-800 flex flex-col overflow-hidden">
      {/* Header */}
      <div className="h-12 border-b border-gray-800 flex items-center justify-between px-3">
        <span className="text-xs font-semibold text-gray-400 tracking-wider">KNOWLEDGE GRAPH</span>
        <button onClick={() => setCollapsed(true)} className="text-gray-500 hover:text-gray-300 text-sm">←</button>
      </div>

      {/* Stats section */}
      {stats && (
        <div className="p-3 border-b border-gray-800">
          <div className="grid grid-cols-3 gap-2">
            <StatBox label="Entities" value={stats.entity_count} />
            <StatBox label="Relations" value={stats.relationship_count} />
            <StatBox label="Content" value={stats.content_count} />
          </div>
        </div>
      )}

      {/* Entity type legend */}
      {stats && stats.type_breakdown && (
        <div className="p-3 border-b border-gray-800 overflow-y-auto max-h-48">
          <h3 className="text-xs font-semibold text-gray-500 mb-2">ENTITY TYPES</h3>
          {stats.type_breakdown.map((t) => (
            <div key={t.entity_type} className="flex items-center justify-between py-0.5 text-xs">
              <div className="flex items-center gap-1.5">
                <span
                  className="inline-block w-2 h-2 rounded-full"
                  style={{ backgroundColor: TYPE_COLORS[t.entity_type] || "#868e96" }}
                />
                <span className="text-gray-300">{t.entity_type}</span>
              </div>
              <span className="text-gray-500">{t.count}</span>
            </div>
          ))}
        </div>
      )}

      {/* Search results */}
      {searchResults.length > 0 && (
        <div className="flex-1 overflow-y-auto p-3">
          <h3 className="text-xs font-semibold text-gray-500 mb-2">
            SEARCH RESULTS ({searchResults.length})
          </h3>
          {searchResults.map((r) => (
            <button
              key={r.id}
              onClick={() => onEntitySelect(r.id)}
              className="w-full text-left p-2 rounded hover:bg-gray-800 transition-colors mb-1"
            >
              <div className="flex items-center gap-1.5">
                <span
                  className="inline-block w-2 h-2 rounded-full flex-shrink-0"
                  style={{ backgroundColor: TYPE_COLORS[r.entity_type] || "#868e96" }}
                />
                <span className="text-sm text-gray-200 truncate">{r.label}</span>
              </div>
              <span className="text-xs text-gray-500 ml-3.5">{r.entity_type} · {r.source_count} sources</span>
            </button>
          ))}
        </div>
      )}

      {/* Selected entity detail */}
      {selectedEntity && typeof selectedEntity === "object" && (
        <div className="p-3 border-t border-gray-800">
          <h3 className="text-xs font-semibold text-gray-500 mb-2">SELECTED</h3>
          <div className="flex items-center gap-1.5 mb-1">
            <span
              className="inline-block w-2.5 h-2.5 rounded-full"
              style={{ backgroundColor: TYPE_COLORS[selectedEntity.entity_type] || "#868e96" }}
            />
            <span className="text-sm font-medium text-gray-200">{selectedEntity.label}</span>
          </div>
          <p className="text-xs text-gray-500">{selectedEntity.entity_type}</p>
          {selectedEntity.description && (
            <p className="text-xs text-gray-400 mt-1 line-clamp-3">{selectedEntity.description}</p>
          )}
          <button
            onClick={() => onEntitySelect(selectedEntity.id)}
            className="mt-2 text-xs px-2 py-1 bg-nodalync-700 hover:bg-nodalync-600 rounded text-white transition-colors"
          >
            Focus on this entity
          </button>
        </div>
      )}

      {/* Navigation */}
      <div className="mt-auto p-3 border-t border-gray-800">
        {viewMode === "subgraph" && (
          <button
            onClick={onShowFullGraph}
            className="w-full text-xs px-3 py-2 bg-gray-800 hover:bg-gray-700 rounded text-gray-300 transition-colors"
          >
            ← Show Full Graph
          </button>
        )}
      </div>
    </div>
  );
}

function StatBox({ label, value }) {
  return (
    <div className="text-center">
      <div className="text-lg font-bold text-gray-200">{value}</div>
      <div className="text-[10px] text-gray-500 uppercase">{label}</div>
    </div>
  );
}
