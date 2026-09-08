import { useCallback, useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import SynthesisWorkspace from "./components/SynthesisWorkspace";
import ContentLibrary from "./components/ContentLibrary";
import CreateContentDialog from "./components/CreateContentDialog";
import KnowledgeImport from "./components/KnowledgeImport";
import BalanceDashboard from "./components/BalanceDashboard";
import RelationshipExplorer from "./components/RelationshipExplorer";
import { useTauriEvents } from "./hooks/useTauriEvents";

const errorMessage = (error) => {
  const message = typeof error === "string" ? error : error?.message || "Something went wrong. Please try again.";
  return /decryption failed|password does not match/i.test(message)
    ? "That password doesn’t unlock this workspace. Check your password and try again."
    : message;
};
const shortId = (id) => id ? `${id.slice(0, 9)}…${id.slice(-6)}` : "Not connected";
const paths = {
  synthesis: <><rect x="3" y="4" width="7" height="7" rx="1"/><rect x="14" y="13" width="7" height="7" rx="1"/><path d="M14 5h6m-3-3v6M4 16h6m-3-3v6m2-6 6 6"/></>,
  library: <><rect x="4" y="3" width="6" height="18" rx="1"/><path d="M14 3h5v18h-5zM7 7v4M16.5 7v4"/></>,
  graph: <><circle cx="6" cy="6" r="3"/><circle cx="18" cy="7" r="3"/><circle cx="12" cy="19" r="3"/><path d="m9 6 6 1M7 9l4 7m6-6-4 6"/></>,
  search: <><circle cx="10.5" cy="10.5" r="6.5"/><path d="m16 16 5 5"/></>,
  plus: <path d="M12 5v14M5 12h14"/>,
  import: <><path d="M12 16V3m-5 5 5-5 5 5M4 15v5a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-5"/></>,
  node: <><rect x="4" y="3" width="16" height="7" rx="2"/><rect x="4" y="14" width="16" height="7" rx="2"/><path d="M8 6.5h.01M8 17.5h.01M12 6.5h4M12 17.5h4"/></>,
  activity: <path d="M3 12h4l3-8 4 16 3-8h4"/>,
  arrow: <path d="M4 12h16m-6-6 6 6-6 6"/>,
  close: <path d="m6 6 12 12M6 18 18 6"/>,
  check: <path d="m5 12 4 4L19 6"/>,
  lock: <><rect x="5" y="10" width="14" height="11" rx="2"/><path d="M8 10V6a4 4 0 0 1 8 0v4m-4 5v2"/></>,
  refresh: <><path d="M20 7v5h-5M4 17v-5h5"/><path d="M6 7a7 7 0 0 1 12-1l2 6M4 12l2 6a7 7 0 0 0 12-1"/></>,
  file: <><path d="M14 3H5v18h14V8zM14 3v5h5M8 13h8m-8 4h6"/></>,
  copy: <><rect x="8" y="8" width="12" height="13" rx="2"/><path d="M15 8V3H3v13h5"/></>,
};
function Icon({ name, size = 18 }) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{paths[name] || paths.file}</svg>;
}
function Brand() {
  return <div className="studio-brand"><span className="studio-mark"><Icon name="graph" size={22}/></span><span>nodalync<small>STUDIO</small></span></div>;
}
function Setup({ mode, onReady, onRetry, startupError }) {
  const [name, setName] = useState("");
  const [password, setPassword] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(null);
  const submitting = useRef(false);
  const creating = mode === "create";
  async function submit(event) {
    event.preventDefault();
    if (submitting.current) return;
    if (creating && password !== confirmation) { setError("The passwords don’t match."); return; }
    submitting.current = true; setBusy(true); setError(null);
    try {
      const identity = await invoke(creating ? "init_node" : "unlock_node", creating ? { password, name: name.trim() || "My workspace" } : { password });
      setPassword(""); setConfirmation(""); onReady(identity);
    } catch (err) { setError(errorMessage(err)); }
    finally { submitting.current = false; setBusy(false); }
  }
  return <div className="studio-setup">
    <header><Brand/><span className="studio-badge"><Icon name="lock" size={13}/> Local by default</span></header>
    <main className="studio-welcome">
      <section className="studio-welcome-story"><span className="studio-eyebrow">YOUR PERSONAL KNOWLEDGE SPACE</span><h1>A place for<br/>your next idea.</h1><p>Bring your sources together. Make something new.<br/>Keep the origins that make it yours.</p>
        <div className="studio-story-steps">{[["file", "Collect", "Notes and sources, in one place."],["synthesis", "Synthesize", "Work with your sources to develop new ideas."],["lock", "Keep control", "Your identity. Your local workspace."]].map(([icon, title, description]) => <div key={title}><span><Icon name={icon}/></span><div><strong>{title}</strong><p>{description}</p></div></div>)}</div>
        <span className="studio-welcome-foot">NODALYNC PROTOCOL <span> / </span> KNOWLEDGE WITH PROVENANCE</span>
      </section>
      <section className="studio-setup-card">
        {mode === "loading" ? <><span className="studio-spinner"/><h2>Opening your workspace</h2><p>Checking the local node…</p></> : mode === "browser" ? <><span className="studio-card-icon"><Icon name="node" size={25}/></span><h2>Open the desktop app</h2><p>Studio uses a local node to store your identity and knowledge. Open Nodalync Studio on this computer to create or unlock your workspace.</p><div className="studio-notice">This browser view shows the interface. Your content and node are available in the native desktop window.</div></> : mode === "error" ? <><h2>We couldn’t open Studio</h2><p role="alert">{startupError}</p><button className="studio-button primary" onClick={onRetry}>Try again</button></> : <>
          <span className="studio-card-icon"><Icon name={creating ? "plus" : "lock"} size={25}/></span><h2>{creating ? "Make this space yours" : "Welcome back"}</h2><p>{creating ? "Create a local identity to start collecting knowledge." : "Unlock your identity to pick up where you left off."}</p>
          <form onSubmit={submit} className="studio-form">
            {creating && <label>Workspace name<input autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="e.g. Gabriel’s workspace" maxLength={80}/></label>}
            <label>Password<input autoFocus={!creating} type="password" autoComplete={creating ? "new-password" : "current-password"} value={password} onChange={(e) => setPassword(e.target.value)} minLength={creating ? 8 : undefined} required placeholder={creating ? "At least 8 characters" : "Your workspace password"}/></label>
            {creating && <label>Confirm password<input type="password" autoComplete="new-password" value={confirmation} onChange={(e) => setConfirmation(e.target.value)} required placeholder="Enter your password again"/></label>}
            {error && <p className="studio-error" role="alert">{error}</p>}
            <button className="studio-button primary" type="submit" disabled={busy}>{busy ? "Opening workspace…" : creating ? "Create workspace" : "Unlock workspace"}<Icon name="arrow"/></button>
          </form>
          <p className="studio-form-note"><Icon name="lock" size={13}/>{creating ? "This password encrypts your local identity. Keep it somewhere safe; there is no password reset." : "Your password is used locally to unlock your identity."}</p>
        </>}
      </section>
    </main>
  </div>;
}

function QuickFind({ isOpen, onClose, items, onContent, onEntity, onAction }) {
  const ref = useRef(null);
  const input = useRef(null);
  const resultList = useRef(null);
  const [query, setQuery] = useState("");
  const [entities, setEntities] = useState([]);
  const [sources, setSources] = useState([]);
  const [error, setError] = useState(null);
  const [searching, setSearching] = useState(false);
  const [activeIndex, setActiveIndex] = useState(0);
  useEffect(() => {
    if (isOpen) {
      setQuery(""); setEntities([]); setSources([]); setActiveIndex(0);
      if (!ref.current?.open) ref.current?.showModal();
      input.current?.focus();
    } else ref.current?.close();
  }, [isOpen]);
  useEffect(() => {
    let cancelled = false;
    setError(null);
    if (!isOpen || !query.trim()) { setEntities([]); setSources([]); setSearching(false); return; }
    setSearching(true);
    const timer = setTimeout(async () => {
      const results = await Promise.allSettled([
        invoke("list_synthesis_sources", { query: query.trim(), offset: 0, limit: 6 }),
        invoke("search_entities", { query: query.trim(), limit: 6 }),
      ]);
      if (cancelled) return;
      setSources(results[0].status === "fulfilled" ? results[0].value.items : []);
      setEntities(results[1].status === "fulfilled" ? results[1].value : []);
      const failed = results.filter((result) => result.status === "rejected");
      if (failed.length) setError(failed.map((result) => errorMessage(result.reason)).join(" "));
      setSearching(false);
    }, 160);
    return () => { cancelled = true; clearTimeout(timer); };
  }, [query, isOpen]);

  const term = query.trim().toLowerCase();
  const actions = [["synthesis", "Open synthesis worktable", "synthesis"], ["new", "New note", "plus"], ["import", "Import files", "import"], ["library", "Open library", "library"], ["graph", "Explore relationships", "graph"], ["node", "Node settings", "node"]];
  const matchingItems = term ? [...new Map([
    ...items.filter((item) => [item.title, item.hash].some((value) => String(value || "").toLowerCase().includes(term))),
    ...sources.filter((item) => item.available_locally),
  ].map((item) => [item.hash, item])).values()].slice(0, 6) : [];
  const results = [
    ...actions.filter(([, label]) => !term || label.toLowerCase().includes(term))
      .map(([id, label, icon]) => ({ key: "action:" + id, label, icon, detail: "Quick action", run: () => onAction(id) })),
    ...matchingItems.map((item) => ({ key: "content:" + item.hash, label: item.title || "Untitled", icon: "file", detail: "Note or source", run: () => onContent(item) })),
    ...(term ? entities.map((entity) => ({ key: "entity:" + entity.id, label: entity.label, icon: "graph", detail: entity.entity_type, run: () => onEntity(entity.id) })) : []),
  ];
  const selectedIndex = Math.min(activeIndex, Math.max(results.length - 1, 0));
  useEffect(() => {
    resultList.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex, results.length]);

  function execute(result) {
    if (!result) return;
    onClose();
    result.run();
  }
  function navigate(event) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const direction = event.key === "ArrowDown" ? 1 : -1;
      setActiveIndex(Math.max(0, Math.min(selectedIndex + direction, results.length - 1)));
    } else if (event.key === "Enter") {
      event.preventDefault();
      execute(results[selectedIndex]);
    }
  }

  return <dialog ref={ref} className="studio-quickfind" aria-label="Find in workspace" onCancel={(event) => { event.preventDefault(); onClose(); }} onClick={(event) => { if (event.target === ref.current) onClose(); }}>
    <div className="studio-quickfind-input"><Icon name="search"/><input
      ref={input} role="combobox" aria-label="Find notes, entities, and actions" aria-autocomplete="list"
      aria-expanded={isOpen} aria-controls="studio-find-results" aria-activedescendant={results.length ? "studio-find-option-" + selectedIndex : undefined}
      placeholder="Find a note, entity, or action…" value={query} onKeyDown={navigate}
      onChange={(event) => { setQuery(event.target.value); setActiveIndex(0); setEntities([]); setSources([]); }}
    /><button className="studio-icon-button" aria-label="Close search" onClick={onClose}><Icon name="close"/></button></div>
    <div ref={resultList} id="studio-find-results" className="studio-quickfind-results" role="listbox" aria-label="Search results" aria-busy={searching}>
      <span className="studio-eyebrow" role="presentation">{term ? "SEARCH RESULTS" : "QUICK ACTIONS"}</span>
      {results.map((result, index) => <button
        key={result.key} id={"studio-find-option-" + index} type="button" role="option" tabIndex={-1}
        aria-selected={index === selectedIndex} style={index === selectedIndex ? { background: "#2b4131" } : undefined}
        onMouseDown={(event) => event.preventDefault()} onClick={() => execute(result)}
      ><Icon name={result.icon}/><span>{result.label}<small>{result.detail}</small></span><Icon name="arrow" size={15}/></button>)}
      {!results.length && <p className="studio-muted" role="status">{searching ? "Searching your workspace…" : "No matches. Try another word."}</p>}
      {error && <p className="studio-error" role="alert">{error}</p>}
    </div><footer>↑ ↓ to choose <span aria-hidden="true"> · </span> Enter to open <span aria-hidden="true"> · </span> Esc to close</footer>
  </dialog>;
}

function ContentReader({ item, onClose }) {
  const [navigation, setNavigation] = useState({ originHash: item.hash, items: [] });
  const [documentState, setDocumentState] = useState(null);
  const [sourceError, setSourceError] = useState(null);
  const [openingSource, setOpeningSource] = useState(null);
  const sourceRequest = useRef(0);
  const ref = useRef(null);
  const history = navigation.originHash === item.hash ? navigation.items : [];
  const currentItem = history[history.length - 1] || item;
  const loaded = documentState?.hash === currentItem.hash;
  const content = loaded ? documentState.content : null;
  const error = loaded ? documentState.error : null;
  const previous = history.length > 1 ? history[history.length - 2] : item;

  useEffect(() => { ref.current?.showModal(); return () => ref.current?.close(); }, []);
  useEffect(() => {
    sourceRequest.current++;
    setSourceError(null); setOpeningSource(null);
    return () => { sourceRequest.current++; };
  }, [item.hash]);
  useEffect(() => {
    let active = true;
    const hash = currentItem.hash;
    const request = currentItem.content_type === "L3"
      ? invoke("get_synthesis_details", { hash })
      : invoke("read_content_text", { hash }).then((body) => ({ body, sources: [] }));
    request.then((content) => { if (active) setDocumentState({ hash, content, error: null }); })
      .catch((error) => { if (active) setDocumentState({ hash, content: null, error: errorMessage(error) }); });
    return () => { active = false; };
  }, [currentItem.hash, currentItem.content_type]);

  async function openSource(source) {
    if (!source.available_locally) return;
    const request = ++sourceRequest.current;
    setOpeningSource(source.hash); setSourceError(null);
    try {
      const original = await invoke("get_content_details", { hash: source.hash });
      if (request === sourceRequest.current) setNavigation({ originHash: item.hash, items: [...history, original] });
    } catch (error) { if (request === sourceRequest.current) setSourceError(errorMessage(error)); }
    finally { if (request === sourceRequest.current) setOpeningSource(null); }
  }
  function goBack() {
    sourceRequest.current++; setOpeningSource(null); setSourceError(null);
    setNavigation({ originHash: item.hash, items: history.slice(0, -1) });
  }

  return <dialog ref={ref} className="studio-reader" aria-labelledby="reader-title" onCancel={(event) => { event.preventDefault(); onClose(); }}>
    <header><div style={{ display: "flex", alignItems: "center", gap: 12 }}>{history.length > 0 && <button className="studio-button" onClick={goBack}>← Back to {previous.content_type === "L3" ? "synthesis" : "source"}</button>}{currentItem.visibility && <span className="studio-badge"><Icon name="lock" size={12}/>{currentItem.visibility}</span>}</div><button className="studio-icon-button" aria-label="Close note" onClick={onClose}><Icon name="close"/></button></header>
    <div className="studio-reader-document">
      <span className="studio-eyebrow">{currentItem.content_type}{currentItem.version != null && ` · VERSION ${currentItem.version}`}</span>
      <h1 id="reader-title">{currentItem.title}</h1>
      <p className="studio-reader-meta">{currentItem.size != null && <>{Number(currentItem.size).toLocaleString()} bytes <span>·</span> </>}Saved on this node</p>
      {error ? <p className="studio-error" role="alert">{error}</p> : !loaded ? <p className="studio-muted">Opening {currentItem.content_type === "L3" ? "synthesis" : "note"}…</p> : <>
        {content.question && <section aria-label="Synthesis question" style={{ marginBottom: 24 }}><span className="studio-eyebrow">QUESTION</span><p style={{ whiteSpace: "pre-wrap", lineHeight: 1.6 }}>{content.question}</p></section>}
        <pre>{content.body}</pre>
        {content.sources?.length > 0 && <section aria-label="Original sources" style={{ marginTop: 32, paddingTop: 24, borderTop: "1px solid var(--border-subtle)" }}>
          <h2 style={{ fontSize: 16 }}>Original sources</h2>
          <ol style={{ listStyle: "none", padding: 0, display: "grid", gap: 10 }}>
            {content.sources.map((source) => <li key={source.hash}><button className="studio-button" style={{ width: "100%", textAlign: "left", justifyContent: "flex-start" }} disabled={!source.available_locally || openingSource !== null} onClick={() => openSource(source)}><span className="studio-eyebrow">[{source.label}]</span><span>{source.title}</span><Icon name="arrow" size={15}/></button>{!source.available_locally && <p className="studio-muted">This source is not stored on this device.</p>}</li>)}
          </ol>
          {openingSource && <p className="studio-muted" role="status">Opening original source…</p>}
          {sourceError && <p className="studio-error" role="alert">Couldn’t open source: {sourceError}</p>}
        </section>}
      </>}
    </div>
    <footer><span>CONTENT ID</span><code>{currentItem.hash}</code></footer>
  </dialog>;
}

export default function App() {
  const [mode, setMode] = useState("loading");
  const [startupError, setStartupError] = useState(null);
  const [identity, setIdentity] = useState(null);
  const [status, setStatus] = useState(null);
  const [items, setItems] = useState([]);
  const [stats, setStats] = useState(null);
  const [libraryLoading, setLibraryLoading] = useState(false);
  const [libraryLoaded, setLibraryLoaded] = useState(false);
  const [libraryError, setLibraryError] = useState(null);
  const [view, setView] = useState("synthesis");
  const [showCreate, setShowCreate] = useState(false);
  const [showSearch, setShowSearch] = useState(false);
  const [showBalance, setShowBalance] = useState(false);
  const [selectedContent, setSelectedContent] = useState(null);
  const [graphFocus, setGraphFocus] = useState(null);
  const [focusRequest, setFocusRequest] = useState(0);
  const [refreshVersion, setRefreshVersion] = useState(0);
  const [readerError, setReaderError] = useState(null);
  const [networkBusy, setNetworkBusy] = useState(false);
  const [nodeError, setNodeError] = useState(null);
  const [toast, setToast] = useState(null);
  const libraryVersion = useRef(0);
  const readerVersion = useRef(0);
  const loading = view === "library" && (libraryLoading || !libraryLoaded);

  const boot = useCallback(async () => {
    if (!isTauri()) { setMode("browser"); return; }
    setMode("loading"); setStartupError(null);
    try {
      const node = await invoke("get_node_status"); setStatus(node);
      if (node.peer_id) { setIdentity(await invoke("get_identity")); setMode("ready"); }
      else setMode(await invoke("check_identity") ? "unlock" : "create");
    } catch (err) { setStartupError(errorMessage(err)); setMode("error"); }
  }, []);
  useEffect(() => { boot(); }, [boot]);
  const loadLibrary = useCallback(async () => {
    const version = ++libraryVersion.current; setLibraryLoading(true);
    const results = await Promise.allSettled([invoke("list_content"), invoke("get_graph_stats")]);
    if (version !== libraryVersion.current) return;
    if (results[0].status === "fulfilled") { setItems(results[0].value); setLibraryError(null); } else setLibraryError(errorMessage(results[0].reason));
    if (results[1].status === "fulfilled") setStats(results[1].value);
    setLibraryLoaded(true); setLibraryLoading(false);
  }, []);
  useEffect(() => {
    if (mode !== "ready") return;
    if (view === "library") { loadLibrary(); return () => { libraryVersion.current++; }; }
  }, [mode, view, loadLibrary]);
  const refresh = useCallback(() => {
    if (view === "library") loadLibrary();
    else if (view === "graph") setRefreshVersion((version) => version + 1);
    invoke("get_node_status").then(setStatus).catch((error) => setNodeError(errorMessage(error)));
  }, [view, loadLibrary]);
  const contentChanged = useCallback(() => {
    setRefreshVersion((version) => version + 1);
    if (view === "library") loadLibrary();
    invoke("get_node_status").then(setStatus).catch((error) => setNodeError(errorMessage(error)));
    window.dispatchEvent(new Event("studio:content-saved"));
  }, [view, loadLibrary]);
  useEffect(() => {
    if (mode !== "ready") return;
    const timer = setInterval(() => { invoke("get_node_status").then(setStatus).catch((err) => setNodeError(errorMessage(err))); }, 15000);
    return () => clearInterval(timer);
  }, [mode]);
  useTauriEvents({ "graph:updated": () => { if (mode === "ready") contentChanged(); }, "l2:complete": () => { if (mode === "ready") contentChanged(); } });
  useEffect(() => { if (!toast) return; const timer = setTimeout(() => setToast(null), 5000); return () => clearTimeout(timer); }, [toast]);
  useEffect(() => {
    const handler = (event) => {
      if (mode !== "ready") return;
      if (document.querySelector(".sy-reader[open], .sy-thought-editor[open]")) return;
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") { event.preventDefault(); if (!showCreate && !selectedContent && !showBalance) setShowSearch((value) => !value); }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "n") { event.preventDefault(); if (!showSearch && !selectedContent && !showBalance) setShowCreate(true); }
    };
    window.addEventListener("keydown", handler); return () => window.removeEventListener("keydown", handler);
  }, [mode, showCreate, showSearch, selectedContent, showBalance]);
  function focusEntity(entityId) {
    setGraphFocus(entityId); setFocusRequest((request) => request + 1); setView("graph");
  }
  function navigateView(nextView) {
    setView(nextView);
  }
  async function openContent(item) {
    const version = ++readerVersion.current; setReaderError(null);
    try {
      const content = item.version != null ? item : await invoke("get_content_details", { hash: item.hash });
      if (version === readerVersion.current) setSelectedContent(content);
    } catch (error) { if (version === readerVersion.current) setReaderError(errorMessage(error)); }
  }
  async function toggleNetwork() {
    if (networkBusy) return; setNetworkBusy(true); setNodeError(null);
    try { await invoke(status?.network_active ? "stop_network" : "auto_start_network", status?.network_active ? {} : { listenPort: null }); setStatus(await invoke("get_node_status")); }
    catch (err) { setNodeError(errorMessage(err)); }
    finally { setNetworkBusy(false); }
  }
  function action(id) { if (id === "new") setShowCreate(true); else if (id === "import") window.__knowledgeImport?.openFilePicker?.(); else navigateView(id); }
  async function copyId() { try { await navigator.clipboard.writeText(identity.peer_id); setToast("Node ID copied"); } catch (err) { setNodeError(`Couldn’t copy the node ID. ${errorMessage(err)}`); } }
  function contentSaved() { contentChanged(); setToast("Saved to your local workspace"); }
  if (mode !== "ready") return <Setup mode={mode} onReady={(value) => { setIdentity(value); setMode("ready"); }} onRetry={boot} startupError={startupError}/>;
  const title = view === "synthesis" ? "Synthesis" : view === "graph" ? "Relationships" : view === "node" ? "Your node" : "Library";
  return <div className="studio-shell">
    <aside className="studio-sidebar"><Brand/><button className="studio-search-trigger" onClick={() => setShowSearch(true)}><Icon name="search"/><span>Find anything</span><kbd>⌘ K</kbd></button>
      <span className="studio-nav-label">WORKSPACE</span><nav aria-label="Workspace"><button className={view === "synthesis" ? "active" : ""} aria-current={view === "synthesis" ? "page" : undefined} onClick={() => navigateView("synthesis")}><Icon name="synthesis"/><span>Synthesis</span></button><button className={view === "library" ? "active" : ""} onClick={() => navigateView("library")}><Icon name="library"/><span>Library</span>{libraryLoaded && !libraryError && <span className="studio-nav-count">{items.length}</span>}</button><button className={view === "graph" ? "active" : ""} onClick={() => navigateView("graph")}><Icon name="graph"/><span>Relationships</span></button><button onClick={() => setShowBalance(true)}><Icon name="activity"/><span>Activity & fees</span></button></nav>
      <div className="studio-sidebar-guide"><span className="studio-guide-line"/><span className="studio-eyebrow">FROM SOURCES TO IDEAS</span><p>Bring your sources together.<br/>Make room for a new idea.</p><button onClick={() => navigateView("synthesis")}>Open your worktable<Icon name="arrow" size={15}/></button></div>
      <div className="studio-sidebar-bottom"><button className={`studio-node-link ${view === "node" ? "active" : ""}`} onClick={() => navigateView("node")}><span className={`studio-node-avatar ${status?.network_active ? "connected" : ""}`}><Icon name="node"/></span><span><strong>{identity?.name || "My workspace"}</strong><small>{status?.network_active ? "Network online" : "Local mode"}</small></span><Icon name="arrow" size={15}/></button><div className="studio-sidebar-foot"><span>STUDIO</span><span>0.1.0</span></div></div>
    </aside>
    <main className="studio-main"><header className="studio-topbar"><div className="studio-breadcrumb"><span>Workspace</span><span>/</span><strong>{title}</strong></div><span className="studio-local-state"><i className={status?.network_active ? "online" : ""}/>{status?.network_active ? `${status.connected_peers} peers connected` : "On this device"}</span></header>
      {view !== "synthesis" && <div className={`studio-page-title ${view === "graph" ? "is-relationships" : ""}`}><div><span className="studio-eyebrow">{view === "node" ? "IDENTITY & CONNECTION" : "YOUR KNOWLEDGE SPACE"}</span><h1>{title}<span className="studio-title-dot">.</span></h1><p>{view === "graph" ? "Explore one entity at a time, with readable relationships and their original sources." : view === "node" ? "A local identity. A place on the network. You choose when to connect." : "A collection of notes and sources, with their origins intact."}</p></div><div className="studio-page-actions"><button className="studio-icon-button" aria-label="Refresh workspace" onClick={refresh} disabled={loading}><Icon name="refresh"/></button><button className="studio-button" onClick={() => action("import")}><Icon name="import"/>Import files</button><button className="studio-button primary" onClick={() => setShowCreate(true)}><Icon name="plus"/>New note</button></div></div>}
      <div className="studio-synthesis-view" hidden={view !== "synthesis"} style={{ display: view === "synthesis" ? "flex" : "none", flex: 1, minHeight: 0, flexDirection: "column" }}><SynthesisWorkspace profileId={identity.peer_id} onSaved={contentSaved} onOpenContent={openContent}/></div>
      {view === "library" && <><div className="studio-metrics"><div><span className="studio-metric-icon"><Icon name="file"/></span><strong>{items.length}</strong><span>notes & sources</span></div><div><span className="studio-metric-icon"><Icon name="graph"/></span><strong>{stats?.entity_count ?? "—"}</strong><span>entities</span></div><div><span className="studio-metric-icon"><Icon name="activity"/></span><strong>{stats?.relationship_count ?? "—"}</strong><span>relationships</span></div><span className="studio-metrics-note"><Icon name="lock" size={13}/> Stored locally</span></div><div className="studio-library-wrap"><ContentLibrary items={items} onSelect={openContent} loading={loading} error={libraryError} onCreate={() => setShowCreate(true)} onImport={() => action("import")}/></div></>}
      {view === "graph" && <div className="studio-graph-wrap"><RelationshipExplorer initialEntityId={graphFocus} focusRequest={focusRequest} refreshVersion={refreshVersion} onFocusChange={setGraphFocus} onOpenContent={openContent}/></div>}
      {view === "node" && <div className="studio-node-page"><section className="studio-settings-card"><div className="studio-settings-heading"><span className="studio-card-icon"><Icon name="lock"/></span><div><h2>Your identity</h2><p>Created locally and encrypted with your password.</p></div></div><dl><div><dt>Workspace</dt><dd>{identity?.name || "My workspace"}</dd></div><div><dt>Node ID</dt><dd><code title={identity?.peer_id}>{shortId(identity?.peer_id)}</code><button className="studio-icon-button" aria-label="Copy node ID" onClick={copyId}><Icon name="copy" size={16}/></button></dd></div><div><dt>Storage</dt><dd className="studio-storage-path">{identity?.data_dir}</dd></div></dl></section><section className="studio-settings-card"><div className="studio-settings-heading"><span className="studio-card-icon"><Icon name="node"/></span><div><h2>Network connection</h2><p>{status?.network_active ? "Your node is connected to the peer network." : "Your workspace works offline. Connect when you want to discover peers."}</p></div></div><div className="studio-network-row"><span className="studio-badge"><i className={status?.network_active ? "online" : ""}/>{status?.network_active ? `${status.connected_peers} connected peers` : "Local mode"}</span><button className="studio-button" disabled={networkBusy} onClick={toggleNetwork}>{networkBusy ? "Updating connection…" : status?.network_active ? "Disconnect" : "Connect to network"}</button></div>{nodeError && <p role="alert" className="studio-error">{nodeError}</p>}</section><div className="studio-settings-note"><Icon name="file"/><p>New notes stay private. Connecting your node does not publish them.</p></div></div>}
      <footer className="studio-statusbar"><span><i/>{loading ? "Refreshing workspace…" : "Local workspace ready"}</span><span>Built on Nodalync <span className="studio-status-separator">/</span> Knowledge with provenance</span></footer>
    </main>
    <CreateContentDialog isOpen={showCreate} onClose={() => setShowCreate(false)} onCreated={contentSaved}/><KnowledgeImport onImportComplete={contentChanged}/><BalanceDashboard isOpen={showBalance} onClose={() => setShowBalance(false)}/><QuickFind isOpen={showSearch} onClose={() => setShowSearch(false)} items={items} onContent={openContent} onEntity={focusEntity} onAction={action}/>
    {selectedContent && <ContentReader item={selectedContent} onClose={() => { readerVersion.current++; setSelectedContent(null); }}/>}{readerError && <div role="alert" className="studio-toast studio-error"><span>Couldn’t open content: {readerError}</span><button className="studio-icon-button" aria-label="Dismiss content error" onClick={() => setReaderError(null)}><Icon name="close" size={14}/></button></div>}{toast && <div role="status" className="studio-toast"><Icon name="check" size={16}/>{toast}<button className="studio-icon-button" aria-label="Dismiss notification" onClick={() => setToast(null)}><Icon name="close" size={14}/></button></div>}
  </div>;
}
