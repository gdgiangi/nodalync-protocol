/**
 * GraphTooltip — HTML tooltip overlay positioned near hovered node.
 */
import { getNodeLevel } from "../../lib/constants";
import { LAYER_HEX } from "./GraphScene";
import { getEntityColor } from "../../lib/constants";

export function GraphTooltip({ node, position }) {
  if (!node || !position) return null;

  const level = node._level || getNodeLevel(node);
  const label = node.label || node.canonical_label || node.id;
  const entityType = node.entity_type || level;

  const color =
    level === "L2" && node.entity_type
      ? getEntityColor(node.entity_type)
      : LAYER_HEX[level] || "#f59e0b";

  const levelLabels = {
    L0: "Source Document",
    L1: "Mention",
    L2: "Entity",
    L3: "Derived Document",
  };

  return (
    <div
      className="tooltip"
      style={{
        left: position.x + 16,
        top: position.y - 10,
        maxWidth: 240,
        pointerEvents: "none",
        zIndex: 100,
      }}
    >
      <div className="flex items-center gap-2 mb-1">
        <div
          className="w-2 h-2 rounded-full flex-shrink-0"
          style={{ background: color, boxShadow: `0 0 6px ${color}60` }}
        />
        <span
          className="text-[11px] font-medium truncate"
          style={{ color: "rgba(255,255,255,0.9)" }}
        >
          {label}
        </span>
      </div>
      <div className="flex items-center gap-2">
        <span
          className="text-[8px] uppercase tracking-wider"
          style={{ color: color + "cc" }}
        >
          {entityType}
        </span>
        <span style={{ color: "rgba(255,255,255,0.08)" }}>·</span>
        <span className="text-[8px]" style={{ color: "rgba(255,255,255,0.3)" }}>
          {levelLabels[level] || level}
        </span>
      </div>
      {node.description && (
        <p
          className="text-[9px] mt-1.5 leading-relaxed"
          style={{ color: "rgba(255,255,255,0.4)" }}
        >
          {node.description.length > 100
            ? node.description.substring(0, 97) + "…"
            : node.description}
        </p>
      )}
      {node.source_count > 0 && (
        <div className="mt-1">
          <span className="mono text-[8px]" style={{ color: "rgba(255,255,255,0.25)" }}>
            {node.source_count} source{node.source_count !== 1 ? "s" : ""}
          </span>
        </div>
      )}
    </div>
  );
}
