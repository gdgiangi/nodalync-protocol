import { useState } from "react";
import { TYPE_COLORS, EDGE_CATEGORY_COLORS } from "../lib/constants";

/**
 * Floating legend overlay for the 3D graph — shows layer hierarchy,
 * entity type colors, and relationship category meanings.
 * Collapsed by default to save space.
 */

const LAYERS = [
  { id: "L0", label: "Source Documents", color: "#3b82f6", size: 5, description: "Raw content — notes, articles, papers" },
  { id: "L2", label: "Entities", color: "#f59e0b", size: 10, description: "People, orgs, concepts (force-directed)" },
  { id: "L3", label: "Derived Knowledge", color: "#8b5cf6", size: 8, description: "Synthesized from L2 patterns" },
];

export default function GraphLegend() {
  const [expanded, setExpanded] = useState(false);

  return (
    <div
      className="absolute bottom-4 left-4 z-30 animate-fade-in"
      style={{
        background: "rgba(6, 6, 10, 0.88)",
        backdropFilter: "blur(16px)",
        WebkitBackdropFilter: "blur(16px)",
        border: "1px solid var(--border-subtle)",
        borderRadius: "var(--radius-lg)",
        overflow: "hidden",
        maxWidth: expanded ? 280 : 160,
        transition: "max-width 0.3s cubic-bezier(0.4, 0, 0.2, 1)",
      }}
    >
      {/* Toggle header */}
      <button
        onClick={() => setExpanded((prev) => !prev)}
        className="w-full flex items-center justify-between px-3 py-2"
        style={{
          background: "transparent",
          border: "none",
          cursor: "pointer",
          color: "var(--text-ghost)",
        }}
        onMouseEnter={(e) => (e.currentTarget.style.color = "var(--text-tertiary)")}
        onMouseLeave={(e) => (e.currentTarget.style.color = "var(--text-ghost)")}
      >
        <span
          style={{
            fontSize: 7,
            letterSpacing: "2.5px",
            textTransform: "uppercase",
            color: "inherit",
          }}
        >
          3D LAYERS
        </span>
        <svg
          width="10"
          height="10"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          style={{
            transform: expanded ? "rotate(180deg)" : "rotate(0deg)",
            transition: "transform 0.2s ease",
          }}
        >
          <polyline points="6 9 12 15 18 9" />
        </svg>
      </button>

      {/* Collapsed: just show layer dots */}
      {!expanded && (
        <div className="flex items-center gap-3 px-3 pb-2">
          {LAYERS.map((layer) => (
            <div key={layer.id} className="flex items-center gap-1.5">
              <div
                style={{
                  width: layer.size,
                  height: layer.size,
                  borderRadius: "50%",
                  background: layer.color,
                  boxShadow: `0 0 6px ${layer.color}50`,
                  flexShrink: 0,
                }}
              />
              <span style={{ fontSize: 8, color: "var(--text-ghost)", letterSpacing: "0.5px" }}>
                {layer.id}
              </span>
            </div>
          ))}
        </div>
      )}

      {/* Expanded content */}
      {expanded && (
        <div className="px-3 pb-3 space-y-3">
          {/* 3D Layer hierarchy */}
          <div>
            <SectionLabel>Layer Hierarchy (3D Space)</SectionLabel>
            <div className="space-y-2">
              {[...LAYERS].reverse().map((layer, i) => (
                <div key={layer.id} className="flex items-start gap-2.5">
                  <div className="flex flex-col items-center" style={{ minWidth: 14 }}>
                    <div
                      style={{
                        width: layer.size,
                        height: layer.size,
                        borderRadius: "50%",
                        background: layer.color,
                        boxShadow: `0 0 8px ${layer.color}40`,
                        flexShrink: 0,
                      }}
                    />
                    {i < LAYERS.length - 1 && (
                      <div
                        style={{
                          width: 1,
                          height: 12,
                          background: "rgba(255,255,255,0.08)",
                          marginTop: 3,
                        }}
                      />
                    )}
                  </div>
                  <div>
                    <div className="flex items-center gap-1.5">
                      <span style={{ fontSize: 9, color: layer.color, fontWeight: 600 }}>
                        {layer.id}
                      </span>
                      <span style={{ fontSize: 9, color: "var(--text-secondary)" }}>
                        {layer.label}
                      </span>
                    </div>
                    <span style={{ fontSize: 8, color: "var(--text-ghost)", lineHeight: 1.3, display: "block", marginTop: 1 }}>
                      {layer.description}
                    </span>
                  </div>
                </div>
              ))}
            </div>
          </div>

          {/* L1 connections */}
          <div>
            <SectionLabel>L1 Connections</SectionLabel>
            <div className="flex items-center gap-2">
              <div
                style={{
                  width: 1,
                  height: 16,
                  background: "linear-gradient(to bottom, #3b82f640, #f59e0b40)",
                  flexShrink: 0,
                }}
              />
              <span style={{ fontSize: 8, color: "var(--text-ghost)" }}>
                Vertical beams (visible on hover)
              </span>
            </div>
          </div>

          {/* Entity types (L2 colors) */}
          <div>
            <SectionLabel>Entity Types (L2)</SectionLabel>
            <div className="flex flex-wrap gap-1.5">
              {Object.entries(TYPE_COLORS).slice(0, 8).map(([type, color]) => (
                <div key={type} className="flex items-center gap-1.5">
                  <div
                    style={{
                      width: 6,
                      height: 6,
                      borderRadius: "50%",
                      background: color,
                      opacity: 0.8,
                      flexShrink: 0,
                    }}
                  />
                  <span style={{ fontSize: 8, color: "var(--text-ghost)" }}>
                    {type}
                  </span>
                </div>
              ))}
            </div>
          </div>

          {/* Controls hint */}
          <div>
            <SectionLabel>Controls</SectionLabel>
            <div className="space-y-0.5">
              <ControlHint keys="Drag" action="Orbit camera" />
              <ControlHint keys="Scroll" action="Zoom in/out" />
              <ControlHint keys="Right-drag" action="Pan" />
              <ControlHint keys="Click node" action="Select + focus" />
              <ControlHint keys="Hover" action="Show connections" />
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function SectionLabel({ children }) {
  return (
    <div
      style={{
        fontSize: 7,
        letterSpacing: "2px",
        textTransform: "uppercase",
        color: "rgba(255, 255, 255, 0.2)",
        marginBottom: 4,
      }}
    >
      {children}
    </div>
  );
}

function ControlHint({ keys, action }) {
  return (
    <div className="flex items-center gap-2">
      <span
        className="mono text-[7px] px-1 py-px rounded"
        style={{
          background: "rgba(255,255,255,0.04)",
          border: "1px solid rgba(255,255,255,0.06)",
          color: "var(--text-ghost)",
          minWidth: 42,
          textAlign: "center",
        }}
      >
        {keys}
      </span>
      <span style={{ fontSize: 8, color: "var(--text-ghost)" }}>{action}</span>
    </div>
  );
}
