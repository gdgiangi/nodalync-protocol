import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getEntityColor, formatPredicate } from "../lib/constants";

const CONNECTION_LIMIT = 500;

function formatTimestamp(ts) {
  if (!ts) return "—";
  const date = new Date(typeof ts === "number" ? ts * 1000 : ts);
  if (isNaN(date.getTime())) return "—";
  const now = new Date();
  const diffMs = now - date;
  const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

  if (diffDays === 0) return "Today";
  if (diffDays === 1) return "Yesterday";
  if (diffDays < 7) return `${diffDays}d ago`;
  if (diffDays < 30) return `${Math.floor(diffDays / 7)}w ago`;
  return date.toLocaleDateString("en-US", { month: "short", day: "numeric" });
}

export default function EntityDetailPanel({
  entity,
  onClose,
  onEntitySelect,
  onFocusEntity,
  onContentSelect,
  availableContentHashes = [],
}) {
  const [activeTab, setActiveTab] = useState("overview");
  const [context, setContext] = useState(null);
  const [attempt, setAttempt] = useState(0);
  const [isOpen, setIsOpen] = useState(false);

  // Animate in on mount
  useEffect(() => {
    const frame = requestAnimationFrame(() => setIsOpen(true));
    return () => cancelAnimationFrame(frame);
  }, []);

  // The graph command is ID-based; text context only includes relationships
  // between search matches and cannot describe an entity's neighborhood.
  useEffect(() => {
    if (!entity?.id) return;
    let active = true;
    const entityId = entity.id;
    setContext(null);
    invoke("get_subgraph", { entityId, maxHops: 1, maxResults: CONNECTION_LIMIT })
      .then((data) => { if (active) setContext({ entityId, data, error: null }); })
      .catch((error) => { if (active) setContext({ entityId, data: null, error: String(error) }); });
    return () => { active = false; };
  }, [entity?.id, attempt]);

  const handleClose = useCallback(() => {
    setIsOpen(false);
    setTimeout(onClose, 300);
  }, [onClose]);

  if (!entity) return null;

  const label = entity.label || entity.canonical_label || entity.id;
  const color = getEntityColor(entity.entity_type);
  const aliases = entity.aliases || [];
  const confidence = entity.confidence != null ? entity.confidence : null;
  const sourceCount = entity.source_count || 0;

  const loading = context?.entityId !== entity.id;
  const contextError = loading ? null : context.error;
  const data = loading ? null : context.data;
  const relationships = (data?.links || []).filter((link) => link.source === entity.id || link.target === entity.id);
  const neighborIds = new Set(relationships.flatMap((link) => [link.source, link.target]));
  const connectedEntities = (data?.nodes || []).filter((node) => node.id !== entity.id && neighborIds.has(node.id));
  const limited = connectedEntities.length >= CONNECTION_LIMIT;

  const tabs = [
    { id: "overview", label: "Overview" },
    { id: "connections", label: "Connections", count: data ? relationships.length : null },
    { id: "sources", label: "Sources", count: sourceCount },
  ];

  return (
    <>
      {/* Backdrop */}
      <div
        className="fixed inset-0 z-40"
        style={{
          background: isOpen ? "rgba(0, 0, 0, 0.3)" : "transparent",
          transition: "background 0.3s ease",
          pointerEvents: isOpen ? "auto" : "none",
        }}
        onClick={handleClose}
      />

      {/* Panel */}
      <div
        className="fixed top-0 right-0 h-full z-50 flex flex-col"
        style={{
          width: 380,
          background: "rgba(6, 6, 10, 0.92)",
          backdropFilter: "blur(24px)",
          WebkitBackdropFilter: "blur(24px)",
          borderLeft: "1px solid var(--border-subtle)",
          transform: isOpen ? "translateX(0)" : "translateX(100%)",
          transition: "transform 0.3s cubic-bezier(0.4, 0, 0.2, 1)",
          boxShadow: isOpen ? "-8px 0 32px rgba(0, 0, 0, 0.3)" : "none",
        }}
      >
        {/* Header */}
        <div
          className="flex items-start gap-3 px-4 pt-4 pb-3 flex-shrink-0"
          style={{ borderBottom: "1px solid var(--border-subtle)" }}
        >
          {/* Entity icon */}
          <div
            className="w-10 h-10 rounded-lg flex items-center justify-center flex-shrink-0 mt-0.5"
            style={{
              background: color + "15",
              border: `1px solid ${color}25`,
            }}
          >
            <div
              className="w-3 h-3 rounded-full"
              style={{ background: color, opacity: 0.8 }}
            />
          </div>

          <div className="flex-1 min-w-0">
            <h2
              className="text-[15px] font-medium truncate"
              style={{ color: "var(--text-primary)" }}
            >
              {label}
            </h2>
            <div className="flex items-center gap-2 mt-0.5">
              <span
                className="text-[10px] uppercase tracking-wider"
                style={{ color: color + "cc" }}
              >
                {entity.entity_type}
              </span>
              {confidence != null && (
                <>
                  <span style={{ color: "rgba(255,255,255,0.08)" }}>·</span>
                  <span
                    className="mono text-[9px]"
                    style={{ color: "var(--text-ghost)" }}
                  >
                    {Math.round(confidence * 100)}% confidence
                  </span>
                </>
              )}
            </div>
          </div>

          {/* Close button */}
          <button
            onClick={handleClose}
            aria-label="Close entity details"
            className="w-7 h-7 flex items-center justify-center rounded-md flex-shrink-0"
            style={{
              color: "var(--text-ghost)",
              transition: "all 0.15s ease",
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.background = "var(--bg-hover)";
              e.currentTarget.style.color = "var(--text-secondary)";
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.background = "transparent";
              e.currentTarget.style.color = "var(--text-ghost)";
            }}
          >
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>

        {/* Quick stats bar */}
        <div
          className="flex items-center px-4 py-2.5 flex-shrink-0"
          style={{ borderBottom: "1px solid var(--border-subtle)" }}
        >
          <QuickStat label="SOURCES" value={sourceCount} />
          <Divider />
          <QuickStat label="CONNECTIONS" value={data ? `${relationships.length}${limited ? "+" : ""}` : null} />
          <Divider />
          <QuickStat
            label="FIRST SEEN"
            value={formatTimestamp(entity.first_seen)}
            mono={false}
          />
        </div>

        {/* Focus button */}
        <div className="px-4 py-2.5 flex-shrink-0" style={{ borderBottom: "1px solid var(--border-subtle)" }}>
          <button
            onClick={() => onFocusEntity?.(entity.id)}
            className="btn-accent btn w-full justify-center text-[10px]"
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
            >
              <circle cx="11" cy="11" r="8" />
              <line x1="21" y1="21" x2="16.65" y2="16.65" />
            </svg>
            Focus on {label}
          </button>
        </div>

        {/* Tabs */}
        <div
          className="flex flex-shrink-0"
          style={{ borderBottom: "1px solid var(--border-subtle)" }}
        >
          {tabs.map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className="flex-1 py-3 text-center relative"
              style={{
                fontSize: 9,
                letterSpacing: "2.5px",
                textTransform: "uppercase",
                color:
                  activeTab === tab.id
                    ? "rgba(255, 255, 255, 0.7)"
                    : "rgba(255, 255, 255, 0.25)",
                background: "transparent",
                border: "none",
                borderBottom:
                  activeTab === tab.id
                    ? "2px solid var(--accent)"
                    : "2px solid transparent",
                cursor: "pointer",
                transition: "all 0.2s ease",
                fontFamily:
                  "'SF Pro Display', -apple-system, 'Segoe UI', sans-serif",
              }}
              onMouseEnter={(e) => {
                if (activeTab !== tab.id)
                  e.currentTarget.style.color = "rgba(255, 255, 255, 0.4)";
              }}
              onMouseLeave={(e) => {
                if (activeTab !== tab.id)
                  e.currentTarget.style.color = "rgba(255, 255, 255, 0.25)";
              }}
            >
              {tab.label}
              {tab.count > 0 && (
                <span
                  className="ml-1.5 mono"
                  style={{
                    fontSize: 8,
                    color: "var(--text-ghost)",
                    letterSpacing: 0,
                  }}
                >
                  {tab.count}
                </span>
              )}
            </button>
          ))}
        </div>

        {/* Content area */}
        <div
          className="flex-1 overflow-y-auto px-4 py-3"
          style={{
            scrollbarWidth: "thin",
            scrollbarColor: "rgba(255,255,255,0.08) transparent",
          }}
        >
          {contextError && <div role="alert" className="studio-error"><p>Couldn’t load connections: {contextError}</p><button className="studio-button" onClick={() => setAttempt((value) => value + 1)}>Try again</button></div>}
          {loading ? (
            <LoadingSkeleton />
          ) : (
            <>
              {activeTab === "overview" && (
                <OverviewTab
                  entity={entity}
                  aliases={aliases}
                  color={color}
                />
              )}
              {activeTab === "connections" && data && (
                <>
                <p className="studio-muted">{relationships.length}{limited ? "+" : ""} relationships · {connectedEntities.length}{limited ? "+" : ""} neighboring entities</p>
                {limited && <p role="status" className="studio-muted">Showing the first {CONNECTION_LIMIT} neighbors.</p>}
                <ConnectionsTab
                  relationships={relationships}
                  connectedEntities={connectedEntities}
                  currentEntityId={entity.id}
                  onEntitySelect={onEntitySelect}
                />
                </>
              )}
              {activeTab === "sources" && (
                <SourcesTab entity={entity} sourceCount={sourceCount} onContentSelect={onContentSelect} availableContentHashes={availableContentHashes} />
              )}
            </>
          )}
        </div>
      </div>
    </>
  );
}

// ─── Sub-components ──────────────────────────────────────────────────────────

function QuickStat({ label, value, mono = true }) {
  return (
    <div className="flex-1 flex flex-col items-center gap-0.5">
      <span
        className={`text-[13px] ${mono ? "mono" : ""}`}
        style={{
          color: "var(--text-secondary)",
          fontWeight: 300,
          letterSpacing: mono ? "-0.5px" : "0",
        }}
      >
        {value ?? "—"}
      </span>
      <span className="label-xs">{label}</span>
    </div>
  );
}

function Divider() {
  return (
    <div
      className="w-px h-8 mx-3"
      style={{ background: "var(--border-subtle)" }}
    />
  );
}

function SectionLabel({ children }) {
  return (
    <h3
      className="mb-2 mt-4 first:mt-0"
      style={{
        fontSize: 8,
        letterSpacing: "3px",
        textTransform: "uppercase",
        color: "rgba(255, 255, 255, 0.25)",
        fontFamily: "'SF Pro Display', -apple-system, 'Segoe UI', sans-serif",
      }}
    >
      {children}
    </h3>
  );
}

function LoadingSkeleton() {
  return (
    <div className="space-y-3">
      {[1, 2, 3, 4].map((i) => (
        <div
          key={i}
          className="skeleton rounded"
          style={{
            height: i === 1 ? 48 : 32,
            animationDelay: `${i * 0.1}s`,
          }}
        />
      ))}
    </div>
  );
}

// ─── Overview Tab ────────────────────────────────────────────────────────────

function OverviewTab({ entity, aliases, color }) {
  return (
    <div className="animate-fade-in">
      {/* Description */}
      {entity.description && (
        <>
          <SectionLabel>Description</SectionLabel>
          <p
            className="text-[12px] leading-relaxed"
            style={{ color: "var(--text-secondary)" }}
          >
            {entity.description}
          </p>
        </>
      )}

      {/* Aliases */}
      {aliases.length > 0 && (
        <>
          <SectionLabel>Also known as</SectionLabel>
          <div className="flex flex-wrap gap-1.5">
            {aliases.map((alias, i) => (
              <span
                key={alias}
                className="pill pill-neutral animate-slide-up"
                style={{
                  animationDelay: `${i * 40}ms`,
                  animationFillMode: "both",
                }}
              >
                {alias}
              </span>
            ))}
          </div>
        </>
      )}

      {/* Metadata */}
      <SectionLabel>Details</SectionLabel>
      <div className="space-y-2">
        <MetadataRow label="Type" value={entity.entity_type} color={color} />
        <MetadataRow
          label="Confidence"
          value={
            entity.confidence != null
              ? `${Math.round(entity.confidence * 100)}%`
              : "—"
          }
        />
        <MetadataRow label="Sources" value={entity.source_count || 0} />
        <MetadataRow
          label="First seen"
          value={formatTimestamp(entity.first_seen)}
        />
        <MetadataRow
          label="Last updated"
          value={formatTimestamp(entity.last_updated)}
        />
        <MetadataRow label="ID" value={entity.id} mono />
      </div>

      {/* Freshness indicator */}
      {entity.source_count > 0 && (
        <>
          <SectionLabel>Freshness</SectionLabel>
          <FreshnessBar entity={entity} />
        </>
      )}
    </div>
  );
}

function MetadataRow({ label, value, color, mono }) {
  return (
    <div className="flex items-center justify-between py-1">
      <span
        className="text-[10px]"
        style={{ color: "var(--text-ghost)", letterSpacing: "0.5px" }}
      >
        {label}
      </span>
      <span
        className={`text-[11px] ${mono ? "mono" : ""}`}
        style={{ color: color || "var(--text-secondary)" }}
      >
        {value}
      </span>
    </div>
  );
}

function FreshnessBar({ entity }) {
  // Simple freshness calc: based on source_count
  const freshness = Math.min(1, (entity.source_count || 1) / 10);
  const width = Math.max(8, freshness * 100);

  return (
    <div className="space-y-1.5">
      <div
        className="h-1 rounded-full overflow-hidden"
        style={{ background: "rgba(255, 255, 255, 0.06)" }}
      >
        <div
          className="h-full rounded-full"
          style={{
            width: `${width}%`,
            background:
              freshness > 0.6
                ? "var(--green)"
                : freshness > 0.3
                ? "var(--yellow)"
                : "var(--text-ghost)",
            transition: "width 0.8s cubic-bezier(0.4, 0, 0.2, 1)",
          }}
        />
      </div>
      <div className="flex justify-between">
        <span className="mono text-[9px]" style={{ color: "var(--text-ghost)" }}>
          {entity.source_count} source{entity.source_count !== 1 ? "s" : ""}
        </span>
        <span className="mono text-[9px]" style={{ color: "var(--text-ghost)" }}>
          {Math.round(freshness * 100)}%
        </span>
      </div>
    </div>
  );
}

// ─── Connections Tab ─────────────────────────────────────────────────────────

function ConnectionsTab({
  relationships,
  connectedEntities,
  currentEntityId,
  onEntitySelect,
}) {
  if (relationships.length === 0 && connectedEntities.length === 0) {
    return (
      <div className="empty-state animate-fade-in">
        <p>No connections found</p>
      </div>
    );
  }

  // Group relationships by predicate
  const grouped = {};
  relationships.forEach((rel) => {
    const pred = rel.predicate || "relatedTo";
    if (!grouped[pred]) grouped[pred] = [];
    grouped[pred].push(rel);
  });

  return (
    <div className="animate-fade-in">
      {/* Relationship groups */}
      {Object.entries(grouped).map(([predicate, rels]) => (
        <div key={predicate} className="mb-4">
          <SectionLabel>{formatPredicate(predicate)}</SectionLabel>
          <div className="space-y-1">
            {rels.map((rel, i) => {
              // Determine the "other" entity
              const isSubject = rel.source === currentEntityId;
              const otherId = isSubject
                ? rel.target
                : rel.source;
              const otherEntity = connectedEntities.find(
                (e) => e.id === otherId
              );
              const otherLabel = otherEntity
                ? otherEntity.label || otherEntity.canonical_label
                : otherId;
              const otherType = otherEntity?.entity_type;

              return (
                <button
                  key={rel.id || i}
                  onClick={() => otherEntity && onEntitySelect?.(otherId)}
                  disabled={!otherEntity}
                  className="w-full text-left p-2 rounded-md animate-slide-up"
                  style={{
                    animationDelay: `${i * 40}ms`,
                    animationFillMode: "both",
                    background: "transparent",
                    border: "none",
                    cursor: "pointer",
                    transition: "background 0.15s ease",
                  }}
                  onMouseEnter={(e) => {
                    e.currentTarget.style.background = "var(--bg-hover)";
                  }}
                  onMouseLeave={(e) => {
                    e.currentTarget.style.background = "transparent";
                  }}
                >
                  <div className="flex items-center gap-2">
                    {otherType && (
                      <span
                        className="inline-block w-[7px] h-[7px] rounded-full flex-shrink-0"
                        style={{
                          backgroundColor: getEntityColor(otherType),
                          opacity: 0.8,
                        }}
                      />
                    )}
                    <span
                      className="text-[12px] truncate"
                      style={{ color: "var(--text-primary)" }}
                    >
                      {otherLabel}
                    </span>
                    {rel.confidence != null && (
                      <span
                        className="mono text-[9px] ml-auto flex-shrink-0"
                        style={{ color: "var(--text-ghost)" }}
                      >
                        {Math.round(rel.confidence * 100)}%
                      </span>
                    )}
                  </div>
                  {isSubject ? (
                    <span
                      className="text-[9px] ml-[15px] block mt-0.5"
                      style={{ color: "var(--text-ghost)" }}
                    >
                      → {formatPredicate(predicate)}
                    </span>
                  ) : (
                    <span
                      className="text-[9px] ml-[15px] block mt-0.5"
                      style={{ color: "var(--text-ghost)" }}
                    >
                      ← {formatPredicate(predicate)}
                    </span>
                  )}
                </button>
              );
            })}
          </div>
        </div>
      ))}

      {/* Connected entities without explicit relationships */}
      {connectedEntities.length > 0 &&
        Object.keys(grouped).length === 0 && (
          <>
            <SectionLabel>Connected entities</SectionLabel>
            <div className="space-y-1">
              {connectedEntities.map((e, i) => (
                <button
                  key={e.id}
                  onClick={() => onEntitySelect?.(e.id)}
                  className="w-full text-left p-2 rounded-md animate-slide-up"
                  style={{
                    animationDelay: `${i * 40}ms`,
                    animationFillMode: "both",
                    background: "transparent",
                    border: "none",
                    cursor: "pointer",
                    transition: "background 0.15s ease",
                  }}
                  onMouseEnter={(e) => {
                    e.currentTarget.style.background = "var(--bg-hover)";
                  }}
                  onMouseLeave={(e) => {
                    e.currentTarget.style.background = "transparent";
                  }}
                >
                  <div className="flex items-center gap-2">
                    <span
                      className="inline-block w-[7px] h-[7px] rounded-full flex-shrink-0"
                      style={{
                        backgroundColor: getEntityColor(e.entity_type),
                        opacity: 0.8,
                      }}
                    />
                    <span
                      className="text-[12px] truncate"
                      style={{ color: "var(--text-primary)" }}
                    >
                      {e.label || e.canonical_label}
                    </span>
                    <span
                      className="mono text-[9px] ml-auto flex-shrink-0"
                      style={{ color: "var(--text-ghost)" }}
                    >
                      {e.entity_type}
                    </span>
                  </div>
                </button>
              ))}
            </div>
          </>
        )}
    </div>
  );
}

// ─── Sources Tab ─────────────────────────────────────────────────────────────

function SourcesTab({ entity, sourceCount, onContentSelect, availableContentHashes = [] }) {
  const [result, setResult] = useState(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    if (!entity?.id) return;
    let active = true;
    const entityId = entity.id;
    setResult(null);
    invoke("get_entity_content_links", { entityId: String(entityId) })
      .then((links) => { if (active) setResult({ entityId, links: links || [], error: null }); })
      .catch((error) => { if (active) setResult({ entityId, links: [], error: String(error) }); });
    return () => { active = false; };
  }, [entity?.id, attempt]);

  const linksLoading = result?.entityId !== entity.id;
  const contentLinks = linksLoading ? [] : result.links;
  const linksError = linksLoading ? null : result.error;
  const available = new Set(availableContentHashes);

  if (linksLoading) {
    return <LoadingSkeleton />;
  }

  if (linksError) {
    return <div role="alert" className="studio-error"><p>Couldn’t load sources: {linksError}</p><button className="studio-button" onClick={() => setAttempt((value) => value + 1)}>Try again</button></div>;
  }

  if (sourceCount === 0 && contentLinks.length === 0) {
    return (
      <div className="empty-state animate-fade-in">
        <p>No sources linked yet</p>
      </div>
    );
  }

  return (
    <div className="animate-fade-in">
      {/* L0 Content Links from IPC */}
      {contentLinks.length > 0 && (
        <>
          <SectionLabel>Linked Content (L0)</SectionLabel>
          <div className="space-y-1.5">
            {contentLinks.map((cl, i) => (
              <button
                type="button"
                key={cl.content_id || cl.content_hash || i}
                className="card animate-slide-up w-full text-left"
                disabled={!onContentSelect || !available.has(cl.content_hash)}
                onClick={() => onContentSelect?.(cl.content_hash)}
                aria-label={`Open source ${cl.content_hash}`}
                style={{
                  animationDelay: `${i * 50}ms`,
                  animationFillMode: "both",
                }}
              >
                <div className="flex items-center gap-3">
                  <div
                    className="w-8 h-8 rounded flex items-center justify-center flex-shrink-0"
                    style={{
                      background: "var(--accent-dim)",
                      border: "1px solid rgba(92, 124, 250, 0.2)",
                    }}
                  >
                    <svg
                      width="14"
                      height="14"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="1.5"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      style={{ color: "var(--accent)" }}
                    >
                      <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                      <polyline points="14 2 14 8 20 8" />
                      <line x1="16" y1="13" x2="8" y2="13" />
                      <line x1="16" y1="17" x2="8" y2="17" />
                    </svg>
                  </div>
                  <div className="flex-1 min-w-0">
                    <p
                      className="text-[11px] truncate"
                      style={{ color: "var(--text-secondary)" }}
                    >
                      {cl.content_hash
                        ? cl.content_hash.substring(0, 16) + "…"
                        : cl.content_id}
                    </p>
                    <div className="flex items-center gap-2 mt-0.5">
                      <span className="pill pill-neutral" style={{ fontSize: 7 }}>
                        {cl.content_type || "L0"}
                      </span>
                      {cl.linked_at && (
                        <span
                          className="text-[9px]"
                          style={{ color: "var(--text-ghost)" }}
                        >
                          {formatTimestamp(cl.linked_at)}
                        </span>
                      )}
                    </div>
                    {!available.has(cl.content_hash) && <p className="studio-muted">Not available in this library</p>}
                  </div>
                </div>
              </button>
            ))}
          </div>
        </>
      )}

      {/* Fallback: summary count when IPC returned nothing but entity has sources */}
      {contentLinks.length === 0 && sourceCount > 0 && (
        <>
          <SectionLabel>Source References</SectionLabel>
          <div className="card" style={{ background: "var(--bg-surface)" }}>
            <div className="flex items-center gap-3">
              <div
                className="w-8 h-8 rounded flex items-center justify-center flex-shrink-0"
                style={{
                  background: "var(--accent-dim)",
                  border: "1px solid rgba(92, 124, 250, 0.2)",
                }}
              >
                <svg
                  width="14"
                  height="14"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  style={{ color: "var(--accent)" }}
                >
                  <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                  <polyline points="14 2 14 8 20 8" />
                  <line x1="16" y1="13" x2="8" y2="13" />
                  <line x1="16" y1="17" x2="8" y2="17" />
                </svg>
              </div>
              <div>
                <p className="text-[12px]" style={{ color: "var(--text-secondary)" }}>
                  {sourceCount} L0 document{sourceCount !== 1 ? "s" : ""} reference this entity
                </p>
              </div>
            </div>
          </div>
        </>
      )}

      {/* Metadata about extraction */}
      <SectionLabel>Extraction Info</SectionLabel>
      <div className="space-y-2">
        <MetadataRow label="Entity ID" value={entity.id} mono />
        <MetadataRow label="First extracted" value={formatTimestamp(entity.first_seen)} />
        <MetadataRow label="Last updated" value={formatTimestamp(entity.last_updated)} />
        <MetadataRow
          label="Confidence"
          value={
            entity.confidence != null
              ? `${Math.round(entity.confidence * 100)}%`
              : "—"
          }
        />
      </div>
    </div>
  );
}
