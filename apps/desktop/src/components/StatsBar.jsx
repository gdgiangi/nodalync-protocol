export default function StatsBar({ stats, graphData, viewMode }) {
  return (
    <div className="h-7 border-t border-gray-800 bg-gray-900/50 flex items-center px-4 gap-6 text-[11px] text-gray-500">
      {graphData && (
        <>
          <span>
            {viewMode === "subgraph" ? "Subgraph" : "Full graph"}: {graphData.nodes.length} nodes, {graphData.links.length} links
          </span>
        </>
      )}
      {stats && (
        <>
          <span className="border-l border-gray-700 pl-6">
            DB: {stats.entity_count} entities, {stats.relationship_count} relationships
          </span>
        </>
      )}
      <span className="ml-auto text-gray-600">
        Nodalync Studio v0.1.0 — Scroll to zoom, drag to pan, click nodes to select
      </span>
    </div>
  );
}
