/**
 * Graph3D — Drop-in replacement for the D3 GraphView.
 * Wraps GraphScene with the same API as the old D3 component.
 */
import {
  forwardRef,
  useImperativeHandle,
  useRef,
  useMemo,
  useCallback,
} from "react";
import GraphScene from "./GraphScene";

const Graph3D = forwardRef(function Graph3D(
  { data, onNodeClick, onBackgroundClick, selectedEntity },
  ref
) {
  const sceneRef = useRef(null);

  // Transform data shape: backend returns { entities, relationships } or { nodes, links }
  const normalizedData = useMemo(() => {
    if (!data) return { nodes: [], links: [] };

    const nodes = data.nodes || data.entities || [];
    const links = (data.links || data.relationships || []).map((r) => ({
      source: r.source || r.source_id || r.subject_id,
      target: r.target || r.target_id || r.object_id,
      predicate: r.predicate || r.relationship_type,
      confidence: r.confidence,
      ...r,
    }));

    return { nodes, links };
  }, [data]);

  // Expose imperative methods for parent compatibility
  useImperativeHandle(ref, () => ({
    zoomToEntity(entityId) {
      return sceneRef.current?.zoomToEntity(entityId) ?? false;
    },
    resetZoom() {
      sceneRef.current?.resetZoom();
    },
  }));

  const handleNodeClick = useCallback(
    (node) => {
      onNodeClick?.(node);
    },
    [onNodeClick]
  );

  return (
    <GraphScene
      ref={sceneRef}
      data={normalizedData}
      onNodeClick={handleNodeClick}
      onBackgroundClick={onBackgroundClick}
      selectedEntity={selectedEntity}
    />
  );
});

export default Graph3D;
