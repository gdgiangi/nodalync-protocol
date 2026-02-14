import { useState, useCallback, useRef, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

/**
 * Import states for each file in the queue
 */
const STATUS = {
  PENDING: "pending",
  IMPORTING: "importing",
  EXTRACTING: "extracting",
  DONE: "done",
  ERROR: "error",
};

/**
 * File type icons and labels
 */
function fileTypeInfo(name) {
  const ext = (name || "").split(".").pop().toLowerCase();
  const map = {
    md: { icon: "📝", label: "Markdown" },
    txt: { icon: "📄", label: "Text" },
    pdf: { icon: "📕", label: "PDF" },
    json: { icon: "🔧", label: "JSON" },
    csv: { icon: "📊", label: "CSV" },
    html: { icon: "🌐", label: "HTML" },
    htm: { icon: "🌐", label: "HTML" },
    xml: { icon: "📋", label: "XML" },
    doc: { icon: "📘", label: "Word" },
    docx: { icon: "📘", label: "Word" },
    rtf: { icon: "📄", label: "Rich Text" },
  };
  return map[ext] || { icon: "📎", label: ext.toUpperCase() || "File" };
}

function formatBytes(bytes) {
  if (!bytes) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/**
 * KnowledgeImport — Full-screen drop zone overlay + import queue panel
 *
 * Provides two import modes:
 *   1. Drag-and-drop files onto the app window
 *   2. "Import Files" button (opens native file picker via Tauri dialog plugin)
 *
 * Calls add_content IPC for each file, shows progress, then refreshes the graph.
 */
export default function KnowledgeImport({ onImportComplete }) {
  const [dragOver, setDragOver] = useState(false);
  const [queue, setQueue] = useState([]); // { id, name, path, status, result?, error? }
  const [panelOpen, setPanelOpen] = useState(false);
  const [totalImported, setTotalImported] = useState(0);
  const processingRef = useRef(false);
  const queueRef = useRef([]);
  const fileInputRef = useRef(null);

  // Keep ref in sync for async processing
  useEffect(() => {
    queueRef.current = queue;
  }, [queue]);

  // Listen for Tauri drag-drop events (provides full file paths)
  useEffect(() => {
    let unlisten;
    async function setupDragDrop() {
      try {
        const appWindow = getCurrentWebviewWindow();
        unlisten = await appWindow.onDragDropEvent((event) => {
          if (event.payload.type === "over") {
            setDragOver(true);
          } else if (event.payload.type === "drop") {
            setDragOver(false);
            const paths = event.payload.paths || [];
            if (paths.length > 0) {
              addFilesToQueue(paths);
            }
          } else if (event.payload.type === "leave") {
            setDragOver(false);
          }
        });
      } catch (err) {
        // Tauri not available (dev mode in browser) — use HTML5 fallback
        console.warn("Tauri drag-drop not available:", err);
      }
    }
    setupDragDrop();
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  // HTML5 drag-drop fallback (for dev mode without Tauri)
  useEffect(() => {
    function handleDragOver(e) {
      e.preventDefault();
      e.stopPropagation();
      if (e.dataTransfer?.types?.includes("Files")) {
        setDragOver(true);
      }
    }
    function handleDragLeave(e) {
      e.preventDefault();
      e.stopPropagation();
      // Only hide if leaving the window entirely
      if (e.clientX <= 0 || e.clientY <= 0 || e.clientX >= window.innerWidth || e.clientY >= window.innerHeight) {
        setDragOver(false);
      }
    }
    function handleDrop(e) {
      e.preventDefault();
      e.stopPropagation();
      setDragOver(false);
      // HTML5 drops don't give us file paths — show a notice
      // (Tauri's event handler above will catch real drops with paths)
    }
    window.addEventListener("dragover", handleDragOver);
    window.addEventListener("dragleave", handleDragLeave);
    window.addEventListener("drop", handleDrop);
    return () => {
      window.removeEventListener("dragover", handleDragOver);
      window.removeEventListener("dragleave", handleDragLeave);
      window.removeEventListener("drop", handleDrop);
    };
  }, []);

  /**
   * Add file paths to the import queue and start processing
   */
  const addFilesToQueue = useCallback((paths) => {
    const newItems = paths.map((filePath) => {
      const name = filePath.split(/[\\/]/).pop();
      return {
        id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        name,
        path: filePath,
        status: STATUS.PENDING,
        result: null,
        error: null,
      };
    });

    setQueue((prev) => [...prev, ...newItems]);
    setPanelOpen(true);

    // Start processing if not already
    if (!processingRef.current) {
      processQueue([...queueRef.current, ...newItems]);
    }
  }, []);

  /**
   * Open native file picker via Tauri dialog
   */
  const openFilePicker = useCallback(async () => {
    try {
      // Try Tauri dialog plugin first (dynamic import with variable to avoid Rollup static resolution)
      const dialogModule = "@tauri-apps/plugin-dialog";
      const { open } = await import(/* @vite-ignore */ dialogModule);
      const selected = await open({
        multiple: true,
        title: "Import Knowledge",
        filters: [
          {
            name: "Documents",
            extensions: ["md", "txt", "pdf", "json", "csv", "html", "htm", "xml", "doc", "docx", "rtf"],
          },
          { name: "All Files", extensions: ["*"] },
        ],
      });
      if (selected) {
        const paths = Array.isArray(selected) ? selected : [selected];
        if (paths.length > 0) {
          addFilesToQueue(paths);
        }
      }
    } catch (err) {
      console.warn("Dialog plugin not available, using HTML fallback:", err);
      // Fallback to HTML file input
      fileInputRef.current?.click();
    }
  }, [addFilesToQueue]);

  /**
   * HTML file input fallback handler
   * Note: HTML file inputs don't provide full paths in Tauri webview,
   * so we use add_text_content with file contents as fallback
   */
  const handleFileInputChange = useCallback(async (e) => {
    const files = Array.from(e.target.files || []);
    if (files.length === 0) return;

    const items = [];
    for (const file of files) {
      items.push({
        id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        name: file.name,
        path: null, // No path available from HTML input
        file, // Keep File object for text reading
        status: STATUS.PENDING,
        result: null,
        error: null,
      });
    }

    setQueue((prev) => [...prev, ...items]);
    setPanelOpen(true);

    if (!processingRef.current) {
      processQueue([...queueRef.current, ...items]);
    }

    // Reset input
    e.target.value = "";
  }, []);

  /**
   * Process queue items sequentially
   */
  async function processQueue(currentQueue) {
    processingRef.current = true;

    for (let i = 0; i < currentQueue.length; i++) {
      const item = currentQueue[i];
      if (item.status !== STATUS.PENDING) continue;

      // Update status to importing
      setQueue((prev) =>
        prev.map((q) => (q.id === item.id ? { ...q, status: STATUS.IMPORTING } : q))
      );

      try {
        let result;

        if (item.path) {
          // Tauri path available — use add_content IPC
          result = await invoke("add_content", {
            filePath: item.path,
            title: item.name.replace(/\.[^.]+$/, ""), // Strip extension for title
          });
        } else if (item.file) {
          // HTML fallback — read file text and use add_text_content
          const text = await item.file.text();
          result = await invoke("add_text_content", {
            text,
            title: item.name.replace(/\.[^.]+$/, ""),
          });
        }

        // Update to extracting (L1 mentions)
        setQueue((prev) =>
          prev.map((q) => (q.id === item.id ? { ...q, status: STATUS.EXTRACTING, result } : q))
        );

        // Trigger extraction if we got a hash back
        if (result?.hash) {
          try {
            const extraction = await invoke("extract_mentions", {
              contentHash: result.hash,
            });
            result = { ...result, extraction };
          } catch (extractErr) {
            // Extraction failure isn't fatal — content was still imported
            console.warn("Extraction failed:", extractErr);
            result = { ...result, extractionError: String(extractErr) };
          }
        }

        // Done
        setQueue((prev) =>
          prev.map((q) => (q.id === item.id ? { ...q, status: STATUS.DONE, result } : q))
        );
        setTotalImported((prev) => prev + 1);
      } catch (err) {
        console.error(`Import failed for ${item.name}:`, err);
        setQueue((prev) =>
          prev.map((q) =>
            q.id === item.id
              ? { ...q, status: STATUS.ERROR, error: typeof err === "string" ? err : err?.message || "Import failed" }
              : q
          )
        );
      }
    }

    processingRef.current = false;

    // Notify parent to refresh graph
    onImportComplete?.();
  }

  /**
   * Clear completed/errored items from queue
   */
  const clearCompleted = useCallback(() => {
    setQueue((prev) => prev.filter((q) => q.status === STATUS.PENDING || q.status === STATUS.IMPORTING || q.status === STATUS.EXTRACTING));
  }, []);

  /**
   * Retry a failed import
   */
  const retryItem = useCallback((id) => {
    setQueue((prev) => {
      const updated = prev.map((q) =>
        q.id === id ? { ...q, status: STATUS.PENDING, error: null, result: null } : q
      );
      // Re-process
      if (!processingRef.current) {
        processQueue(updated);
      }
      return updated;
    });
  }, []);

  // Expose openFilePicker to parent via window bridge
  useEffect(() => {
    window.__knowledgeImport = { openFilePicker };
    return () => { delete window.__knowledgeImport; };
  }, [openFilePicker]);

  const pendingCount = queue.filter((q) => q.status === STATUS.PENDING || q.status === STATUS.IMPORTING || q.status === STATUS.EXTRACTING).length;
  const doneCount = queue.filter((q) => q.status === STATUS.DONE).length;
  const errorCount = queue.filter((q) => q.status === STATUS.ERROR).length;

  return (
    <>
      {/* Hidden file input for HTML fallback */}
      <input
        ref={fileInputRef}
        type="file"
        multiple
        accept=".md,.txt,.pdf,.json,.csv,.html,.htm,.xml,.doc,.docx,.rtf"
        onChange={handleFileInputChange}
        style={{ display: "none" }}
      />

      {/* Drag-over overlay */}
      {dragOver && (
        <div
          className="fixed inset-0 z-[100] flex items-center justify-center"
          style={{
            background: "rgba(6, 6, 10, 0.85)",
            backdropFilter: "blur(8px)",
            WebkitBackdropFilter: "blur(8px)",
            animation: "fade-in 0.15s ease",
          }}
        >
          <div
            className="flex flex-col items-center gap-4 p-12 rounded-2xl"
            style={{
              border: "2px dashed var(--accent)",
              background: "rgba(92, 124, 250, 0.04)",
              maxWidth: 480,
              animation: "scale-in 0.2s ease",
            }}
          >
            {/* Upload icon */}
            <div
              className="w-20 h-20 rounded-2xl flex items-center justify-center"
              style={{
                background: "var(--accent-dim)",
                border: "1px solid rgba(92, 124, 250, 0.2)",
                animation: "glow-pulse 2s ease-in-out infinite",
              }}
            >
              <svg
                width="36"
                height="36"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.5"
                strokeLinecap="round"
                strokeLinejoin="round"
                style={{ color: "var(--accent)" }}
              >
                <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
                <polyline points="17 8 12 3 7 8" />
                <line x1="12" y1="3" x2="12" y2="15" />
              </svg>
            </div>

            <div className="text-center">
              <p className="text-[15px] font-light mb-1" style={{ color: "var(--text-primary)" }}>
                Drop files to import
              </p>
              <p className="text-[11px]" style={{ color: "var(--text-tertiary)" }}>
                Documents will be processed and added to your knowledge graph
              </p>
            </div>

            {/* Supported formats */}
            <div className="flex flex-wrap justify-center gap-1.5 mt-1">
              {["MD", "TXT", "PDF", "JSON", "CSV", "HTML"].map((ext) => (
                <span
                  key={ext}
                  className="pill pill-neutral"
                  style={{ fontSize: 7, letterSpacing: 1.5 }}
                >
                  {ext}
                </span>
              ))}
            </div>
          </div>
        </div>
      )}

      {/* Import queue panel (slide-out from right) */}
      {panelOpen && queue.length > 0 && (
        <ImportPanel
          queue={queue}
          pendingCount={pendingCount}
          doneCount={doneCount}
          errorCount={errorCount}
          onClose={() => setPanelOpen(false)}
          onClearCompleted={clearCompleted}
          onRetry={retryItem}
          onAddMore={openFilePicker}
        />
      )}
    </>
  );
}

/**
 * Import queue panel — slide-out from right side
 */
function ImportPanel({ queue, pendingCount, doneCount, errorCount, onClose, onClearCompleted, onRetry, onAddMore }) {
  const [animateIn, setAnimateIn] = useState(false);

  useEffect(() => {
    requestAnimationFrame(() => setAnimateIn(true));
  }, []);

  const handleClose = useCallback(() => {
    setAnimateIn(false);
    setTimeout(onClose, 250);
  }, [onClose]);

  return (
    <div
      className="fixed top-0 right-0 h-full z-40 flex flex-col"
      style={{
        width: 360,
        background: "rgba(10, 10, 16, 0.95)",
        backdropFilter: "blur(32px)",
        WebkitBackdropFilter: "blur(32px)",
        borderLeft: "1px solid var(--border-subtle)",
        boxShadow: "-8px 0 40px rgba(0, 0, 0, 0.3)",
        transform: animateIn ? "translateX(0)" : "translateX(100%)",
        opacity: animateIn ? 1 : 0,
        transition: "all 0.3s cubic-bezier(0.4, 0, 0.2, 1)",
      }}
    >
      {/* Header */}
      <div
        className="flex items-center justify-between px-4 py-3 flex-shrink-0"
        style={{ borderBottom: "1px solid var(--border-subtle)" }}
      >
        <div className="flex items-center gap-2.5">
          <div
            className="w-6 h-6 rounded-md flex items-center justify-center"
            style={{
              background: "rgba(74, 222, 128, 0.1)",
              border: "1px solid rgba(74, 222, 128, 0.2)",
            }}
          >
            <svg
              width="12"
              height="12"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              style={{ color: "var(--green)" }}
            >
              <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
              <polyline points="17 8 12 3 7 8" />
              <line x1="12" y1="3" x2="12" y2="15" />
            </svg>
          </div>
          <span className="text-[12px] font-medium" style={{ color: "var(--text-primary)" }}>
            Knowledge Import
          </span>
        </div>

        <div className="flex items-center gap-2">
          {/* Status pills */}
          {pendingCount > 0 && (
            <span className="pill pill-yellow" style={{ fontSize: 7 }}>
              {pendingCount} PROCESSING
            </span>
          )}
          {doneCount > 0 && (
            <span className="pill pill-green" style={{ fontSize: 7 }}>
              {doneCount} DONE
            </span>
          )}
          {errorCount > 0 && (
            <span className="pill pill-red" style={{ fontSize: 7 }}>
              {errorCount} FAILED
            </span>
          )}

          <button
            onClick={handleClose}
            className="w-6 h-6 flex items-center justify-center rounded-md"
            style={{ color: "var(--text-ghost)", transition: "all 0.15s ease" }}
            onMouseEnter={(e) => {
              e.currentTarget.style.background = "var(--bg-hover)";
              e.currentTarget.style.color = "var(--text-secondary)";
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.background = "transparent";
              e.currentTarget.style.color = "var(--text-ghost)";
            }}
          >
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>
      </div>

      {/* Queue items */}
      <div className="flex-1 overflow-y-auto px-3 py-2" style={{ minHeight: 0 }}>
        {queue.map((item, idx) => (
          <ImportQueueItem
            key={item.id}
            item={item}
            onRetry={() => onRetry(item.id)}
            style={{ animationDelay: `${idx * 40}ms` }}
          />
        ))}
      </div>

      {/* Footer actions */}
      <div
        className="flex items-center justify-between px-4 py-2.5 flex-shrink-0"
        style={{ borderTop: "1px solid var(--border-subtle)" }}
      >
        <button onClick={onAddMore} className="btn text-[10px]">
          <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <line x1="12" y1="5" x2="12" y2="19" />
            <line x1="5" y1="12" x2="19" y2="12" />
          </svg>
          Add More
        </button>

        {(doneCount > 0 || errorCount > 0) && (
          <button onClick={onClearCompleted} className="btn text-[10px]" style={{ color: "var(--text-ghost)" }}>
            Clear Finished
          </button>
        )}
      </div>
    </div>
  );
}

/**
 * Individual import queue item
 */
function ImportQueueItem({ item, onRetry, style }) {
  const { icon, label } = fileTypeInfo(item.name);
  const isActive = item.status === STATUS.IMPORTING || item.status === STATUS.EXTRACTING;

  return (
    <div
      className="flex items-start gap-3 px-3 py-2.5 rounded-lg mb-1 animate-slide-up"
      style={{
        background: isActive ? "var(--accent-dim)" : "transparent",
        border: `1px solid ${isActive ? "rgba(92, 124, 250, 0.15)" : "transparent"}`,
        transition: "all 0.25s ease",
        ...style,
      }}
    >
      {/* File type icon */}
      <div className="flex-shrink-0 mt-0.5 text-[16px]">{icon}</div>

      {/* Details */}
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2 mb-0.5">
          <span
            className="text-[11px] truncate"
            style={{ color: "var(--text-primary)", maxWidth: 200 }}
            title={item.name}
          >
            {item.name}
          </span>
          <span className="mono text-[8px] flex-shrink-0" style={{ color: "var(--text-ghost)" }}>
            {label}
          </span>
        </div>

        {/* Status line */}
        {item.status === STATUS.PENDING && (
          <span className="text-[9px]" style={{ color: "var(--text-ghost)" }}>
            Waiting...
          </span>
        )}

        {item.status === STATUS.IMPORTING && (
          <div className="flex items-center gap-1.5">
            <div
              className="w-2.5 h-2.5 rounded-full border-[1.5px]"
              style={{
                borderColor: "rgba(92, 124, 250, 0.15)",
                borderTopColor: "var(--accent)",
                animation: "spin 0.8s linear infinite",
              }}
            />
            <span className="text-[9px]" style={{ color: "var(--accent)" }}>
              Importing content...
            </span>
          </div>
        )}

        {item.status === STATUS.EXTRACTING && (
          <div className="flex items-center gap-1.5">
            <div
              className="w-2.5 h-2.5 rounded-full border-[1.5px]"
              style={{
                borderColor: "rgba(250, 204, 21, 0.15)",
                borderTopColor: "var(--yellow)",
                animation: "spin 0.8s linear infinite",
              }}
            />
            <span className="text-[9px]" style={{ color: "var(--yellow)" }}>
              Extracting mentions...
            </span>
          </div>
        )}

        {item.status === STATUS.DONE && item.result && (
          <div className="flex flex-col gap-0.5">
            <div className="flex items-center gap-2">
              <svg
                width="10"
                height="10"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2.5"
                strokeLinecap="round"
                strokeLinejoin="round"
                style={{ color: "var(--green)" }}
              >
                <polyline points="20 6 9 17 4 12" />
              </svg>
              <span className="text-[9px]" style={{ color: "var(--green)" }}>
                Imported
              </span>
              {item.result.size && (
                <span className="mono text-[8px]" style={{ color: "var(--text-ghost)" }}>
                  {formatBytes(item.result.size)}
                </span>
              )}
            </div>

            {/* Extraction stats */}
            {item.result.extraction && (
              <div className="flex items-center gap-3 mt-0.5">
                <span className="mono text-[8px]" style={{ color: "var(--text-tertiary)" }}>
                  {item.result.extraction.total_mentions || item.result.mentions || 0} mentions
                </span>
                {item.result.extraction.entities_found > 0 && (
                  <span className="mono text-[8px]" style={{ color: "var(--accent)" }}>
                    {item.result.extraction.entities_found} entities
                  </span>
                )}
                {item.result.extraction.new_entities > 0 && (
                  <span className="mono text-[8px]" style={{ color: "var(--green)" }}>
                    +{item.result.extraction.new_entities} new
                  </span>
                )}
              </div>
            )}

            {/* Hash */}
            {item.result.hash && (
              <span
                className="mono text-[7px] mt-0.5 truncate"
                style={{ color: "var(--text-ghost)", maxWidth: 200 }}
                title={item.result.hash}
              >
                {item.result.hash.slice(0, 16)}…
              </span>
            )}
          </div>
        )}

        {item.status === STATUS.ERROR && (
          <div className="flex items-center gap-2">
            <span className="text-[9px]" style={{ color: "var(--red)" }}>
              {item.error || "Import failed"}
            </span>
            <button
              onClick={onRetry}
              className="text-[8px] px-1.5 py-0.5 rounded"
              style={{
                color: "var(--accent)",
                background: "var(--accent-dim)",
                border: "1px solid rgba(92, 124, 250, 0.2)",
                cursor: "pointer",
              }}
            >
              Retry
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
