import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import GraphView from "./components/GraphView";
import Sidebar from "./components/Sidebar";
import StatsBar from "./components/StatsBar";
import SearchBar from "./components/SearchBar";
import EntityDetailPanel from "./components/EntityDetailPanel";
import { useTauriEvents } from "./hooks/useTauriEvents";
import CreateContentDialog from "./components/CreateContentDialog";
import CommandPalette from "./components/CommandPalette";

function App() {
  const [graphData, setGraphData] = useState({ nodes: [], links: [] });
  const [stats, setStats] = useState(null);
  const [selectedEntity, setSelectedEntity] = useState(null);
  const [detailEntity, setDetailEntity] = useState(null);
  const [searchResults, setSearchResults] = useState([]);
  const [loading, setLoading] = useState(true);
  const [viewMode, setViewMode] = useState("full"); // 'full' or 'subgraph'
  const [showCreateDialog, setShowCreateDialog] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const graphRef = useRef(null);

  // Load full graph on mount
  useEffect(() => {
    loadFullGraph();
    loadStats();
  }, []);

  // Listen for backend events — auto-refresh graph when content is processed
  useTauriEvents({
    "graph:updated": () => {
      loadFullGraph();
      loadStats();
    },
    "l2:complete": () => {
      loadStats();
    },
  });

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
    setDetailEntity(node);
    // Zoom camera to center the clicked entity
    if (graphRef.current) {
      graphRef.current.zoomToEntity(node.id);
    }
  }

  function handleBackgroundClick() {
    // Click away → deselect and return to full view
    if (selectedEntity) {
      setSelectedEntity(null);
      setDetailEntity(null);
      if (graphRef.current) {
        graphRef.current.resetZoom();
      }
    }
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

  // Command palette action handler
  function handlePaletteAction(actionId) {
    switch (actionId) {
      case "action:full-graph":
        loadFullGraph();
        break;
      case "action:new-content":
        setShowCreateDialog(true);
        break;
      case "action:toggle-sidebar":
        // Sidebar toggle is internal to Sidebar — emit event
        break;
      case "action:zoom-fit":
        // TODO: Reset zoom on graph
        break;
      case "action:review-queue":
        // TODO: Open review queue panel
        break;
      case "action:settings":
        // TODO: Open settings panel
        break;
    }
  }

  // Keyboard shortcuts
  useEffect(() => {
    function handleKeyDown(e) {
      // Ctrl+K / Cmd+K → command palette
      if ((e.ctrlKey || e.metaKey) && e.key === "k") {
        e.preventDefault();
        setPaletteOpen((prev) => !prev);
        return;
      }
      // Escape closes detail panel or create dialog (when palette is not open)
      if (e.key === "Escape" && detailEntity && !paletteOpen) {
        setDetailEntity(null);
        e.preventDefault();
      }
      // Ctrl+N opens create dialog
      if ((e.ctrlKey || e.metaKey) && e.key === "n" && !showCreateDialog && !paletteOpen) {
        setShowCreateDialog(true);
        e.preventDefault();
      }
      // / focuses search (when palette is not open)
      if (e.key === "/" && !e.ctrlKey && !e.metaKey && !detailEntity && !paletteOpen && !showCreateDialog) {
        const searchInput = document.querySelector('input[placeholder="Search entities..."]');
        if (searchInput && document.activeElement !== searchInput) {
          searchInput.focus();
          e.preventDefault();
        }
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [detailEntity, paletteOpen, showCreateDialog]);

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

          {/* Command palette trigger */}
          <button
            onClick={() => setPaletteOpen(true)}
            className="flex items-center gap-2 px-2.5 py-1 rounded-md flex-shrink-0 transition-all duration-150"
            style={{
              background: 'rgba(255, 255, 255, 0.02)',
              border: '1px solid var(--border-subtle)',
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.background = 'var(--bg-hover)';
              e.currentTarget.style.borderColor = 'var(--border-hover)';
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.background = 'rgba(255, 255, 255, 0.02)';
              e.currentTarget.style.borderColor = 'var(--border-subtle)';
            }}
            title="Command Palette (Ctrl+K)"
          >
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" style={{ color: 'var(--text-ghost)' }}>
              <path d="M18 3a3 3 0 0 0-3 3v12a3 3 0 0 0 3 3 3 3 0 0 0 3-3 3 3 0 0 0-3-3H6a3 3 0 0 0-3 3 3 3 0 0 0 3 3 3 3 0 0 0 3-3V6a3 3 0 0 0-3-3 3 3 0 0 0-3 3 3 3 0 0 0 3 3h12a3 3 0 0 0 3-3 3 3 0 0 0-3-3z" />
            </svg>
            <div className="flex items-center gap-0.5">
              <kbd className="mono text-[8px] px-1 py-px rounded" style={{
                color: 'var(--text-ghost)',
                background: 'rgba(255, 255, 255, 0.03)',
                border: '1px solid rgba(255, 255, 255, 0.06)',
              }}>Ctrl</kbd>
              <kbd className="mono text-[8px] px-1 py-px rounded" style={{
                color: 'var(--text-ghost)',
                background: 'rgba(255, 255, 255, 0.03)',
                border: '1px solid rgba(255, 255, 255, 0.06)',
              }}>K</kbd>
            </div>
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
            ref={graphRef}
            data={graphData}
            onNodeClick={handleNodeClick}
            onBackgroundClick={handleBackgroundClick}
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

      {/* Command Palette */}
      <CommandPalette
        isOpen={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        onAction={handlePaletteAction}
        onEntitySelect={(entityId) => {
          handleEntitySelect(entityId);
        }}
      />
    </div>
  );
}

export default App;
