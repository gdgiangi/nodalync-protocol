import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getEntityColor, formatPredicate } from "../lib/constants";
import "./relationships.css";

const DIRECTORY_SIZE = 12;
const RELATIONSHIP_SIZE = 8;
const SOURCE_SIZE = 12;
const EMPTY_PAGE = { items: [], total: 0, offset: 0, limit: 12, has_more: false };
const message = (error) => typeof error === "string" ? error : error?.message || "This view could not be loaded.";
const lastPageOffset = (page) => Math.floor(Math.max(0, page.total - 1) / page.limit) * page.limit;

function TypeLabel({ type }) {
  return <span className="rx-type"><i style={{ background: getEntityColor(type) }}/>{type || "Note"}</span>;
}

function PageControls({ page, noun, busy, onOffset }) {
  if (!page.total) return null;
  return <div className="rx-pagination">
    <span>{page.offset + 1}–{page.offset + page.items.length} of {page.total.toLocaleString()} {noun}</span>
    <div>
      <button aria-label={`Previous ${noun} page`} disabled={busy || page.offset === 0} onClick={() => onOffset(Math.max(0, page.offset - page.limit))}>←</button>
      <button aria-label={`Next ${noun} page`} disabled={busy || !page.has_more} onClick={() => onOffset(page.offset + page.limit)}>→</button>
    </div>
  </div>;
}

function Failure({ error, onRetry }) {
  return <div className="rx-failure" role="alert"><p>{error}</p><button onClick={onRetry}>Try again</button></div>;
}

function RelationshipRow({ relationship, focusId, onFocus }) {
  const sourceIsFocus = relationship.source.id === focusId;
  const targetIsFocus = relationship.target.id === focusId;
  const note = (entity, isFocus) => isFocus
    ? <div className="rx-endpoint is-focus"><span className="rx-endpoint-kicker">CURRENT FOCUS</span><strong>{entity.label}</strong><TypeLabel type={entity.entity_type}/></div>
    : <button className="rx-endpoint" aria-label={`Follow ${entity.label}`} onClick={() => onFocus(entity)}><span className="rx-endpoint-kicker">FOLLOW THIS {entity.id.startsWith("vault:") ? "NOTE" : "ENTITY"} ↗</span><strong>{entity.label}</strong><TypeLabel type={entity.entity_type}/></button>;
  return <li className="rx-relationship" aria-label={`${relationship.source.label} ${formatPredicate(relationship.predicate)} ${relationship.target.label}`}>
    {note(relationship.source, sourceIsFocus)}
    <div className="rx-verb"><span>{formatPredicate(relationship.predicate)}</span><span className="rx-directed-line" aria-hidden="true">⟶</span><small>{sourceIsFocus && targetIsFocus ? "SELF LINK" : sourceIsFocus ? "OUTGOING" : "INCOMING"}</small></div>
    {note(relationship.target, targetIsFocus)}
  </li>;
}

export default function RelationshipExplorer({ initialEntityId, focusRequest = 0, refreshVersion = 0, onFocusChange, onOpenContent }) {
  const [focus, setFocus] = useState(initialEntityId ? { id: initialEntityId, label: "" } : null);
  const focusRef = useRef(focus);
  focusRef.current = focus;
  const callbacks = useRef({ onFocusChange, onOpenContent });
  callbacks.current = { onFocusChange, onOpenContent };
  const externalRequest = useRef(focusRequest);
  const [trail, setTrail] = useState([]);
  const [kind, setKind] = useState("notes");
  const [query, setQuery] = useState("");
  const [entityType, setEntityType] = useState("");
  const [directoryOffset, setDirectoryOffset] = useState(0);
  const [directory, setDirectory] = useState({ ...EMPTY_PAGE, counts: null, types: [] });
  const [directoryBusy, setDirectoryBusy] = useState(true);
  const [directoryError, setDirectoryError] = useState(null);
  const [direction, setDirection] = useState("all");
  const [predicate, setPredicate] = useState("");
  const [relationshipOffset, setRelationshipOffset] = useState(0);
  const [neighborhood, setNeighborhood] = useState(null);
  const [neighborhoodBusy, setNeighborhoodBusy] = useState(false);
  const [neighborhoodError, setNeighborhoodError] = useState(null);
  const [sourcesOpen, setSourcesOpen] = useState(false);
  const [sourceOffset, setSourceOffset] = useState(0);
  const [sources, setSources] = useState(EMPTY_PAGE);
  const [sourcesBusy, setSourcesBusy] = useState(false);
  const [sourcesError, setSourcesError] = useState(null);
  const [attempt, setAttempt] = useState(0);
  const readingArea = useRef(null);
  const pendingScroll = useRef(null);
  const focusHeading = useRef(null);
  const pendingHeadingFocus = useRef(false);
  const viewRef = useRef(null);
  viewRef.current = { direction, predicate, relationshipOffset, sourcesOpen, sourceOffset };

  const selectFocus = useCallback((entity, remember = true, force = false) => {
    const current = focusRef.current;
    if (!entity?.id || (current?.id === entity.id && !force)) return;
    if (remember && current && current.id !== entity.id) {
      const previousView = { ...current, ...viewRef.current, scrollTop: readingArea.current?.scrollTop || 0 };
      setTrail((previous) => [...previous.slice(-29), previousView]);
    }
    setFocus({ id: entity.id, label: entity.label || "" });
    focusRef.current = { id: entity.id, label: entity.label || "" };
    setDirection(entity.direction || "all"); setPredicate(entity.predicate || ""); setRelationshipOffset(entity.relationshipOffset || 0);
    setSourcesOpen(entity.sourcesOpen || false); setSourceOffset(entity.sourceOffset || 0); setNeighborhood(null);
    pendingScroll.current = entity.scrollTop || 0;
    pendingHeadingFocus.current = Boolean(current || remember || force);
    if (force && current?.id === entity.id) setAttempt((value) => value + 1);
    callbacks.current.onFocusChange?.(entity.id);
    readingArea.current?.scrollTo({ top: 0 });
  }, []);

  useEffect(() => {
    if (externalRequest.current === focusRequest) return;
    externalRequest.current = focusRequest;
    if (initialEntityId) selectFocus({ id: initialEntityId }, true, true);
  }, [focusRequest, initialEntityId, selectFocus]);

  useEffect(() => {
    let active = true;
    setDirectoryBusy(true); setDirectoryError(null);
    const timer = setTimeout(() => {
      invoke("list_relationship_entities", { query: query.trim() || null, kind, entityType: entityType || null, offset: directoryOffset, limit: DIRECTORY_SIZE })
        .then((page) => {
          if (!active) return;
          if (page.offset > 0 && page.offset >= page.total) { setDirectoryOffset(lastPageOffset(page)); return; }
          setDirectory(page);
          if (!focusRef.current && page.items.length) selectFocus(page.items[0], false);
        })
        .catch((error) => { if (active) setDirectoryError(message(error)); })
        .finally(() => { if (active) setDirectoryBusy(false); });
    }, query ? 180 : 0);
    return () => { active = false; clearTimeout(timer); };
  }, [kind, query, entityType, directoryOffset, refreshVersion, attempt, selectFocus]);

  useEffect(() => {
    if (!focus?.id) return;
    let active = true;
    setNeighborhoodBusy(true); setNeighborhoodError(null);
    invoke("get_relationship_neighborhood", { entityId: focus.id, direction, predicate: predicate || null, offset: relationshipOffset, limit: RELATIONSHIP_SIZE })
      .then((page) => {
        if (!active) return;
        if (page.offset > 0 && page.offset >= page.total) { setRelationshipOffset(lastPageOffset(page)); return; }
        setNeighborhood(page);
        setFocus((current) => current?.id === page.entity.id ? { id: current.id, label: page.entity.label } : current);
      })
      .catch((error) => { if (active) setNeighborhoodError(message(error)); })
      .finally(() => { if (active) setNeighborhoodBusy(false); });
    return () => { active = false; };
  }, [focus?.id, direction, predicate, relationshipOffset, refreshVersion, attempt]);

  useEffect(() => {
    if (!sourcesOpen || !focus?.id) return;
    let active = true;
    setSourcesBusy(true); setSourcesError(null);
    invoke("list_relationship_sources", { entityId: focus.id, offset: sourceOffset, limit: SOURCE_SIZE })
      .then((page) => {
        if (!active) return;
        if (page.offset > 0 && page.offset >= page.total) { setSourceOffset(lastPageOffset(page)); return; }
        setSources(page);
      })
      .catch((error) => { if (active) setSourcesError(message(error)); })
      .finally(() => { if (active) setSourcesBusy(false); });
    return () => { active = false; };
  }, [sourcesOpen, focus?.id, sourceOffset, refreshVersion, attempt]);

  useEffect(() => {
    if (pendingScroll.current === null || neighborhoodBusy || (sourcesOpen && sourcesBusy) || neighborhood?.entity.id !== focus?.id) return;
    readingArea.current?.scrollTo({ top: pendingScroll.current });
    pendingScroll.current = null;
    if (pendingHeadingFocus.current) {
      focusHeading.current?.focus({ preventScroll: true });
      pendingHeadingFocus.current = false;
    }
  }, [focus?.id, neighborhood?.entity.id, neighborhoodBusy, sourcesOpen, sourcesBusy]);

  function goBack(index = trail.length - 1) {
    const destination = trail[index];
    if (!destination) return;
    setTrail(trail.slice(0, index));
    selectFocus(destination, false, true);
  }
  function chooseDirection(value) { setDirection(value); setRelationshipOffset(0); }
  function chooseRelationshipOffset(offset) { setRelationshipOffset(offset); readingArea.current?.scrollTo({ top: 0 }); }
  const current = neighborhood?.entity.id === focus?.id ? neighborhood : null;
  const sourceTotal = current?.source_total ?? 0;
  const retry = () => setAttempt((value) => value + 1);

  return <section className="rx-explorer" aria-label="Relationship explorer">
    <aside className="rx-directory" aria-label="Find a note or entity">
      <header><span className="rx-eyebrow">CHOOSE A STARTING POINT</span><h2>Your knowledge</h2></header>
      <div className="rx-kind" aria-label="Knowledge category">
        <button aria-pressed={kind === "notes"} onClick={() => { setKind("notes"); setEntityType(""); setDirectoryOffset(0); }}>Your notes <span>{directory.counts?.notes ?? "—"}</span></button>
        <button aria-pressed={kind === "concepts"} onClick={() => { setKind("concepts"); setEntityType(""); setDirectoryOffset(0); }}>Extracted <span>{directory.counts?.concepts ?? "—"}</span></button>
      </div>
      <label className="rx-search"><span aria-hidden="true">⌕</span><input aria-label="Search relationships" placeholder={kind === "notes" ? "Find a note…" : "Find an extracted entity…"} value={query} onChange={(event) => { setQuery(event.target.value); setDirectoryOffset(0); }}/></label>
      <label className="rx-type-filter"><span>Show</span><select aria-label="Filter entity type" value={entityType} disabled={directoryBusy} onChange={(event) => { setEntityType(event.target.value); setDirectoryOffset(0); }}><option value="">All types</option>{entityType && !directory.types.some((type) => type.entity_type === entityType) && <option value={entityType}>{entityType} · 0</option>}{directory.types.map((type) => <option key={type.entity_type} value={type.entity_type}>{type.entity_type} · {type.count}</option>)}</select></label>
      <div className="rx-directory-list" aria-busy={directoryBusy}>
        {directoryError ? <Failure error={directoryError} onRetry={retry}/> : directoryBusy ? <p className="rx-loading" role="status">Finding {kind === "notes" ? "notes" : "entities"}…</p> : !directory.items.length ? <div className="rx-empty-list"><h3>No matches here</h3><p>{query || entityType ? "Try another title or clear the type filter." : kind === "notes" ? "Imported vault notes will appear here. You can also browse extracted entities." : "Extracted entities will appear after your sources are indexed."}</p></div> : directory.items.map((entity) => <button key={entity.id} className={`rx-directory-item ${focus?.id === entity.id ? "is-selected" : ""}`} aria-pressed={focus?.id === entity.id} onClick={() => selectFocus(entity)}><strong>{entity.label}</strong><span><TypeLabel type={entity.entity_type}/><small>{entity.relationship_count ? `${entity.relationship_count} links` : "No links"}</small></span></button>)}
      </div>
      <PageControls page={directory} noun={kind === "notes" ? "notes" : "entities"} busy={directoryBusy} onOffset={setDirectoryOffset}/>
      <footer>{kind === "notes" ? "Most connected notes appear first." : "Automatically extracted names and topics. Open their sources to check the context."}</footer>
    </aside>

    <div className="rx-detail">
      <nav className="rx-trail" aria-label="Relationship navigation"><button onClick={() => goBack()} disabled={!trail.length}>← Back</button><div>{trail.slice(-3).map((entity, index) => <span key={`${entity.id}-${index}`}><button onClick={() => goBack(Math.max(0, trail.length - 3) + index)}>{entity.label || "Previous note"}</button><i aria-hidden="true">/</i></span>)}<strong>{focus?.label || "Choose a note"}</strong></div></nav>
      <div className="rx-reading-area" ref={readingArea} aria-busy={neighborhoodBusy}>
        {!focus ? <div className="rx-welcome"><span className="rx-welcome-mark" aria-hidden="true">◎</span><h2>Follow a thread of thought.</h2><p>Choose a note on the left. Its direct relationships will appear here, with a name and direction for every link.</p></div> : neighborhoodError ? <Failure error={neighborhoodError} onRetry={retry}/> : !current ? <p className="rx-loading" role="status">Opening this neighborhood…</p> : <>
          <header className="rx-focus-heading"><div><div className="rx-focus-kicker"><span className="rx-eyebrow">IN FOCUS</span><TypeLabel type={current.entity.entity_type}/></div><h2 ref={focusHeading} tabIndex={-1}>{current.entity.label}</h2>{current.entity.description && <p className="rx-description">{current.entity.description.replace(/^Obsidian: /, "")}</p>}</div><button className={`rx-sources-toggle ${sourcesOpen ? "is-active" : ""}`} aria-expanded={sourcesOpen} onClick={() => setSourcesOpen(!sourcesOpen)}>Original sources <span>{sourceTotal}</span></button></header>
          {sourcesOpen && <section className="rx-sources" aria-label="Original sources for this entity"><header><h3>Where this comes from</h3><p>Open the original before drawing a conclusion.</p></header>{sourcesError ? <Failure error={sourcesError} onRetry={retry}/> : sourcesBusy ? <p className="rx-loading" role="status">Opening source references…</p> : <><div className="rx-source-list">{sources.items.map((source) => <button key={source.hash} disabled={!source.available_locally} onClick={() => callbacks.current.onOpenContent?.({ hash: source.hash })}><span><strong>{source.title || "Untitled source"}</strong><small>{source.available_locally ? "Read original" : "Unavailable on this device"}</small></span><span aria-hidden="true">↗</span></button>)}</div>{!sources.total && <p className="rx-empty-copy">No original source references are stored for this entity.</p>}<PageControls page={sources} noun="sources" busy={sourcesBusy} onOffset={setSourceOffset}/></>}</section>}
          <section className="rx-connections" aria-label="Direct relationships">
            <div className="rx-section-heading"><h3>Direct connections <span>{current.entity.relationship_count}</span></h3><span>Read each link from left to right</span></div>
            <div className="rx-filters"><div className="rx-direction" aria-label="Link direction">{[["all", "All directions"], ["incoming", "Incoming"], ["outgoing", "Outgoing"]].map(([value, label]) => <button key={value} aria-pressed={direction === value} onClick={() => chooseDirection(value)}>{label} <span>{current.direction_counts[value]}</span></button>)}</div><select aria-label="Filter relationship type" value={predicate} onChange={(event) => { setPredicate(event.target.value); setRelationshipOffset(0); }}><option value="">Every relationship</option>{predicate && !current.predicates.some((item) => item.predicate === predicate) && <option value={predicate}>{formatPredicate(predicate)} · 0</option>}{current.predicates.map((item) => <option key={item.predicate} value={item.predicate}>{formatPredicate(item.predicate)} · {item.count}</option>)}</select></div>
            {neighborhoodBusy ? <p className="rx-loading" role="status">Updating connections…</p> : current.items.length ? <ol className="rx-relationship-list">{current.items.map((relationship) => <RelationshipRow key={relationship.id} relationship={relationship} focusId={focus.id} onFocus={selectFocus}/>)}</ol> : <div className="rx-no-connections"><h4>{current.entity.relationship_count ? "No links match these filters." : "No explicit links for this one yet."}</h4><p>{current.entity.relationship_count ? "Try another direction or relationship type." : sourceTotal ? "Its original sources are still available above. You can use them in a synthesis worktable." : "Choose another note to continue exploring."}</p></div>}
          </section>
        </>}
      </div>
      <footer className="rx-detail-footer">{current?.total ? <PageControls page={current} noun="connections" busy={neighborhoodBusy} onOffset={chooseRelationshipOffset}/> : <span>Follow a note to explore its neighborhood.</span>}</footer>
    </div>
  </section>;
}
