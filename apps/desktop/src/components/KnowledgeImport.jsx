import { useCallback, useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { open } from "@tauri-apps/plugin-dialog";
import "./studio-content.css";

const ACTIVE = new Set(["queued", "saving", "indexing"]);
const SUPPORTED = /\.(txt|md)$/i;

function message(error) {
  return typeof error === "string" ? error : error?.message || "Import failed. Please try again.";
}

function ImportIcon({ size = 20 }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true">
      <path d="M4 15v4a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-4M12 3v12m-5-7 5-5 5 5" />
    </svg>
  );
}

export default function KnowledgeImport({ onImportComplete }) {
  const [queue, setQueue] = useState([]);
  const [panelOpen, setPanelOpen] = useState(false);
  const [dragOver, setDragOver] = useState(false);
  const [pickerError, setPickerError] = useState("");
  const queueRef = useRef([]);
  const processingRef = useRef(false);
  const mountedRef = useRef(true);
  const callbackRef = useRef(onImportComplete);
  const fileInputRef = useRef(null);
  const panelRef = useRef(null);
  const panelReturnFocusRef = useRef(null);
  const nextIdRef = useRef(0);
  const panelVisible = panelOpen && queue.length > 0;

  useEffect(() => { callbackRef.current = onImportComplete; }, [onImportComplete]);
  useEffect(() => {
    mountedRef.current = true;
    return () => { mountedRef.current = false; };
  }, []);

  // The ref is updated synchronously so additions during an in-flight import
  // are visible to the worker without waiting for a React render.
  const updateQueue = useCallback((update) => {
    queueRef.current = update(queueRef.current);
    if (mountedRef.current) setQueue(queueRef.current);
  }, []);

  const updateItem = useCallback((id, fields) => {
    updateQueue((current) => current.map((item) => item.id === id ? { ...item, ...fields } : item));
  }, [updateQueue]);

  const drainQueue = useCallback(async () => {
    if (processingRef.current) return;
    processingRef.current = true;
    let changed = false;
    try {
      while (mountedRef.current) {
        const item = queueRef.current.find((candidate) => candidate.status === "queued");
        if (!item) break;
        let result = item.result;
        try {
          if (!result) {
            updateItem(item.id, { status: "saving" });
            if (!isTauri()) throw new Error("Open the desktop app to save files to your local node.");
            if (item.path) {
              result = await invoke("add_content", {
                filePath: item.path,
                title: item.name.replace(/\.[^.]+$/, ""),
                description: null,
              });
            } else {
              // Decode strictly: never convert binary or invalid UTF-8 into a note.
              const bytes = await item.file.arrayBuffer();
              let text;
              try {
                text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
              } catch {
                throw new Error("This file is not UTF-8 text. Import a .txt or .md text file.");
              }
              result = await invoke("add_text_content", {
                text,
                title: item.name.replace(/\.[^.]+$/, ""),
                description: null,
              });
            }
            changed = true;
          }

          updateItem(item.id, { status: "indexing", result });
          try {
            const extraction = await invoke("extract_mentions", { contentHash: result.hash });
            updateItem(item.id, { status: "done", result: { ...result, extraction }, error: null });
          } catch (indexError) {
            updateItem(item.id, {
              status: "warning",
              result,
              error: "The file is saved, but graph indexing failed. " + message(indexError),
            });
          }
          changed = true;
        } catch (importError) {
          updateItem(item.id, { status: "error", error: message(importError) });
        }
      }
    } finally {
      processingRef.current = false;
      if (changed && mountedRef.current) callbackRef.current?.();
    }
  }, [updateItem]);

  const enqueue = useCallback((files) => {
    const additions = files.map((file) => {
      const path = typeof file === "string" ? file : null;
      const name = path ? path.split(/[\\/]/).pop() : file.name;
      const supported = SUPPORTED.test(name);
      return {
        id: ++nextIdRef.current,
        name,
        path,
        file: path ? null : file,
        status: supported ? "queued" : "error",
        result: null,
        retryable: supported,
        error: supported ? null : "This format is not supported yet. Choose a plain-text (.txt) or Markdown (.md) file.",
      };
    });
    updateQueue((current) => [...current, ...additions]);
    setPickerError("");
    setPanelOpen(true);
    void drainQueue();
  }, [drainQueue, updateQueue]);

  const openFilePicker = useCallback(async () => {
    setPickerError("");
    if (!isTauri()) {
      fileInputRef.current?.click();
      return;
    }
    try {
      const selected = await open({
        multiple: true,
        directory: false,
        title: "Import text and Markdown",
        filters: [{ name: "Text and Markdown", extensions: ["txt", "md"] }],
      });
      if (selected) enqueue(Array.isArray(selected) ? selected : [selected]);
    } catch (error) {
      setPickerError("The file picker could not open. " + message(error));
    }
  }, [enqueue]);

  useEffect(() => {
    const bridge = { openFilePicker, openQueue: () => setPanelOpen(true) };
    window.__knowledgeImport = bridge;
    return () => {
      if (window.__knowledgeImport === bridge) delete window.__knowledgeImport;
    };
  }, [openFilePicker]);

  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let unlisten;
    getCurrentWebviewWindow().onDragDropEvent((event) => {
      if (disposed) return;
      const type = event.payload.type;
      if (type === "enter" || type === "over") setDragOver(true);
      if (type === "leave") setDragOver(false);
      if (type === "drop") {
        setDragOver(false);
        if (event.payload.paths?.length) enqueue(event.payload.paths);
      }
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    }).catch(() => {
      // The native picker remains available when file-drop events are unavailable.
    });
    return () => { disposed = true; unlisten?.(); };
  }, [enqueue]);

  useEffect(() => {
    function drag(event) {
      if (!Array.from(event.dataTransfer?.types || []).includes("Files")) return;
      event.preventDefault();
      if (!isTauri()) setDragOver(true);
    }
    function leave(event) {
      if (!event.relatedTarget) setDragOver(false);
    }
    function drop(event) {
      if (!Array.from(event.dataTransfer?.types || []).includes("Files")) return;
      event.preventDefault();
      setDragOver(false);
      // Native drops are handled once through Tauri with their original paths.
      if (!isTauri() && event.dataTransfer.files.length) enqueue(Array.from(event.dataTransfer.files));
    }
    window.addEventListener("dragover", drag);
    window.addEventListener("dragleave", leave);
    window.addEventListener("drop", drop);
    return () => {
      window.removeEventListener("dragover", drag);
      window.removeEventListener("dragleave", leave);
      window.removeEventListener("drop", drop);
    };
  }, [enqueue]);

  useEffect(() => {
    if (!panelVisible) return;
    panelReturnFocusRef.current = document.activeElement;
    panelRef.current?.focus();
    function escape(event) {
      if (event.key === "Escape" && !document.querySelector("dialog[open]")) {
        setPanelOpen(false);
      }
    }
    window.addEventListener("keydown", escape);
    return () => {
      window.removeEventListener("keydown", escape);
      panelReturnFocusRef.current?.focus?.();
    };
  }, [panelVisible]);

  function retry(item) {
    if (!item.retryable || ACTIVE.has(item.status)) return;
    // A saved file retries graph indexing only, avoiding duplicate-content errors.
    updateItem(item.id, { status: "queued", error: null });
    void drainQueue();
  }

  const pending = queue.filter((item) => ACTIVE.has(item.status)).length;
  const completed = queue.filter((item) => item.status === "done").length;
  const attention = queue.filter((item) => item.status === "error" || item.status === "warning").length;

  return (
    <>
      <input
        ref={fileInputRef}
        type="file"
        accept=".txt,.md,text/plain,text/markdown"
        multiple
        hidden
        aria-label="Choose text or Markdown files"
        onChange={(event) => {
          const files = Array.from(event.target.files || []);
          if (files.length) enqueue(files);
          event.target.value = "";
        }}
      />

      {pickerError && (
        <div className="sc-import-notice" role="alert">
          <span>{pickerError}</span>
          <button className="sc-icon-button" aria-label="Dismiss file picker error" onClick={() => setPickerError("")}>×</button>
        </div>
      )}

      {dragOver && (
        <div className="sc-drop-overlay">
          <div className="sc-drop-target">
            <ImportIcon size={34} />
            <h2>Bring your knowledge in</h2>
            <p>Drop plain-text or Markdown files to save them privately.</p>
            <span className="sc-format-label">.txt · .md</span>
          </div>
        </div>
      )}

      {!panelOpen && queue.length > 0 && (
        <button className="sc-import-reopen" onClick={() => setPanelOpen(true)}>
          <ImportIcon size={16} />
          {pending ? "Importing " + pending + " " + (pending === 1 ? "file" : "files") : attention ? "Imports need attention" : "View imports"}
        </button>
      )}

      {panelVisible && (
        <aside className="sc-import-panel" ref={panelRef} tabIndex={-1} aria-label="File imports">
          <header className="sc-import-header">
            <div><span className="sc-eyebrow">Your originals</span><h2>File imports</h2></div>
            <button className="sc-icon-button" aria-label="Close import panel" onClick={() => setPanelOpen(false)}>
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true"><path d="m6 6 12 12M18 6 6 18" /></svg>
            </button>
          </header>
          <p className="sc-import-intro">Text and Markdown, saved privately on this node.</p>
          <div className="sc-import-summary" role="status" aria-live="polite">
            <span>{completed} imported</span>
            {pending > 0 && <span>{pending} in progress</span>}
            {attention > 0 && <span className="sc-warning-text">{attention} need attention</span>}
          </div>
          <ul className="sc-import-list">
            {queue.map((item) => (
              <li key={item.id} className="sc-import-item">
                <span className="sc-import-file-icon"><ImportIcon size={18} /></span>
                <div className="sc-import-item-body">
                  <h3 title={item.name}>{item.name}</h3>
                  {ACTIVE.has(item.status) && <p className="sc-import-progress">
                    <span className="sc-spinner" />
                    {item.status === "queued" ? "Waiting to import…" : item.status === "saving" ? "Saving original…" : "Connecting to your graph…"}
                  </p>}
                  {item.status === "done" && (
                    <>
                      <p className="sc-success-text">Imported to your library</p>
                      <p className="sc-import-metadata">
                        {item.result.extraction?.mention_count ?? item.result.mentions ?? 0} mentions · {item.result.extraction?.entities?.length || 0} entities
                        {(item.result.extraction?.entities || []).some((entity) => !entity.existing) && " · " + item.result.extraction.entities.filter((entity) => !entity.existing).length + " new"}
                      </p>
                    </>
                  )}
                  {(item.status === "error" || item.status === "warning") && (
                    <>
                      <p className={item.status === "warning" ? "sc-warning-text" : "sc-error-text"}>{item.error}</p>
                      {item.retryable && <button className="sc-text-button" onClick={() => retry(item)}>{item.result ? "Retry indexing" : "Retry import"}</button>}
                    </>
                  )}
                </div>
              </li>
            ))}
          </ul>
          <footer className="sc-import-footer">
            <button className="sc-button sc-button-primary" onClick={openFilePicker}>Add files</button>
            {queue.some((item) => !ACTIVE.has(item.status)) && <button className="sc-button" onClick={() => updateQueue((current) => current.filter((item) => ACTIVE.has(item.status)))}>Clear finished</button>}
          </footer>
        </aside>
      )}
    </>
  );
}
