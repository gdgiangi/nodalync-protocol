import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import GraphView from "./components/graph3d";
import ContentLibrary from "./components/ContentLibrary";
import Sidebar from "./components/Sidebar";
import StatsBar from "./components/StatsBar";
import SearchBar from "./components/SearchBar";
import EntityDetailPanel from "./components/EntityDetailPanel";
import { useTauriEvents } from "./hooks/useTauriEvents";
import CreateContentDialog from "./components/CreateContentDialog";
import CommandPalette from "./components/CommandPalette";
import GraphLegend from "./components/GraphLegend";
import BalanceDashboard from "./components/BalanceDashboard";
import KnowledgeImport from "./components/KnowledgeImport";

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
  const [showBalance, setShowBalance] = useState(false);
  const [currentView, setCurrentView] = useState("library"); // 'library' or 'graph'
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

  // Handle knowledge import completion — refresh graph
  function handleImportComplete() {
    setTimeout(() => {
      loadFullGraph();
      loadStats();
    }, 1000);
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
      case "action:import-files":
        window.__knowledgeImport?.openFilePicker?.();
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
      case "action:balance":
        setShowBalance(true);
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

          {/* View toggle */}
          <div className="flex items-center gap-1 flex-shrink-0">
            <button
              onClick={() => setCurrentView("library")}
              className={`px-3 py-1.5 text-xs rounded-md transition-all duration-150 ${
                currentView === "library" ? "btn-accent" : "btn"
              }`}
            >
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="mr-1.5">
                <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
                <polyline points="14 2 14 8 20 8"/>
                <line x1="16" y1="13" x2="8" y2="13"/>
                <line x1="16" y1="17" x2="8" y2="17"/>
                <polyline points="10 9 9 9 8 9"/>
              </svg>
              Library
            </button>
            <button
              onClick={() => setCurrentView("graph")}
              className={`px-3 py-1.5 text-xs rounded-md transition-all duration-150 ${
                currentView === "graph" ? "btn-accent" : "btn"
              }`}
            >
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="mr-1.5">
                <circle cx="12" cy="12" r="3"/>
                <path d="M12 1v6m0 6v6"/>
                <path d="m21 12-6-3-6 3-6-3"/>
              </svg>
              Graph
            </button>
          </div>

          {/* Import files button */}
          <button
            onClick={() => window.__knowledgeImport?.openFilePicker?.()}
            className="btn text-[10px] flex-shrink-0"
            style={{
              borderColor: "rgba(74, 222, 128, 0.2)",
              background: "rgba(74, 222, 128, 0.05)",
              color: "var(--green)",
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.background = "rgba(74, 222, 128, 0.1)";
              e.currentTarget.style.borderColor = "rgba(74, 222, 128, 0.35)";
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.background = "rgba(74, 222, 128, 0.05)";
              e.currentTarget.style.borderColor = "rgba(74, 222, 128, 0.2)";
            }}
            title="Import files into knowledge graph"
          >
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
              <polyline points="17 8 12 3 7 8" />
              <line x1="12" y1="3" x2="12" y2="15" />
            </svg>
            Import
          </button>

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

          {/* Balance dashboard button */}
          <button
            onClick={() => setShowBalance(true)}
            className="btn text-[10px] flex-shrink-0"
            style={{
              borderColor: "rgba(250, 204, 21, 0.2)",
              background: "rgba(250, 204, 21, 0.05)",
              color: "var(--yellow)",
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.background = "rgba(250, 204, 21, 0.1)";
              e.currentTarget.style.borderColor = "rgba(250, 204, 21, 0.35)";
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.background = "rgba(250, 204, 21, 0.05)";
              e.currentTarget.style.borderColor = "rgba(250, 204, 21, 0.2)";
            }}
            title="Balance & Transactions"
          >
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
              <line x1="12" y1="1" x2="12" y2="23" />
              <path d="M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6" />
            </svg>
            Balance
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

        {/* Main view */}
        <div className="flex-1 relative overflow-hidden">
          {currentView === "library" ? (
            <ContentLibrary
              data={graphData}
              onNodeClick={handleNodeClick}
              loading={loading}
            />
          ) : (
            <>
              <GraphView
                ref={graphRef}
                data={graphData}
                onNodeClick={handleNodeClick}
                onBackgroundClick={handleBackgroundClick}
                selectedEntity={selectedEntity}
              />

              {/* Graph legend */}
              {graphData.nodes.length > 0 && <GraphLegend />}

              {/* Empty state overlay for graph view */}
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
            </>
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

      {/* Balance & Transaction Dashboard */}
      <BalanceDashboard
        isOpen={showBalance}
        onClose={() => setShowBalance(false)}
      />

      {/* Knowledge Import — drag-drop overlay + queue panel */}
      <KnowledgeImport onImportComplete={handleImportComplete} />
    </div>
  );
}

export default App;
