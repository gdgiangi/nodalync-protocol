/**
 * GraphScene — Main 3D knowledge graph visualization using React Three Fiber.
 * Renders L0 (bottom), L2 (middle), L3 (top) planes with force-directed layout.
 */
import { useRef, useState, useCallback, useMemo, useEffect } from "react";
import { Canvas } from "@react-three/fiber";
import { OrbitControls } from "@react-three/drei";
import { EffectComposer, Bloom } from "@react-three/postprocessing";
import * as THREE from "three";
import { ForceGraph } from "./ForceGraph";
import { GraphTooltip } from "./GraphTooltip";
import { getNodeLevel } from "../../lib/constants";

// Layer Y positions in 3D space
export const LAYER_Y = {
  L0: -8,
  L2: 0,
  L3: 8,
};

// Node colors by layer
export const LAYER_COLORS = {
  L0: new THREE.Color("#3b82f6"),
  L2: new THREE.Color("#f59e0b"),
  L3: new THREE.Color("#8b5cf6"),
};

export const LAYER_HEX = {
  L0: "#3b82f6",
  L2: "#f59e0b",
  L3: "#8b5cf6",
};

export default function GraphScene({
  data,
  onNodeClick,
  onBackgroundClick,
  selectedEntity,
}) {
  const controlsRef = useRef(null);
  const [hoveredNode, setHoveredNode] = useState(null);
  const [tooltipPos, setTooltipPos] = useState(null);

  // Classify nodes into layers
  const { l0Nodes, l2Nodes, l3Nodes, l1Connections, l2Links } = useMemo(() => {
    if (!data?.nodes?.length) {
      return { l0Nodes: [], l2Nodes: [], l3Nodes: [], l1Connections: [], l2Links: [] };
    }

    const l0 = [];
    const l2 = [];
    const l3 = [];

    const nodeMap = new Map();
    data.nodes.forEach((n) => {
      const level = getNodeLevel(n);
      nodeMap.set(n.id, { ...n, _level: level });
      if (level === "L0") l0.push({ ...n, _level: level });
      else if (level === "L3") l3.push({ ...n, _level: level });
      else l2.push({ ...n, _level: level }); // L1 and L2 go to middle plane
    });

    // L2-L2 edges (horizontal relationships)
    const links = (data.links || []).filter((l) => {
      const src = nodeMap.get(l.source);
      const tgt = nodeMap.get(l.target);
      return src && tgt;
    });

    // L1 connections: vertical beams between L0 and L2
    // For now, connect L0 nodes to L2 nodes that share entity_content_links
    // We'll infer these from the graph data — any edge from an L0 to an L2 is an L1 connection
    const l1conns = [];
    links.forEach((l) => {
      const src = nodeMap.get(l.source);
      const tgt = nodeMap.get(l.target);
      if (src && tgt) {
        if (
          (src._level === "L0" && (tgt._level === "L2" || tgt._level === "L1")) ||
          ((src._level === "L2" || src._level === "L1") && tgt._level === "L0")
        ) {
          l1conns.push({
            source: src._level === "L0" ? l.source : l.target,
            target: src._level === "L0" ? l.target : l.source,
          });
        }
      }
    });

    return {
      l0Nodes: l0,
      l2Nodes: l2,
      l3Nodes: l3,
      l1Connections: l1conns,
      l2Links: links.filter((l) => {
        const src = nodeMap.get(l.source);
        const tgt = nodeMap.get(l.target);
        return (
          src &&
          tgt &&
          (src._level === "L2" || src._level === "L1") &&
          (tgt._level === "L2" || tgt._level === "L1")
        );
      }),
    };
  }, [data]);

  const handleNodeHover = useCallback((node, screenPos) => {
    setHoveredNode(node);
    setTooltipPos(screenPos);
  }, []);

  const handleNodeUnhover = useCallback(() => {
    setHoveredNode(null);
    setTooltipPos(null);
  }, []);

  const handleBgClick = useCallback(
    (e) => {
      // Only if clicking the background (not a node)
      if (e.target === e.currentTarget || e.object?.userData?.isBackground) {
        onBackgroundClick?.();
      }
    },
    [onBackgroundClick]
  );

  return (
    <div className="w-full h-full relative" style={{ background: "#0a0a0a" }}>
      <Canvas
        camera={{ position: [0, 45, 60], fov: 50, near: 0.1, far: 500 }}
        gl={{ antialias: true, alpha: false, powerPreference: "high-performance" }}
        onCreated={({ gl }) => {
          gl.setClearColor("#0a0a0a");
          gl.toneMapping = THREE.ACESFilmicToneMapping;
          gl.toneMappingExposure = 1.2;
        }}
        style={{ background: "#0a0a0a" }}
      >
        {/* Ambient light */}
        <ambientLight intensity={0.15} />
        <pointLight position={[0, 20, 0]} intensity={0.3} color="#ffffff" />

        {/* Background plane for click detection */}
        <mesh
          position={[0, 0, -50]}
          userData={{ isBackground: true }}
          onClick={handleBgClick}
        >
          <planeGeometry args={[500, 500]} />
          <meshBasicMaterial color="#0a0a0a" transparent opacity={0} />
        </mesh>

        {/* Layer indicator planes (subtle grid) */}
        <LayerPlane y={LAYER_Y.L0} color="#3b82f6" label="L0 — Sources" />
        <LayerPlane y={LAYER_Y.L2} color="#f59e0b" label="L2 — Entities" />
        <LayerPlane y={LAYER_Y.L3} color="#8b5cf6" label="L3 — Derived" />

        {/* Force-directed graph */}
        <ForceGraph
          l0Nodes={l0Nodes}
          l2Nodes={l2Nodes}
          l3Nodes={l3Nodes}
          l2Links={l2Links}
          l1Connections={l1Connections}
          selectedEntity={selectedEntity}
          hoveredNode={hoveredNode}
          onNodeClick={onNodeClick}
          onNodeHover={handleNodeHover}
          onNodeUnhover={handleNodeUnhover}
          controlsRef={controlsRef}
        />

        {/* Post-processing bloom */}
        <EffectComposer>
          <Bloom
            luminanceThreshold={0.2}
            luminanceSmoothing={0.9}
            intensity={0.8}
            radius={0.6}
          />
        </EffectComposer>

        {/* Camera controls */}
        <OrbitControls
          ref={controlsRef}
          enablePan={true}
          enableZoom={true}
          enableRotate={true}
          minDistance={8}
          maxDistance={150}
          dampingFactor={0.08}
          enableDamping={true}
          rotateSpeed={0.4}
          zoomSpeed={0.6}
          panSpeed={0.8}
          target={[0, LAYER_Y.L2, 0]}
          makeDefault
        />
      </Canvas>

      {/* HTML tooltip overlay */}
      {hoveredNode && tooltipPos && (
        <GraphTooltip node={hoveredNode} position={tooltipPos} />
      )}
    </div>
  );
}

/**
 * Subtle translucent plane marking each layer
 */
function LayerPlane({ y, color }) {
  return (
    <group position={[0, y, 0]}>
      {/* Thin grid ring to indicate the plane */}
      <mesh rotation={[-Math.PI / 2, 0, 0]}>
        <ringGeometry args={[28, 30, 64]} />
        <meshBasicMaterial
          color={color}
          transparent
          opacity={0.04}
          side={THREE.DoubleSide}
        />
      </mesh>
      {/* Inner subtle disc */}
      <mesh rotation={[-Math.PI / 2, 0, 0]}>
        <circleGeometry args={[28, 64]} />
        <meshBasicMaterial
          color={color}
          transparent
          opacity={0.015}
          side={THREE.DoubleSide}
        />
      </mesh>
    </group>
  );
}
