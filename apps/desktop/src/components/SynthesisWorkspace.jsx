import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ReactFlow, Background, Controls, ReactFlowProvider } from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { blankBoard, boardKey, MAX_SOURCES, MAX_THOUGHTS, initialPassage, citationFor, removeSource, citePassage, saveFingerprint, validateBoard, readBoardStore } from "../lib/synthesis";
import "./synthesis/synthesis.css";

function Mark({ kind = "idea", size = 18 }) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{kind === "source" ? <><path d="M5 3h10l4 4v14H5zM15 3v5h4M8 12h8M8 16h6"/></> : kind === "question" ? <><circle cx="12" cy="12" r="9"/><path d="M9 9a3 3 0 0 1 6 0c0 2-3 2-3 4m0 3h.01"/></> : kind === "tension" ? <><path d="m5 5 6 6-6 8m14-14-6 6 6 8"/></> : kind === "search" ? <><circle cx="10" cy="10" r="6"/><path d="m15 15 5 5"/></> : <><path d="M9 18h6M9 21h6M8 14a7 7 0 1 1 8 0l-1 2H9zM12 1V0M2 9H0m24 0h-2"/></>}</svg>;
}
function SourceNode({ data, selected }) {
  return <article className={`sy-source-card ${selected ? "is-selected" : ""}`}>
    <header className="sy-drag-handle"><span className="sy-citation">{data.label}</span><span className="sy-card-kind">SOURCE PASSAGE</span><button className="nodrag sy-remove" aria-label={`Remove source ${data.title}`} onClick={data.onRemove}>×</button></header>
    <h3>{data.title}</h3>{data.description?.startsWith("Obsidian: ") && <p className="sy-source-origin" title={data.description}>{data.description.slice(10)}</p>}<p className="sy-passage">{data.excerpt || "Choose a passage from this source."}</p>
    <footer className="nodrag"><button onClick={data.onRead}>Read & choose passage <span>↗</span></button><button className="sy-cite-action" onClick={data.onCite}>Cite in draft</button></footer>
  </article>;
}
function ThoughtNode({ data, selected }) {
  return <article className={`sy-thought-card sy-${data.kind} ${selected ? "is-selected" : ""}`}>
    <header className="sy-drag-handle"><Mark kind={data.kind}/><span>{data.kind === "tension" ? "A TENSION" : data.kind === "question" ? "A QUESTION" : "MY THOUGHT"}</span><button className="nodrag sy-remove" aria-label={`Remove ${data.kind} card`} onClick={data.onRemove}>×</button></header>
    <button className="nodrag sy-thought-text" aria-label={`Edit ${data.kind} card`} onClick={data.onEdit}>{data.text || (data.kind === "tension" ? "These sources disagree about…" : data.kind === "question" ? "What if…? What is missing?" : "Putting these together, I think…")}</button>
    <footer className="nodrag"><span>Your interpretation</span><button disabled={!data.text.trim()} onClick={data.onUse}>Use in draft ↗</button></footer>
  </article>;
}
const nodeTypes = { source: SourceNode, thought: ThoughtNode };

function ThoughtEditor({ thought, onChange, onClose }) {
  const dialog = useRef(null);
  const textArea = useRef(null);
  useEffect(() => { const element = dialog.current; element.showModal(); textArea.current?.focus(); return () => element.close(); }, []);
  return <dialog ref={dialog} className="sy-thought-editor" aria-label={`Edit ${thought.kind}`} onCancel={(event) => { event.preventDefault(); onClose(); }}>
    <header><Mark kind={thought.kind}/><h2>{thought.kind === "tension" ? "Name the tension" : thought.kind === "question" ? "Follow a question" : "Put your thought into words"}</h2><button aria-label="Close thinking card editor" onClick={onClose}>×</button></header>
    <p>This is your interpretation. Let it be unfinished.</p>
    <textarea ref={textArea} aria-label="Thinking card text" placeholder="What are you noticing?" value={thought.text} onChange={(event) => onChange(event.target.value)}/>
    <footer><span>Kept with your worktable</span><button className="sy-primary" onClick={onClose}>Back to worktable</button></footer>
  </dialog>;
}

function SourceReader({ source, label, onClose, onExcerpt }) {
  const dialog = useRef(null);
  const textArea = useRef(null);
  const [text, setText] = useState(null);
  const [error, setError] = useState(null);
  const [range, setRange] = useState(null);
  useEffect(() => {
    const element = dialog.current; element.showModal();
    return () => element.close();
  }, []);
  useEffect(() => {
    let active = true;
    invoke("read_content_text", { hash: source.hash }).then((value) => { if (active) setText(value); }).catch((err) => { if (active) setError(String(err)); });
    return () => { active = false; };
  }, [source.hash]);
  return <dialog ref={dialog} className="sy-reader" onCancel={(event) => { event.preventDefault(); onClose(); }} aria-labelledby="sy-source-title">
    <header><span className="sy-citation">{label}</span><div><small>ORIGINAL SOURCE</small><h2 id="sy-source-title">{source.title}</h2></div><button aria-label="Close source reader" onClick={onClose}>×</button></header>
    <p>Select the passage you want to think with. The original stays intact.</p>
    {error ? <p role="alert">{error}</p> : text === null ? <p role="status">Opening source…</p> : <textarea ref={textArea} readOnly value={text} aria-label="Original source text. Select a passage to bring onto the board." onSelect={(event) => { const { selectionStart: start, selectionEnd: end } = event.target; setRange(end > start ? { start, end } : null); }}/>}
    <footer><code>{source.hash.slice(0, 16)}…</code><span>{range ? `${range.end - range.start} characters selected` : "Select text to keep a passage"}</span><button className="sy-primary" disabled={!range || range.end - range.start > 4000} onClick={() => { onExcerpt(text.slice(range.start, range.end), range); onClose(); }}>Keep passage</button></footer>
    {range && range.end - range.start > 4000 && <p role="alert">Choose a passage of up to 4,000 characters.</p>}
  </dialog>;
}

function Workspace({ profileId, onSaved, onOpenContent }) {
  const key = boardKey(profileId || "local");
  const [selectedIds, setSelectedIds] = useState(new Set());
  const [store, setStore] = useState({ version: 1, activeId: null, boards: [] });
  const storeRef = useRef(store);
  storeRef.current = store;
  const [loadedKey, setLoadedKey] = useState(null);
  const [storageError, setStorageError] = useState(null);
  const [error, setError] = useState(null);
  const [savedMessage, setSavedMessage] = useState(null);
  const [sourceOpen, setSourceOpen] = useState(true);
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(0);
  const [catalog, setCatalog] = useState({ items: [], total: 0, has_more: false });
  const [catalogBusy, setCatalogBusy] = useState(false);
  const [catalogError, setCatalogError] = useState(null);
  const [catalogVersion, setCatalogVersion] = useState(0);
  const [pendingSource, setPendingSource] = useState(null);
  const [reader, setReader] = useState(null);
  const [editingThought, setEditingThought] = useState(null);
  const [outline, setOutline] = useState(false);
  const [saving, setSaving] = useState(false);
  const submitting = useRef(false);
  const flow = useRef(null);
  const editor = useRef(null);
  const board = store.boards.find((item) => item.id === store.activeId);

  useEffect(() => {
    try {
      const restored = readBoardStore(localStorage, key);
      if (!restored.boards.length) { const next = blankBoard(); restored.boards = [next]; restored.activeId = next.id; }
      if (!restored.boards.some((item) => item.id === restored.activeId)) restored.activeId = restored.boards[0].id;
      const activeBoard = restored.boards.find((item) => item.id === restored.activeId);
      setSourceOpen(!activeBoard.sources.length && !activeBoard.thoughts.length);
      setStore(restored); setStorageError(null); setLoadedKey(key);
    } catch (err) { setStorageError(String(err)); setLoadedKey(null); }
  }, [key]);
  useEffect(() => {
    if (loadedKey !== key) return;
    const timer = setTimeout(() => {
      try { localStorage.setItem(key, JSON.stringify(store)); setStorageError(null); }
      catch { setStorageError("This worktable could not be stored on this device. Keep the app open and save your synthesis to protect the draft."); }
    }, 250);
    return () => clearTimeout(timer);
  }, [store, key, loadedKey]);
  useEffect(() => {
    if (loadedKey !== key) return;
    function flush() { try { localStorage.setItem(key, JSON.stringify(storeRef.current)); } catch { /* Visible autosave error handles quota failures. */ } }
    window.addEventListener("pagehide", flush);
    return () => { flush(); window.removeEventListener("pagehide", flush); };
  }, [key, loadedKey]);
  useEffect(() => {
    const refresh = () => setCatalogVersion((value) => value + 1);
    window.addEventListener("studio:content-saved", refresh);
    return () => window.removeEventListener("studio:content-saved", refresh);
  }, []);
  useEffect(() => {
    let active = true; setCatalogBusy(true); setCatalogError(null);
    const timer = setTimeout(() => {
      invoke("list_synthesis_sources", { query: query.trim() || null, offset: page * 12, limit: 12 })
        .then((data) => { if (active) setCatalog(data); })
        .catch((err) => { if (active) { setCatalogError(String(err)); setCatalog({ items: [], total: 0, has_more: false }); } })
        .finally(() => { if (active) setCatalogBusy(false); });
    }, query ? 180 : 0);
    return () => { active = false; clearTimeout(timer); };
  }, [query, page, catalogVersion]);

  const update = useCallback((change) => {
    setStore((current) => ({ ...current, boards: current.boards.map((item) => item.id === current.activeId ? { ...(typeof change === "function" ? change(item) : { ...item, ...change }), updatedAt: Date.now() } : item) }));
    setSavedMessage(null);
  }, []);
  function newBoard() {
    if (store.boards.length >= 30) { setError("You have 30 worktables. Continue an existing one; your saved syntheses are also in the library."); return; }
    const next = blankBoard();
    setStore((current) => ({ ...current, activeId: next.id, boards: [...current.boards, next] }));
    setSourceOpen(true); setReader(null); setError(null); setSavedMessage(null);
  }
  async function addSource(item) {
    if (pendingSource || saving || board.sources.some((source) => source.hash === item.hash)) return;
    if (board.sources.length >= MAX_SOURCES) { setError(`Keep this working set focused: up to ${MAX_SOURCES} sources per board.`); return; }
    const boardId = board.id;
    setPendingSource(item.hash); setError(null);
    try {
      const text = await invoke("read_content_text", { hash: item.hash });
      setStore((current) => ({ ...current, boards: current.boards.map((currentBoard) => currentBoard.id !== boardId || currentBoard.sources.some((source) => source.hash === item.hash) ? currentBoard : { ...currentBoard, sources: [...currentBoard.sources, { ...item, ...initialPassage(text), position: { x: 30 + (currentBoard.sources.length % 2) * 300, y: 30 + Math.floor(currentBoard.sources.length / 2) * 330 } }], updatedAt: Date.now() }) }));
    } catch (err) { setError(`Couldn’t bring in this source: ${String(err)}`); }
    finally { setPendingSource(null); }
  }
  function addThought(kind) {
    if (board.thoughts.length >= MAX_THOUGHTS) { setError(`A worktable holds up to ${MAX_THOUGHTS} thought cards. Start another board to explore a different question.`); return; }
    const position = { x: 30 + (board.thoughts.length % 2) * 300, y: Math.ceil(board.sources.length / 2) * 330 + 35 + Math.floor(board.thoughts.length / 2) * 260 };
    const id = crypto.randomUUID();
    update((current) => ({ ...current, thoughts: [...current.thoughts, { id, kind, text: "", position }] }));
    setEditingThought(id);
  }
  function append(text) { update((current) => ({ ...current, body: `${current.body.trimEnd()}${current.body.trim() ? "\n\n" : ""}${text}\n\n` })); editor.current?.focus(); }
  function takeOff(hash) { try { const next = removeSource(board, hash); update(next); } catch (err) { setError(err.message); } }
  function cite(source) { update((current) => ({ ...current, body: citePassage(current.body, source, citationFor(current.sources, source.hash)) })); editor.current?.focus(); }

  const nodes = useMemo(() => board ? [
    ...board.sources.map((source, index) => ({ id: source.hash, selected: selectedIds.has(source.hash), type: "source", position: source.position, dragHandle: ".sy-drag-handle", data: { ...source, label: `S${index + 1}`, onRemove: () => takeOff(source.hash), onRead: () => setReader(source), onCite: () => cite(source) } })),
    ...board.thoughts.map((thought) => ({ id: thought.id, selected: selectedIds.has(thought.id), type: "thought", position: thought.position, dragHandle: ".sy-drag-handle", data: { ...thought, onEdit: () => setEditingThought(thought.id), onChange: (text) => update((current) => ({ ...current, thoughts: current.thoughts.map((item) => item.id === thought.id ? { ...item, text } : item) })), onRemove: () => update((current) => ({ ...current, thoughts: current.thoughts.filter((item) => item.id !== thought.id) })), onUse: () => append(thought.text) } })),
  ] : [], [board, selectedIds]);
  function moveNodes(changes) {
    const selections = changes.filter((change) => change.type === "select");
    if (selections.length) setSelectedIds((current) => { const next = new Set(current); for (const change of selections) { if (change.selected) next.add(change.id); else next.delete(change.id); } return next; });
    const moved = changes.filter((change) => change.type === "position" && change.position);
    if (!moved.length) return;
    const positions = new Map(moved.map((change) => [change.id, change.position]));
    update((current) => ({ ...current, sources: current.sources.map((item) => positions.has(item.hash) ? { ...item, position: positions.get(item.hash) } : item), thoughts: current.thoughts.map((item) => positions.has(item.id) ? { ...item, position: positions.get(item.id) } : item) }));
  }
  async function save() {
    if (submitting.current) return;
    const validation = validateBoard(board);
    if (validation) { setError(validation); return; }
    const snapshot = board, fingerprint = saveFingerprint(board);
    submitting.current = true; setSaving(true); setError(null);
    try {
      const result = await invoke("save_synthesis", { title: snapshot.title.trim(), question: snapshot.question.trim(), body: snapshot.body, sourceHashes: snapshot.sources.map((source) => source.hash) });
      setStore((current) => ({ ...current, boards: current.boards.map((item) => item.id === snapshot.id ? { ...item, saved: { ...result, fingerprint }, updatedAt: Date.now() } : item) }));
      setSavedMessage("Saved privately, with its original sources."); onSaved?.(result);
    } catch (err) { setError(`Couldn’t save this synthesis: ${String(err)}`); }
    finally { submitting.current = false; setSaving(false); }
  }
  function openSaved() {
    const saved = board.saved;
    onOpenContent?.({ hash: saved.hash, title: saved.title, content_type: "L3", visibility: "Private", size: new TextEncoder().encode(saved.text).length });
  }

  if (!board || loadedKey !== key) return <div className="sy-loading">{storageError ? <p role="alert">{storageError}</p> : "Opening your worktable…"}</div>;
  const isSaved = board.saved?.fingerprint === saveFingerprint(board);
  return <section className="sy-workspace" aria-label="Synthesis workspace">
    <header className="sy-top"><div><span className="sy-eyebrow">FROM WHAT YOU KNOW · TO WHAT YOU THINK</span><h1>Synthesis<span>.</span></h1></div><div className="sy-board-tools"><label><span className="sy-sr">Choose worktable</span><select aria-label="Choose worktable" value={board.id} onChange={(event) => { setStore((current) => ({ ...current, activeId: event.target.value })); setReader(null); setError(null); }}>{store.boards.map((item, index) => <option key={item.id} value={item.id}>{item.title || `Untitled worktable ${index + 1}`}</option>)}</select></label><button onClick={newBoard} disabled={saving}>+ New worktable</button></div></header>
    <div className="sy-question"><span><Mark kind="question"/>WORKING QUESTION</span><input aria-label="Working question" placeholder="What are you trying to understand, challenge, or imagine?" value={board.question} onChange={(event) => update({ question: event.target.value })}/><small>A good question gives the material somewhere to go.</small></div>
    {(error || storageError || savedMessage) && <div className={`sy-notice ${error || storageError ? "is-error" : ""}`} role={error || storageError ? "alert" : "status"}><span>{error || storageError || savedMessage}</span>{error && <button aria-label="Dismiss error" onClick={() => setError(null)}>×</button>}</div>}
    <div className="sy-work-area">
      <div className="sy-thinking"><div className="sy-toolbar"><button className={sourceOpen ? "is-active" : ""} onClick={() => setSourceOpen(!sourceOpen)}><Mark kind="source"/>Sources <span>{board.sources.length}</span></button><div className="sy-tool-divider"/><button onClick={() => addThought("thought")}>+ Thought</button><button onClick={() => addThought("question")}>+ Question</button><button onClick={() => addThought("tension")}>+ Tension</button><div className="sy-toolbar-spacer"/><button title="Switch between visual worktable and accessible outline" onClick={() => setOutline(!outline)}>{outline ? "Canvas" : "Outline"}</button></div>
        <div className="sy-canvas">
          {outline ? <div className="sy-outline" aria-label="Worktable outline">{nodes.map((node) => node.type === "source" ? <SourceNode key={node.id} data={node.data}/> : <ThoughtNode key={node.id} data={node.data}/>)}{!nodes.length && <p>Bring in sources or add a thought to start your worktable.</p>}</div> : <ReactFlow key={board.id} nodes={nodes} edges={[]} nodeTypes={nodeTypes} onNodesChange={moveNodes} onInit={(instance) => { flow.current = instance; }} defaultViewport={board.viewport} onMoveEnd={(_, viewport) => update({ viewport })} minZoom={0.45} maxZoom={1.5} nodesConnectable={false} deleteKeyCode={null} panOnScroll zoomOnDoubleClick={false} onlyRenderVisibleElements colorMode="dark" ariaLabelConfig={{ "controls.fitView.ariaLabel": "Fit worktable" }}>
            <Background color="#3b4842" gap={24} size={1}/><Controls showInteractive={false} fitViewOptions={{ padding: 0.18, minZoom: 0.55, maxZoom: 0.9 }}/>
          </ReactFlow>}
          {!nodes.length && !outline && <div className="sy-empty"><div className="sy-empty-cards"><span><Mark kind="source" size={28}/></span><span><Mark kind="question" size={28}/></span><span><Mark kind="idea" size={28}/></span></div><h2>Let a new idea take shape.</h2><p>Bring a few sources onto the table.<br/>Put their ideas beside your own.<br/>Keep what emerges in the draft.</p><button onClick={() => setSourceOpen(true)}>Find your first sources ↗</button></div>}
          {sourceOpen && <aside className="sy-source-drawer" aria-label="Find sources"><header><div><span className="sy-eyebrow">MATERIAL TO THINK WITH</span><h2>Your sources</h2></div><button aria-label="Close source drawer" onClick={() => setSourceOpen(false)}>×</button></header><label className="sy-search"><Mark kind="search" size={16}/><input aria-label="Search original sources" placeholder="Search your library…" value={query} onChange={(event) => { setQuery(event.target.value); setPage(0); }}/></label><div className="sy-source-results" aria-busy={catalogBusy}>{catalogBusy ? <p role="status">Finding sources…</p> : catalogError ? <div role="alert"><p>{catalogError}</p><button onClick={() => setCatalogVersion((value) => value + 1)}>Try again</button></div> : catalog.items.length ? catalog.items.map((item) => { const included = board.sources.some((source) => source.hash === item.hash); return <button key={item.hash} className={included ? "is-included" : ""} disabled={included || !item.available_locally || Boolean(pendingSource) || saving} onClick={() => addSource(item)}><span className="sy-source-row-icon"><Mark kind="source"/></span><span><strong>{item.title || "Untitled source"}</strong><small>{!item.available_locally ? "Unavailable on this device" : included ? "On your worktable" : `${Math.max(1, Math.round(item.size / 1024))} KB · Original`}</small></span><b>{pendingSource === item.hash ? "…" : included ? "✓" : "+"}</b></button>; }) : <p>{query ? "No sources match. Try another word." : "Your originals will appear here. Create or import notes in the library to begin."}</p>}</div><footer><span>{catalog.total ? `${page * 12 + 1}–${Math.min(page * 12 + 12, catalog.total)} of ${catalog.total.toLocaleString()}` : "0 sources"}</span><button aria-label="Previous source page" disabled={page === 0 || catalogBusy} onClick={() => setPage(page - 1)}>←</button><button aria-label="Next source page" disabled={!catalog.has_more || catalogBusy} onClick={() => setPage(page + 1)}>→</button></footer><div className="sy-drawer-done"><button onClick={() => setSourceOpen(false)}>Return to worktable <span>→</span></button></div></aside>}
        </div><footer className="sy-canvas-footer"><span>{board.sources.length} sources · {board.thoughts.length} thinking cards</span><span>Arrange by dragging a card’s header</span></footer>
      </div>
      <aside className="sy-draft" aria-label="New idea draft"><header><span className="sy-draft-kicker"><Mark kind="idea"/>THE IDEA TAKING SHAPE</span><span className="sy-private">Private</span></header><div className="sy-draft-page"><textarea rows={2} aria-label="Synthesis title" className="sy-draft-title" placeholder="Give your idea a title" value={board.title} onChange={(event) => update({ title: event.target.value })}/><div className="sy-editor-label"><span>YOUR SYNTHESIS</span><small>{board.body.trim() ? board.body.trim().split(/\s+/).length : 0} words</small></div><textarea ref={editor} aria-label="Synthesis draft" placeholder={"What becomes possible when you put these sources together?\n\nWrite a new interpretation, proposal, or hypothesis. Use citations to keep its foundations close."} value={board.body} onChange={(event) => update({ body: event.target.value })}/><div className="sy-draft-sources"><span>BUILT FROM</span>{board.sources.length ? board.sources.map((source, index) => <button key={source.hash} onClick={() => setReader(source)} title={source.title}><b>S{index + 1}</b>{source.title}</button>) : <p>Your selected sources will stay attached to the idea.</p>}</div></div><footer><span>{storageError ? "Autosave needs attention" : "Worktable saved on this device"}</span><button className="sy-save" disabled={saving || isSaved} onClick={save}>{saving ? "Saving…" : isSaved ? "✓ Saved to library" : board.saved ? "Save revised synthesis" : "Save synthesis"}</button>{board.saved && <button className="sy-open-saved" onClick={openSaved}>Open saved synthesis ↗</button>}<small>{board.saved && !isSaved ? "Changes will be saved as a new derived document." : "Original sources are preserved with the synthesis."}</small></footer></aside>
    </div>
    {editingThought && board.thoughts.some((thought) => thought.id === editingThought) && <ThoughtEditor thought={board.thoughts.find((thought) => thought.id === editingThought)} onClose={() => setEditingThought(null)} onChange={(text) => update((current) => ({ ...current, thoughts: current.thoughts.map((thought) => thought.id === editingThought ? { ...thought, text } : thought) }))}/>}
    {reader && <SourceReader source={reader} label={citationFor(board.sources, reader.hash)} onClose={() => setReader(null)} onExcerpt={(excerpt, passage) => update((current) => ({ ...current, sources: current.sources.map((source) => source.hash === reader.hash ? { ...source, excerpt, passage } : source) }))}/>}
  </section>;
}
export default function SynthesisWorkspace(props) { return <ReactFlowProvider><Workspace {...props}/></ReactFlowProvider>; }
