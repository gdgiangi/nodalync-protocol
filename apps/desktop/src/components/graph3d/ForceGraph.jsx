/**
 * ForceGraph — Manages force simulation on L2 plane and renders all nodes/edges.
 * Uses d3-force for physics on the XZ plane at L2's Y level.
 */
import { useRef, useMemo, useEffect, useCallback, useState } from "react";
import { useFrame, useThree } from "@react-three/fiber";
import {
  forceSimulation,
  forceLink,
  forceManyBody,
  forceCenter,
  forceCollide,
} from "d3-force";
import * as THREE from "three";
import { GraphNode } from "./GraphNode";
import { GraphEdges } from "./GraphEdges";
import { LAYER_Y } from "./GraphScene";
import { getNodeLevel } from "../../lib/constants";

export function ForceGraph({
  l0Nodes,
  l2Nodes,
  l3Nodes,
  l2Links,
  l1Connections,
  selectedEntity,
  hoveredNode,
  onNodeClick,
  onNodeHover,
  onNodeUnhover,
  controlsRef,
}) {
  const groupRef = useRef();
  const simRef = useRef(null);
  const positionsRef = useRef(new Map());
  const [tick, setTick] = useState(0);

  // Build simulation for L2 nodes (force-directed on XZ plane)
  useEffect(() => {
    if (!l2Nodes.length) return;

    // Create simulation nodes with initial random positions on XZ
    const simNodes = l2Nodes.map((n, i) => ({
      id: n.id,
      x: (Math.random() - 0.5) * 30,
      y: 0, // not used by force, just for d3 compat
      z: (Math.random() - 0.5) * 30, // We'll use vx/vy for x/z movement
      vx: 0,
      vy: 0,
      index: i,
    }));

    const nodeIdMap = new Map(simNodes.map((n) => [n.id, n]));

    const simLinks = l2Links
      .map((l) => {
        const src = typeof l.source === "object" ? l.source.id : l.source;
        const tgt = typeof l.target === "object" ? l.target.id : l.target;
        if (nodeIdMap.has(src) && nodeIdMap.has(tgt)) {
          return { source: src, target: tgt, ...l };
        }
        return null;
      })
      .filter(Boolean);

    const sim = forceSimulation(simNodes)
      .force(
        "link",
        forceLink(simLinks)
          .id((d) => d.id)
          .distance(5)
          .strength(0.3)
      )
      .force("charge", forceManyBody().strength(-80).distanceMax(40))
      .force("centerX", forceCenter(0, 0).strength(0.05))
      .force("collide", forceCollide().radius(2).strength(0.5))
      .alphaDecay(0.02)
      .velocityDecay(0.3);

    sim.on("tick", () => {
      // Update positions map — d3 uses x,y but we map to 3D x,z
      simNodes.forEach((n) => {
        positionsRef.current.set(n.id, {
          x: n.x,
          y: LAYER_Y.L2,
          z: n.y, // d3's y → our z
        });
      });
      setTick((t) => t + 1);
    });

    simRef.current = sim;

    // Also set positions for L0 and L3 nodes (spread in a circle pattern)
    l0Nodes.forEach((n, i) => {
      const angle = (i / Math.max(l0Nodes.length, 1)) * Math.PI * 2;
      const radius = 8 + Math.random() * 12;
      positionsRef.current.set(n.id, {
        x: Math.cos(angle) * radius + (Math.random() - 0.5) * 4,
        y: LAYER_Y.L0,
        z: Math.sin(angle) * radius + (Math.random() - 0.5) * 4,
      });
    });

    l3Nodes.forEach((n, i) => {
      const angle = (i / Math.max(l3Nodes.length, 1)) * Math.PI * 2;
      const radius = 6 + Math.random() * 8;
      positionsRef.current.set(n.id, {
        x: Math.cos(angle) * radius + (Math.random() - 0.5) * 3,
        y: LAYER_Y.L3,
        z: Math.sin(angle) * radius + (Math.random() - 0.5) * 3,
      });
    });

    return () => {
      sim.stop();
    };
  }, [l0Nodes, l2Nodes, l3Nodes, l2Links]);

  // All nodes combined
  const allNodes = useMemo(
    () => [...l0Nodes, ...l2Nodes, ...l3Nodes],
    [l0Nodes, l2Nodes, l3Nodes]
  );

  // Get connected node IDs for hover highlighting
  const hoveredConnections = useMemo(() => {
    if (!hoveredNode) return new Set();
    const connected = new Set([hoveredNode.id]);
    l2Links.forEach((l) => {
      const src = typeof l.source === "object" ? l.source.id : l.source;
      const tgt = typeof l.target === "object" ? l.target.id : l.target;
      if (src === hoveredNode.id) connected.add(tgt);
      if (tgt === hoveredNode.id) connected.add(src);
    });
    l1Connections.forEach((l) => {
      if (l.source === hoveredNode.id) connected.add(l.target);
      if (l.target === hoveredNode.id) connected.add(l.source);
    });
    return connected;
  }, [hoveredNode, l2Links, l1Connections]);

  return (
    <group ref={groupRef}>
      {/* L2-L2 relationship edges */}
      <GraphEdges
        links={l2Links}
        positions={positionsRef.current}
        hoveredNode={hoveredNode}
        hoveredConnections={hoveredConnections}
        type="horizontal"
        tick={tick}
      />

      {/* L1 vertical connections (hidden by default, visible on hover) */}
      <GraphEdges
        links={l1Connections}
        positions={positionsRef.current}
        hoveredNode={hoveredNode}
        hoveredConnections={hoveredConnections}
        type="vertical"
        tick={tick}
      />

      {/* Render all nodes */}
      {allNodes.map((node) => {
        const pos = positionsRef.current.get(node.id);
        if (!pos) return null;

        const level = node._level || getNodeLevel(node);
        const isSelected =
          selectedEntity &&
          (selectedEntity.id === node.id || selectedEntity === node.id);
        const isHovered = hoveredNode?.id === node.id;
        const isConnectedToHover =
          hoveredNode && hoveredConnections.has(node.id);
        const isDimmed = hoveredNode && !hoveredConnections.has(node.id);

        return (
          <GraphNode
            key={node.id}
            node={node}
            position={[pos.x, pos.y, pos.z]}
            level={level}
            isSelected={isSelected}
            isHovered={isHovered}
            isConnectedToHover={isConnectedToHover}
            isDimmed={isDimmed}
            onClick={onNodeClick}
            onHover={onNodeHover}
            onUnhover={onNodeUnhover}
            controlsRef={controlsRef}
          />
        );
      })}
    </group>
  );
}
