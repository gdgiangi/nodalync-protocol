/**
 * Community Detection for Knowledge Graph Clustering
 * Uses simple connected components grouping based on edge density
 * Alternative to full Louvain algorithm for speed and simplicity
 */

/**
 * Detect communities using connected components with density-based clustering
 * @param {Array} nodes - Array of node objects with id property
 * @param {Array} links - Array of link objects with source/target properties
 * @returns {Object} - { clusters: Map<clusterId, nodeIds[]>, nodeToCluster: Map<nodeId, clusterId> }
 */
export function detectCommunities(nodes, links) {
  if (!nodes.length || !links.length) {
    // Single cluster for all nodes if no links
    const allNodeIds = nodes.map(n => n.id);
    return {
      clusters: new Map([['cluster_0', allNodeIds]]),
      nodeToCluster: new Map(allNodeIds.map(id => [id, 'cluster_0']))
    };
  }

  // Build adjacency list
  const adjacency = new Map();
  nodes.forEach(node => {
    adjacency.set(node.id, new Set());
  });

  links.forEach(link => {
    const source = typeof link.source === 'object' ? link.source.id : link.source;
    const target = typeof link.target === 'object' ? link.target.id : link.target;
    
    if (adjacency.has(source) && adjacency.has(target)) {
      adjacency.get(source).add(target);
      adjacency.get(target).add(source);
    }
  });

  // Find connected components using DFS
  const visited = new Set();
  const components = [];

  function dfs(nodeId, component) {
    if (visited.has(nodeId)) return;
    visited.add(nodeId);
    component.push(nodeId);
    
    const neighbors = adjacency.get(nodeId) || new Set();
    neighbors.forEach(neighborId => {
      if (!visited.has(neighborId)) {
        dfs(neighborId, component);
      }
    });
  }

  // Find all connected components
  nodes.forEach(node => {
    if (!visited.has(node.id)) {
      const component = [];
      dfs(node.id, component);
      if (component.length > 0) {
        components.push(component);
      }
    }
  });

  // Further subdivide large components based on edge density
  const finalClusters = [];
  components.forEach((component, idx) => {
    if (component.length <= 15) {
      // Small components stay as single clusters
      finalClusters.push(component);
    } else {
      // Large components get subdivided by edge density
      const subclusters = subdivideByDensity(component, adjacency, links);
      finalClusters.push(...subclusters);
    }
  });

  // Build result maps
  const clusters = new Map();
  const nodeToCluster = new Map();
  
  finalClusters.forEach((cluster, idx) => {
    const clusterId = `cluster_${idx}`;
    clusters.set(clusterId, cluster);
    cluster.forEach(nodeId => {
      nodeToCluster.set(nodeId, clusterId);
    });
  });

  return { clusters, nodeToCluster };
}

/**
 * Subdivide large connected components based on local edge density
 */
function subdivideByDensity(component, adjacency, links) {
  if (component.length <= 8) return [component];

  // Calculate edge density within this component
  const componentSet = new Set(component);
  const internalEdges = links.filter(link => {
    const source = typeof link.source === 'object' ? link.source.id : link.source;
    const target = typeof link.target === 'object' ? link.target.id : link.target;
    return componentSet.has(source) && componentSet.has(target);
  });

  const maxPossibleEdges = (component.length * (component.length - 1)) / 2;
  const density = internalEdges.length / maxPossibleEdges;

  // If density is high, keep as single cluster
  if (density > 0.3 || component.length <= 10) {
    return [component];
  }

  // Otherwise, split by node degree within the component
  const nodeDegrees = new Map();
  component.forEach(nodeId => {
    const neighbors = adjacency.get(nodeId) || new Set();
    const internalNeighbors = Array.from(neighbors).filter(n => componentSet.has(n));
    nodeDegrees.set(nodeId, internalNeighbors.length);
  });

  // Sort by degree and create multiple subclusters
  const sortedNodes = component.sort((a, b) => nodeDegrees.get(b) - nodeDegrees.get(a));
  const targetClusterSize = Math.ceil(component.length / Math.ceil(component.length / 12));
  
  const subclusters = [];
  for (let i = 0; i < sortedNodes.length; i += targetClusterSize) {
    subclusters.push(sortedNodes.slice(i, i + targetClusterSize));
  }

  return subclusters;
}

/**
 * Calculate cluster metrics for visualization
 */
export function calculateClusterMetrics(clusters, nodeToCluster, nodes, links) {
  const clusterMetrics = new Map();

  clusters.forEach((nodeIds, clusterId) => {
    const nodeSet = new Set(nodeIds);
    
    // Count internal edges
    const internalEdges = links.filter(link => {
      const source = typeof link.source === 'object' ? link.source.id : link.source;
      const target = typeof link.target === 'object' ? link.target.id : link.target;
      return nodeSet.has(source) && nodeSet.has(target);
    }).length;

    // Count external edges
    const externalEdges = links.filter(link => {
      const source = typeof link.source === 'object' ? link.source.id : link.source;
      const target = typeof link.target === 'object' ? link.target.id : link.target;
      return (nodeSet.has(source) && !nodeSet.has(target)) || 
             (!nodeSet.has(source) && nodeSet.has(target));
    }).length;

    // Determine dominant entity type
    const entityTypes = {};
    nodeIds.forEach(nodeId => {
      const node = nodes.find(n => n.id === nodeId);
      if (node?.entity_type) {
        entityTypes[node.entity_type] = (entityTypes[node.entity_type] || 0) + 1;
      }
    });
    
    const dominantType = Object.entries(entityTypes)
      .sort(([,a], [,b]) => b - a)[0]?.[0] || 'Concept';

    clusterMetrics.set(clusterId, {
      nodeCount: nodeIds.length,
      internalEdges,
      externalEdges,
      density: nodeIds.length > 1 ? internalEdges / ((nodeIds.length * (nodeIds.length - 1)) / 2) : 0,
      dominantEntityType: dominantType,
      entityTypeDistribution: entityTypes
    });
  });

  return clusterMetrics;
}