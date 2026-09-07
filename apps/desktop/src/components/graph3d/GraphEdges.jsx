/** Relationships use a stable pixel width rather than subpixel WebGL line primitives. */
import { Line } from "@react-three/drei";

export function GraphEdges({ links, positions, hoveredNode }) {
  return <group>{links.map((link, index) => {
    const sourceId = typeof link.source === "object" ? link.source.id : link.source;
    const targetId = typeof link.target === "object" ? link.target.id : link.target;
    const source = positions.get(sourceId);
    const target = positions.get(targetId);
    if (!source || !target) return null;
    const points = [[source.x, source.y, source.z], [target.x, target.y, target.z]];
    if (!points.flat().every(Number.isFinite)) return null;
    const highlighted = hoveredNode && (sourceId === hoveredNode.id || targetId === hoveredNode.id);
    return <Line key={link.id || `${sourceId}:${targetId}:${index}`} points={points}
      color={highlighted ? "#ffdb8a" : "#97b1d2"} lineWidth={highlighted ? 3 : 2}
      transparent opacity={hoveredNode && !highlighted ? 0.25 : 0.85}
      depthWrite={false} toneMapped={false} />;
  })}</group>;
}
