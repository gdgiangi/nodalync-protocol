import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import GraphView from "./components/GraphView";
import Sidebar from "./components/Sidebar";
import StatsBar from "./components/StatsBar";
import SearchBar from "./components/SearchBar";
import EntityDetailPanel from "./components/EntityDetailPanel";
import CreateContentDialog from "./components/CreateContentDialog";

function App() {
  const [graphData, setGraphData] = useState({ nodes: [], links: [] });
  const [stats, setStats] = useState(null);
  const [selectedEntity, setSelectedEntity] = useState(null);
  const [detailEntity, setDetailEntity] = useState(null);
  const [searchResults, setSearchResults] = useState([]);
  const [loading, setLoading] = useState(true);
  const [viewMode, setViewMode] = useState("full"); // 'full' or 'subgraph'
  const [showCreateDialog, setShowCreateDialog] = useState(false);

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
      setSelectedEntity(null);
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

  const handleSearch = useCallback(async (query) => {
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
  }, []);

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
    // Open the detail panel
    setDetailEntity(node);
  }

  function handleDetailClose() {
    setDetailEntity(null);
  }

  function handleDetailEntitySelect(entityId) {
    // Navigate to a different entity from within the detail panel
    // First find it in the current graph data
    const node = graphData.nodes.find((n) => n.id === entityId);
    if (node) {
      setSelectedEntity(node);
      setDetailEntity(node);
    } else {
      // Entity not in current view — load its subgraph
      handleEntitySelect(entityId);
    }
  }

  function handleFocusEntity(entityId) {
    handleEntitySelect(entityId);
  }

  // Handle content creation success — refresh graph
  function handleContentCreated(result) {
    // Reload graph data after a short delay for processing
    setTimeout(() => {
      loadFullGraph();
      loadStats();
    }, 2000);
  }

  // Keyboard shortcuts
  useEffect(() => {
    function handleKeyDown(e) {
      // Escape closes detail panel or create dialog
      if (e.key === "Escape" && detailEntity) {
        setDetailEntity(null);
        e.preventDefault();
      }
      // Ctrl+N opens create dialog
      if ((e.ctrlKey || e.metaKey) && e.key === "n" && !showCreateDialog) {
        setShowCreateDialog(true);
        e.preventDefault();
      }
      // / focuses search
      if (e.key === "/" && !e.ctrlKey && !e.metaKey && !detailEntity && !showCreateDialog) {
        const searchInput = document.querySelector('input[placeholder="Search entities..."]');
        if (searchInput && document.activeElement !== searchInput) {
          searchInput.focus();
          e.preventDefault();
        }
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [detailEntity, showCreateDialog]);

  return (
    <div className="flex h-screen w-screen" style={{ background: 'var(--bg-deep)' }}>
      {/* Sidebar */}
      <Sidebar
        stats={stats}
        selectedEntity={selectedEntity}
        searchResults={searchResults}
        onEntitySelect={handleEntitySelect}
        onShowFullGraph={loadFullGraph}
        viewMode={viewMode}
      />

      {/* Main content */}
      <div className="flex-1 flex flex-col min-w-0">
        {/* Top bar */}
        <div
          className="h-11 flex items-center px-4 gap-4 flex-shrink-0"
          style={{
            borderBottom: '1px solid var(--border-subtle)',
            background: 'rgba(6, 6, 10, 0.6)',
            backdropFilter: 'blur(12px)',
            WebkitBackdropFilter: 'blur(12px)',
          }}
        >
          <h1
            className="label-sm flex-shrink-0"
            style={{ color: 'var(--accent)', letterSpacing: '3px' }}
          >
            NODALYNC STUDIO
          </h1>

          <SearchBar onSearch={handleSearch} />

          {/* Create content button */}
          <button
            onClick={() => setShowCreateDialog(true)}
            className="btn btn-accent text-[10px] flex-shrink-0"
            title="Create new content (Ctrl+N)"
          >
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <line x1="12" y1="5" x2="12" y2="19" />
              <line x1="5" y1="12" x2="19" y2="12" />
            </svg>
            New
          </button>

          {viewMode === "subgraph" && (
            <button onClick={loadFullGraph} className="btn text-[10px] flex-shrink-0">
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <polyline points="15 18 9 12 15 6" />
              </svg>
              Full Graph
            </button>
          )}

          {loading && (
            <div className="flex items-center gap-2 flex-shrink-0">
              <div
                className="w-1.5 h-1.5 rounded-full"
                style={{
                  background: 'var(--accent)',
                  animation: 'pulse-subtle 1.2s ease-in-out infinite',
                }}
              />
              <span className="label-xs" style={{ color: 'var(--text-ghost)' }}>
                LOADING
              </span>
            </div>
          )}
        </div>

        {/* Graph visualization */}
        <div className="flex-1 relative overflow-hidden">
          <GraphView
            data={graphData}
            onNodeClick={handleNodeClick}
            selectedEntity={selectedEntity}
          />

          {/* Empty state overlay */}
          {!loading && graphData.nodes.length === 0 && (
            <div className="absolute inset-0 flex items-center justify-center">
              <div className="text-center animate-fade-in">
                <div
                  className="w-16 h-16 mx-auto mb-4 rounded-full flex items-center justify-center"
                  style={{
                    background: 'var(--bg-elevated)',
                    border: '1px solid var(--border-subtle)',
                  }}
                >
                  <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" style={{ color: 'var(--text-ghost)' }}>
                    <circle cx="12" cy="12" r="10" />
                    <line x1="12" y1="8" x2="12" y2="12" />
                    <line x1="12" y1="16" x2="12.01" y2="16" />
                  </svg>
                </div>
                <p className="text-[13px] mb-1" style={{ color: 'var(--text-tertiary)' }}>
                  No knowledge yet
                </p>
                <p className="text-[11px] mb-3" style={{ color: 'var(--text-ghost)' }}>
                  Add content to start building your graph
                </p>
                <button
                  onClick={() => setShowCreateDialog(true)}
                  className="btn btn-accent text-[11px]"
                >
                  <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <line x1="12" y1="5" x2="12" y2="19" />
                    <line x1="5" y1="12" x2="19" y2="12" />
                  </svg>
                  Create your first note
                </button>
              </div>
            </div>
          )}
        </div>

        {/* Bottom stats bar */}
        <StatsBar stats={stats} graphData={graphData} viewMode={viewMode} />
      </div>

      {/* Entity Detail Panel — slide-out */}
      {detailEntity && (
        <EntityDetailPanel
          entity={detailEntity}
          onClose={handleDetailClose}
          onEntitySelect={handleDetailEntitySelect}
          onFocusEntity={handleFocusEntity}
        />
      )}

      {/* Create Content Dialog */}
      <CreateContentDialog
        isOpen={showCreateDialog}
        onClose={() => setShowCreateDialog(false)}
        onCreated={handleContentCreated}
      />
    </div>
  );
}

export default App;
