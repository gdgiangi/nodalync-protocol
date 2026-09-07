/** A bounded force layout containing only the real graph entities and relationships. */
import { useEffect, useMemo, useState } from "react";
import { forceSimulation, forceLink, forceManyBody, forceCollide, forceX, forceY } from "d3-force";
import { GraphNode } from "./GraphNode";
import { GraphEdges } from "./GraphEdges";
import { LAYER_Y } from "./GraphScene";
import { getNodeLevel } from "../../lib/constants";

export function ForceGraph({
  l0Nodes, l2Nodes, l3Nodes, l2Links, l1Connections, selectedEntity, hoveredNode,
  onNodeClick, onNodeHover, onNodeUnhover, nodePositionsRef, nodeObjectsRef, onLayoutReady,
}) {
  const [tick, setTick] = useState(0);
  const allNodes = useMemo(() => [...l0Nodes, ...l2Nodes, ...l3Nodes], [l0Nodes, l2Nodes, l3Nodes]);
  const links = useMemo(() => [...l2Links, ...l1Connections], [l2Links, l1Connections]);

  useEffect(() => {
    const previous = new Map(nodePositionsRef.current);
    nodePositionsRef.current.clear();
    if (!allNodes.length) return;
    const radius = Math.max(12, Math.sqrt(allNodes.length) * 8);
    const nodes = allNodes.map((node, index) => {
      const position = previous.get(node.id);
      const angle = index / allNodes.length * Math.PI * 2;
      return {
        id: node.id,
        level: node._level || getNodeLevel(node),
        x: position?.x ?? Math.cos(angle) * radius,
        y: position?.z ?? Math.sin(angle) * radius,
      };
    });
    const ids = new Set(nodes.map((node) => node.id));
    const simulationLinks = links.map((link) => ({
      source: typeof link.source === "object" ? link.source.id : link.source,
      target: typeof link.target === "object" ? link.target.id : link.target,
    })).filter((link) => ids.has(link.source) && ids.has(link.target));

    const simulation = forceSimulation(nodes)
      .force("link", forceLink(simulationLinks).id((node) => node.id).distance(18).strength(0.3))
      .force("charge", forceManyBody().strength(-120).distanceMax(90))
      .force("collision", forceCollide(8).strength(1).iterations(3))
      .force("x", forceX(0).strength(0.06))
      .force("y", forceY(0).strength(0.06))
      .alphaDecay(0.03)
      .velocityDecay(0.5);
    function updatePositions() {
      for (const node of nodes) {
        nodePositionsRef.current.set(node.id, { x: node.x, y: LAYER_Y[node.level] ?? 0, z: node.y });
      }
      setTick((value) => value + 1);
    }
    simulation.on("tick", updatePositions);
    simulation.tick(180);
    updatePositions();
    const frame = requestAnimationFrame(() => onLayoutReady?.());
    return () => {
      simulation.stop();
      cancelAnimationFrame(frame);
    };
  }, [allNodes, links, nodePositionsRef, onLayoutReady]);

  const connected = useMemo(() => {
    const ids = new Set(hoveredNode ? [hoveredNode.id] : []);
    for (const link of links) {
      const source = typeof link.source === "object" ? link.source.id : link.source;
      const target = typeof link.target === "object" ? link.target.id : link.target;
      if (source === hoveredNode?.id) ids.add(target);
      if (target === hoveredNode?.id) ids.add(source);
    }
    return ids;
  }, [links, hoveredNode]);

  return (
    <group>
      <GraphEdges links={links} positions={nodePositionsRef.current} hoveredNode={hoveredNode} tick={tick} />
      {allNodes.map((node) => {
        const position = nodePositionsRef.current.get(node.id);
        if (!position || ![position.x, position.y, position.z].every(Number.isFinite)) return null;
        return <GraphNode key={node.id} node={node} position={[position.x, position.y, position.z]}
          level={node._level || getNodeLevel(node)}
          isSelected={selectedEntity?.id === node.id || selectedEntity === node.id}
          isHovered={hoveredNode?.id === node.id}
          isDimmed={Boolean(hoveredNode && !connected.has(node.id))}
          onClick={onNodeClick} onHover={onNodeHover} onUnhover={onNodeUnhover}
          nodeObjectsRef={nodeObjectsRef} />;
      })}
    </group>
  );
}
