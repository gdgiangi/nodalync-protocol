/**
 * ForceGraph — Cluster-based macro visualization of knowledge graph.
 * Groups nodes into communities and renders as glowing nebulae.
 * Individual nodes fade in on zoom for semantic zoom behavior.
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
import { ClusterNebula } from "./ClusterNebula";
import { LAYER_Y } from "./GraphScene";
import { getNodeLevel } from "../../lib/constants";
import { detectCommunities, calculateClusterMetrics } from "../../lib/communityDetection";

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
  const clusterPositionsRef = useRef(new Map());
  const [tick, setTick] = useState(0);
  const [hoveredCluster, setHoveredCluster] = useState(null);
  const { camera } = useThree();

  // Community detection and clustering
  const { clusters, nodeToCluster, clusterMetrics, clusterCentroids } = useMemo(() => {
    if (!l2Nodes.length) {
      return { 
        clusters: new Map(), 
        nodeToCluster: new Map(), 
        clusterMetrics: new Map(),
        clusterCentroids: new Map()
      };
    }

    // Detect communities in L2 network
    const { clusters, nodeToCluster } = detectCommunities(l2Nodes, l2Links);
    const clusterMetrics = calculateClusterMetrics(clusters, nodeToCluster, l2Nodes, l2Links);
    
    // Calculate initial cluster centroids (will be refined by simulation)
    const clusterCentroids = new Map();
    clusters.forEach((nodeIds, clusterId) => {
      // Start clusters in a circular arrangement
      const clusterIndex = Array.from(clusters.keys()).indexOf(clusterId);
      const totalClusters = clusters.size;
      const angle = (clusterIndex / Math.max(totalClusters, 1)) * Math.PI * 2;
      const radius = Math.max(15, totalClusters * 2);
      
      clusterCentroids.set(clusterId, {
        x: Math.cos(angle) * radius,
        y: LAYER_Y.L2,
        z: Math.sin(angle) * radius
      });
    });

    return { clusters, nodeToCluster, clusterMetrics, clusterCentroids };
  }, [l2Nodes, l2Links]);

  // Calculate zoom level for semantic zoom behavior
  const zoomLevel = useMemo(() => {
    if (!camera) return 1.0;
    const distance = camera.position.distanceTo(new THREE.Vector3(0, 0, 0));
    return Math.max(0, Math.min(1, (distance - 5) / 80)); // 0 = close, 1 = far
  }, [camera.position, tick]);

  // Enhanced force simulation with cluster-based layout
  useEffect(() => {
    if (!l2Nodes.length || !clusters.size) return;

    // Create simulation nodes - include both individual nodes AND cluster centroids
    const simNodes = [];
    
    // Add individual nodes with cluster assignment
    l2Nodes.forEach((n, i) => {
      const clusterId = nodeToCluster.get(n.id);
      const clusterCentroid = clusterCentroids.get(clusterId) || { x: 0, z: 0 };
      
      simNodes.push({
        id: n.id,
        x: clusterCentroid.x + (Math.random() - 0.5) * 4,
        y: 0, // d3 y coordinate
        z: clusterCentroid.z + (Math.random() - 0.5) * 4,
        clusterId,
        type: 'node',
        originalNode: n,
        index: i,
      });
    });

    // Add cluster centroid nodes
    clusters.forEach((nodeIds, clusterId) => {
      const centroid = clusterCentroids.get(clusterId);
      simNodes.push({
        id: `cluster_${clusterId}`,
        x: centroid.x,
        y: 0,
        z: centroid.z,
        clusterId,
        type: 'cluster',
        nodeCount: nodeIds.length,
        index: simNodes.length,
      });
    });

    const nodeIdMap = new Map(simNodes.map((n) => [n.id, n]));

    // Create links for simulation
    const simLinks = [];
    
    // Within-cluster attraction (stronger)
    clusters.forEach((nodeIds, clusterId) => {
      const clusterNodeId = `cluster_${clusterId}`;
      nodeIds.forEach(nodeId => {
        simLinks.push({
          source: nodeId,
          target: clusterNodeId,
          type: 'cluster-attraction',
          strength: 0.8
        });
      });
    });

    // Inter-cluster repulsion (add links between cluster centroids)
    const clusterIds = Array.from(clusters.keys());
    for (let i = 0; i < clusterIds.length; i++) {
      for (let j = i + 1; j < clusterIds.length; j++) {
        simLinks.push({
          source: `cluster_${clusterIds[i]}`,
          target: `cluster_${clusterIds[j]}`,
          type: 'cluster-repulsion',
          strength: 0.1
        });
      }
    }

    // Original L2 edges (weaker now)
    l2Links.forEach(l => {
      const src = typeof l.source === "object" ? l.source.id : l.source;
      const tgt = typeof l.target === "object" ? l.target.id : l.target;
      if (nodeIdMap.has(src) && nodeIdMap.has(tgt)) {
        simLinks.push({
          source: src,
          target: tgt,
          type: 'original',
          strength: 0.15,
          ...l
        });
      }
    });

    // Enhanced force simulation
    const sim = forceSimulation(simNodes)
      .force(
        "link",
        forceLink(simLinks)
          .id((d) => d.id)
          .distance((d) => {
            if (d.type === 'cluster-attraction') return 0.5;
            if (d.type === 'cluster-repulsion') return 25;
            return 3;
          })
          .strength((d) => d.strength || 0.3)
      )
      .force("charge", forceManyBody()
        .strength((d) => d.type === 'cluster' ? -200 : -30)
        .distanceMax(50)
      )
      .force("centerX", forceCenter(0, 0).strength(0.02))
      .force("collide", forceCollide()
        .radius((d) => d.type === 'cluster' ? 8 : 1.5)
        .strength(0.7)
      )
      .alphaDecay(0.015)
      .velocityDecay(0.4);

    sim.on("tick", () => {
      // Update positions for individual nodes
      simNodes.forEach((n) => {
        if (n.type === 'node') {
          positionsRef.current.set(n.id, {
            x: n.x,
            y: LAYER_Y.L2,
            z: n.y, // d3's y → our z
          });
        } else if (n.type === 'cluster') {
          clusterPositionsRef.current.set(n.clusterId, {
            x: n.x,
            y: LAYER_Y.L2,
            z: n.y,
          });
        }
      });
      setTick((t) => t + 1);
    });

    simRef.current = sim;

    // Position L0 and L3 nodes in clusters too
    l0Nodes.forEach((n, i) => {
      const angle = (i / Math.max(l0Nodes.length, 1)) * Math.PI * 2;
      const radius = 12 + Math.random() * 8;
      positionsRef.current.set(n.id, {
        x: Math.cos(angle) * radius + (Math.random() - 0.5) * 6,
        y: LAYER_Y.L0,
        z: Math.sin(angle) * radius + (Math.random() - 0.5) * 6,
      });
    });

    l3Nodes.forEach((n, i) => {
      const angle = (i / Math.max(l3Nodes.length, 1)) * Math.PI * 2;
      const radius = 8 + Math.random() * 6;
      positionsRef.current.set(n.id, {
        x: Math.cos(angle) * radius + (Math.random() - 0.5) * 4,
        y: LAYER_Y.L3,
        z: Math.sin(angle) * radius + (Math.random() - 0.5) * 4,
      });
    });

    return () => {
      sim.stop();
    };
  }, [l0Nodes, l2Nodes, l3Nodes, l2Links, clusters, nodeToCluster, clusterCentroids]);

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

  // Enhanced edge categorization for cluster-aware rendering
  const { intraClusterLinks, interClusterLinks } = useMemo(() => {
    const intra = [];
    const inter = [];
    
    l2Links.forEach(link => {
      const srcId = typeof link.source === "object" ? link.source.id : link.source;
      const tgtId = typeof link.target === "object" ? link.target.id : link.target;
      
      const srcCluster = nodeToCluster.get(srcId);
      const tgtCluster = nodeToCluster.get(tgtId);
      
      if (srcCluster && tgtCluster) {
        if (srcCluster === tgtCluster) {
          intra.push(link);
        } else {
          inter.push(link);
        }
      }
    });
    
    return { intraClusterLinks: intra, interClusterLinks: inter };
  }, [l2Links, nodeToCluster]);

  const handleClusterHover = useCallback((clusterId, nodes, metrics) => {
    setHoveredCluster(clusterId);
    // Could trigger tooltip with cluster info
  }, []);

  const handleClusterUnhover = useCallback(() => {
    setHoveredCluster(null);
  }, []);

  const handleClusterClick = useCallback((clusterId, nodes) => {
    // Focus camera on cluster and show individual nodes
    if (controlsRef?.current) {
      const clusterPos = clusterPositionsRef.current.get(clusterId);
      if (clusterPos) {
        const controls = controlsRef.current;
        const targetPos = new THREE.Vector3(clusterPos.x, clusterPos.y, clusterPos.z);
        
        // Animate to cluster
        const startTarget = controls.target.clone();
        const startTime = performance.now();
        const duration = 1000;

        function animate() {
          const elapsed = performance.now() - startTime;
          const t = Math.min(elapsed / duration, 1);
          const eased = 1 - Math.pow(1 - t, 3);

          controls.target.lerpVectors(startTarget, targetPos, eased);
          controls.update();

          if (t < 1) requestAnimationFrame(animate);
        }
        animate();
      }
    }
  }, [controlsRef]);

  return (
    <group ref={groupRef}>
      {/* Inter-cluster edges (slightly brighter) */}
      <GraphEdges
        links={interClusterLinks}
        positions={positionsRef.current}
        hoveredNode={hoveredNode}
        hoveredConnections={hoveredConnections}
        type="horizontal"
        intensity="inter-cluster"
        tick={tick}
      />

      {/* Intra-cluster edges (very faint) */}
      <GraphEdges
        links={intraClusterLinks}
        positions={positionsRef.current}
        hoveredNode={hoveredNode}
        hoveredConnections={hoveredConnections}
        type="horizontal"
        intensity="intra-cluster"
        tick={tick}
      />

      {/* L1 vertical connections */}
      <GraphEdges
        links={l1Connections}
        positions={positionsRef.current}
        hoveredNode={hoveredNode}
        hoveredConnections={hoveredConnections}
        type="vertical"
        tick={tick}
      />

      {/* Cluster nebulae */}
      {Array.from(clusters.entries()).map(([clusterId, nodeIds]) => {
        const clusterPos = clusterPositionsRef.current.get(clusterId);
        const metrics = clusterMetrics.get(clusterId);
        
        if (!clusterPos || !metrics) return null;

        const isHovered = hoveredCluster === clusterId;
        const isDimmed = hoveredCluster && hoveredCluster !== clusterId;
        
        return (
          <ClusterNebula
            key={clusterId}
            clusterId={clusterId}
            nodes={nodeIds.map(id => l2Nodes.find(n => n.id === id)).filter(Boolean)}
            centroid={[clusterPos.x, clusterPos.y, clusterPos.z]}
            metrics={metrics}
            isHovered={isHovered}
            isDimmed={isDimmed}
            zoomLevel={zoomLevel}
            onClick={handleClusterClick}
            onHover={handleClusterHover}
            onUnhover={handleClusterUnhover}
          >
            {/* Individual nodes within cluster */}
            {nodeIds.map(nodeId => {
              const node = l2Nodes.find(n => n.id === nodeId);
              const pos = positionsRef.current.get(nodeId);
              if (!node || !pos) return null;

              const level = node._level || getNodeLevel(node);
              const isSelected = selectedEntity && 
                (selectedEntity.id === node.id || selectedEntity === node.id);
              const isNodeHovered = hoveredNode?.id === node.id;
              const isConnectedToHover = hoveredNode && hoveredConnections.has(node.id);
              const isNodeDimmed = (hoveredNode && !hoveredConnections.has(node.id)) || 
                (hoveredCluster && hoveredCluster !== clusterId);

              return (
                <GraphNode
                  key={node.id}
                  node={node}
                  position={[pos.x - clusterPos.x, pos.y - clusterPos.y, pos.z - clusterPos.z]}
                  level={level}
                  isSelected={isSelected}
                  isHovered={isNodeHovered}
                  isConnectedToHover={isConnectedToHover}
                  isDimmed={isNodeDimmed}
                  zoomLevel={zoomLevel}
                  inCluster={true}
                  onClick={onNodeClick}
                  onHover={onNodeHover}
                  onUnhover={onNodeUnhover}
                  controlsRef={controlsRef}
                />
              );
            })}
          </ClusterNebula>
        );
      })}

      {/* L0 and L3 nodes (outside clusters) */}
      {[...l0Nodes, ...l3Nodes].map((node) => {
        const pos = positionsRef.current.get(node.id);
        if (!pos) return null;

        const level = node._level || getNodeLevel(node);
        const isSelected = selectedEntity && 
          (selectedEntity.id === node.id || selectedEntity === node.id);
        const isNodeHovered = hoveredNode?.id === node.id;
        const isConnectedToHover = hoveredNode && hoveredConnections.has(node.id);
        const isNodeDimmed = hoveredNode && !hoveredConnections.has(node.id);

        return (
          <GraphNode
            key={node.id}
            node={node}
            position={[pos.x, pos.y, pos.z]}
            level={level}
            isSelected={isSelected}
            isHovered={isNodeHovered}
            isConnectedToHover={isConnectedToHover}
            isDimmed={isNodeDimmed}
            zoomLevel={zoomLevel}
            inCluster={false}
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
