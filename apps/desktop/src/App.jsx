import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import GraphView from "./components/GraphView";
import Sidebar from "./components/Sidebar";
import StatsBar from "./components/StatsBar";
import SearchBar from "./components/SearchBar";

function App() {
  const [graphData, setGraphData] = useState({ nodes: [], links: [] });
  const [stats, setStats] = useState(null);
  const [selectedEntity, setSelectedEntity] = useState(null);
  const [searchResults, setSearchResults] = useState([]);
  const [loading, setLoading] = useState(true);
  const [viewMode, setViewMode] = useState("full"); // 'full' or 'subgraph'

  // Load full graph on mount
  useEffect(() => {
    loadFullGraph();
    loadStats();
  }, []);

  async function loadFullGraph() {
    try {
      setLoading(true);
      const data = await invoke("get_graph_data");
      setGraphData(data);
      setViewMode("full");
    } catch (err) {
      console.error("Failed to load graph:", err);
    } finally {
      setLoading(false);
    }
  }

  async function loadStats() {
    try {
      const s = await invoke("get_graph_stats");
      setStats(s);
    } catch (err) {
      console.error("Failed to load stats:", err);
    }
  }

  async function handleSearch(query) {
    if (!query.trim()) {
      setSearchResults([]);
      return;
    }
    try {
      const results = await invoke("search_entities", { query, limit: 20 });
      setSearchResults(results);
    } catch (err) {
      console.error("Search failed:", err);
    }
  }

  async function handleEntitySelect(entityId) {
    try {
      setLoading(true);
      const data = await invoke("get_subgraph", {
        entityId,
        maxHops: 2,
        maxResults: 50,
      });
      setGraphData(data);
      setSelectedEntity(entityId);
      setViewMode("subgraph");
    } catch (err) {
      console.error("Failed to load subgraph:", err);
    } finally {
      setLoading(false);
    }
  }

  function handleNodeClick(node) {
    setSelectedEntity(node);
  }

  return (
    <div className="flex h-screen w-screen">
      {/* Left sidebar */}
      <Sidebar
        stats={stats}
        selectedEntity={selectedEntity}
        searchResults={searchResults}
        onEntitySelect={handleEntitySelect}
        onShowFullGraph={loadFullGraph}
        viewMode={viewMode}
      />

      {/* Main content area */}
      <div className="flex-1 flex flex-col">
        {/* Top bar */}
        <div className="h-12 border-b border-gray-800 flex items-center px-4 gap-4 bg-gray-900/50">
          <h1 className="text-sm font-semibold text-nodalync-400 tracking-wide">
            NODALYNC STUDIO
          </h1>
          <SearchBar onSearch={handleSearch} />
          {viewMode === "subgraph" && (
            <button
              onClick={loadFullGraph}
              className="text-xs px-3 py-1 bg-gray-800 hover:bg-gray-700 rounded text-gray-300 transition-colors"
            >
              ← Full Graph
            </button>
          )}
          {loading && (
            <span className="text-xs text-gray-500 animate-pulse">
              Loading...
            </span>
          )}
        </div>

        {/* Graph visualization */}
        <div className="flex-1 relative">
          <GraphView
            data={graphData}
            onNodeClick={handleNodeClick}
            selectedEntity={selectedEntity}
          />
        </div>

        {/* Bottom stats bar */}
        <StatsBar stats={stats} graphData={graphData} viewMode={viewMode} />
      </div>
    </div>
  );
}

export default App;
