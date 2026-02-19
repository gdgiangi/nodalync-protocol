/**
 * GraphEdges — Renders edge lines between nodes.
 * Two types: horizontal (L2-L2 relationships) and vertical (L1 connections).
 */
import { useRef, useMemo, useEffect } from "react";
import { useFrame } from "@react-three/fiber";
import * as THREE from "three";
import { LAYER_Y } from "./GraphScene";

export function GraphEdges({
  links,
  positions,
  hoveredNode,
  hoveredConnections,
  type, // "horizontal" or "vertical"
  intensity = "normal", // "intra-cluster", "inter-cluster", or "normal"
  tick, // force re-render on simulation tick
}) {
  const lineRef = useRef();

  // Build line geometry
  const { linePositions, lineColors } = useMemo(() => {
    if (!links.length) return { linePositions: null, lineColors: null };

    const pts = [];
    const cols = [];

    links.forEach((link) => {
      const srcId = typeof link.source === "object" ? link.source.id : link.source;
      const tgtId = typeof link.target === "object" ? link.target.id : link.target;
      const srcPos = positions.get(srcId);
      const tgtPos = positions.get(tgtId);

      if (!srcPos || !tgtPos) return;

      // For vertical L1 connections, draw from L0 pos to L2 pos
      let sx = srcPos.x, sy = srcPos.y, sz = srcPos.z;
      let tx = tgtPos.x, ty = tgtPos.y, tz = tgtPos.z;

      if (type === "vertical") {
        // Source is L0 (bottom), target is L2 (middle)
        sy = LAYER_Y.L0;
        ty = LAYER_Y.L2;
        // Use target's x,z position for the top end
      }

      pts.push(sx, sy, sz, tx, ty, tz);

      // Color
      const isHighlighted =
        hoveredNode &&
        (srcId === hoveredNode.id || tgtId === hoveredNode.id);

      if (type === "vertical") {
        // L1: blue tint
        const alpha = isHighlighted ? 0.6 : hoveredNode ? 0.0 : 0.0;
        // Only show vertical connections when hovering related node
        cols.push(
          0.23, 0.51, 0.96, alpha,
          0.23, 0.51, 0.96, alpha
        );
      } else {
        // L2-L2: cluster-aware intensity
        let alpha = 0.1; // default
        
        if (intensity === "intra-cluster") {
          // Within clusters: very faint, almost invisible
          alpha = isHighlighted ? 0.3 : hoveredNode ? 0.01 : 0.02;
        } else if (intensity === "inter-cluster") {
          // Between clusters: slightly brighter, showing connections
          alpha = isHighlighted ? 0.7 : hoveredNode ? 0.05 : 0.15;
        } else {
          // Normal intensity (fallback)
          alpha = isHighlighted ? 0.7 : hoveredNode ? 0.03 : 0.1;
        }

        if (isHighlighted) {
          cols.push(
            0.96, 0.62, 0.04, alpha,
            0.96, 0.62, 0.04, alpha
          );
        } else {
          cols.push(
            1.0, 1.0, 1.0, alpha,
            1.0, 1.0, 1.0, alpha
          );
        }
      }
    });

    if (pts.length === 0) return { linePositions: null, lineColors: null };

    return {
      linePositions: new Float32Array(pts),
      lineColors: new Float32Array(cols),
    };
  }, [links, positions, hoveredNode, hoveredConnections, type, tick]);

  if (!linePositions) return null;

  return (
    <lineSegments>
      <bufferGeometry>
        <bufferAttribute
          attach="attributes-position"
          array={linePositions}
          count={linePositions.length / 3}
          itemSize={3}
        />
        <bufferAttribute
          attach="attributes-color"
          array={lineColors}
          count={lineColors.length / 4}
          itemSize={4}
        />
      </bufferGeometry>
      <lineBasicMaterial
        vertexColors
        transparent
        opacity={1}
        depthWrite={false}
        blending={THREE.AdditiveBlending}
        linewidth={1}
      />
    </lineSegments>
  );
}
