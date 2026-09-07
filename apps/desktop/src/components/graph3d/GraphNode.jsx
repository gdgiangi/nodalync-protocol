/**
 * GraphNode — Individual 3D node with glow, hover pulse, and label.
 */
import { useRef, useMemo, useCallback } from "react";
import { useFrame, useThree } from "@react-three/fiber";
import { Html } from "@react-three/drei";
import * as THREE from "three";
import { LAYER_COLORS } from "./GraphScene";
import { getEntityColor } from "../../lib/constants";

// Node sizes by level
const NODE_SIZE = {
  L0: 0.7,
  L2: 1.05,
  L3: 1.15,
};

export function GraphNode({
  node,
  position,
  level,
  isSelected,
  isHovered,
  isDimmed,
  zoomLevel = 1.0,
  inCluster = false,
  onClick,
  onHover,
  onUnhover,
  nodeObjectsRef,
}) {
  const meshRef = useRef();
  const glowRef = useRef();
  const { camera, gl, size: canvasSize } = useThree();
  const worldPosition = useRef(new THREE.Vector3());
  const setMeshRef = useCallback((mesh) => {
    meshRef.current = mesh;
    if (mesh) nodeObjectsRef?.current.set(node.id, mesh);
    else nodeObjectsRef?.current.delete(node.id);
  }, [node.id, nodeObjectsRef]);
  const pulsePhase = useMemo(() => Array.from(String(node.id))
    .reduce((value, character) => (value * 31 + character.charCodeAt(0)) % 360, 0), [node.id]);


  // Unified L2 color scheme - amber/gold base with subtle entity type hints
  const color = useMemo(() => {
    if (level === "L2") {
      const baseColor = new THREE.Color("#f59e0b"); // Amber/gold base
      if (node.entity_type) {
        const entityColor = new THREE.Color(getEntityColor(node.entity_type));
        // Very subtle tint - only 8% of entity color mixed in
        baseColor.lerp(entityColor, 0.08);
      }
      return baseColor;
    }
    return LAYER_COLORS[level] || LAYER_COLORS.L2;
  }, [level, node.entity_type]);

  // Keep an accessible visual and pointer target at the initial overview distance.
  const size = NODE_SIZE[level] || 1.05;

  // Animate: pulse on hover, breathe gently
  useFrame(({ clock }) => {
    if (!meshRef.current) return;

    const t = clock.getElapsedTime();

    // Gentle breathing
    const breathe = 1 + Math.sin(t * 1.5 + pulsePhase * 0.01745) * 0.03;

    // Hover pulse
    const hoverScale = isHovered ? 1.4 : isSelected ? 1.2 : 1.0;
    meshRef.current.getWorldPosition(worldPosition.current);
    const distance = camera.position.distanceTo(worldPosition.current);
    const worldPerPixel = 2 * distance * Math.tan(THREE.MathUtils.degToRad(camera.fov) / 2)
      / Math.max(canvasSize.height, 1);
    const targetScale = worldPerPixel * 7 * hoverScale * breathe;

    // Smooth interpolation
    const currentScale = meshRef.current.scale.x;
    const newScale = THREE.MathUtils.lerp(currentScale, targetScale, 0.1);
    meshRef.current.scale.setScalar(newScale);

    // Emissive intensity
    const mat = meshRef.current.material;
    if (mat) {
      // Opacity for dimming and zoom-based visibility
      let targetOpacity = isDimmed ? 0.25 : 1.0;
      targetOpacity *= nodeOpacity; // Apply zoom-based visibility
      mat.opacity = THREE.MathUtils.lerp(mat.opacity, targetOpacity, 0.1);
    }

    // Glow sphere
    if (glowRef.current) {
      const glowScale = newScale * (isHovered ? 3.5 : isSelected ? 3.0 : 2.2);
      glowRef.current.scale.setScalar(glowScale);
      const glowMat = glowRef.current.material;
      if (glowMat) {
        let targetGlowOpacity = isHovered
          ? 0.25
          : isSelected
          ? 0.18
          : isDimmed
          ? 0.02
          : 0.08;
        targetGlowOpacity *= nodeOpacity; // Apply zoom-based visibility
        glowMat.opacity = THREE.MathUtils.lerp(
          glowMat.opacity,
          targetGlowOpacity,
          0.08
        );
      }
    }
  });

  const handleClick = useCallback(
    (e) => {
      e.stopPropagation();
      onClick?.(node);
    },
    [node, onClick]
  );

  const handlePointerOver = useCallback(
    (e) => {
      e.stopPropagation();
      gl.domElement.style.cursor = "pointer";

      // Get screen position for tooltip
      const vec = meshRef.current
        ? meshRef.current.getWorldPosition(new THREE.Vector3())
        : new THREE.Vector3(...position);
      vec.project(camera);
      const x = (vec.x * 0.5 + 0.5) * gl.domElement.clientWidth;
      const y = (-vec.y * 0.5 + 0.5) * gl.domElement.clientHeight;

      onHover?.(node, { x, y });
    },
    [node, position, camera, gl, onHover]
  );

  const handlePointerOut = useCallback(
    (e) => {
      e.stopPropagation();
      gl.domElement.style.cursor = "default";
      onUnhover?.();
    },
    [gl, onUnhover]
  );

  // Overview still shows every real entity. Zoom changes emphasis, not existence.
  const nodeOpacity = inCluster && level === "L2" && zoomLevel > 0.7 ? 0.85 : 1;

  const label = node.label || node.canonical_label || "";
  const truncLabel = label.length > 20 ? label.substring(0, 17) + "…" : label;

  return (
    <group position={position}>
      {/* Outer glow sphere */}
      <mesh ref={glowRef}>
        <sphereGeometry args={[1, 16, 16]} />
        <meshBasicMaterial
          color={color}
          transparent
          opacity={0.08}
          depthWrite={false}
          blending={THREE.AdditiveBlending}
        />
      </mesh>

      {/* Core node sphere */}
      <mesh
        ref={setMeshRef}
        onClick={handleClick}
        onPointerOver={handlePointerOver}
        onPointerOut={handlePointerOut}
        scale={size}
      >
        <sphereGeometry args={[1, 24, 24]} />
        <meshBasicMaterial color={isSelected ? "#ffffff" : level === "L2" ? "#ffce72" : color}
          transparent opacity={1} toneMapped={false} depthTest={false} />
      </mesh>

      {/* Selection ring */}
      {isSelected && (
        <mesh rotation={[Math.PI / 2, 0, 0]} scale={size * 2}>
          <ringGeometry args={[1.0, 1.15, 32]} />
          <meshBasicMaterial
            color="#ffffff"
            transparent
            opacity={0.4}
            side={THREE.DoubleSide}
            depthWrite={false}
          />
        </mesh>
      )}

      {/* Local, accessible labels remain usable at every zoom level. */}
      <Html position={[0, 0, 0]} center zIndexRange={[5, 0]} style={{ pointerEvents: "auto", userSelect: "none" }}>
        <button type="button" data-graph-entity={node.id} aria-label={`Select ${label || "entity"}`}
          aria-pressed={Boolean(isSelected)} title={label}
          onClick={(event) => { event.stopPropagation(); onClick?.(node); }}
          onMouseEnter={handlePointerOver} onMouseLeave={handlePointerOut}
          onFocus={(event) => { event.currentTarget.style.outline = "2px solid #ffd486"; handlePointerOver(event); }}
          onBlur={(event) => { event.currentTarget.style.outline = "none"; handlePointerOut(event); }}
          style={{
            display: "block", transform: "translateY(22px)", cursor: "pointer",
            color: isSelected ? "#ffffff" : "#e5edf7", fontSize: 12, lineHeight: "18px",
            maxWidth: 125, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
            textShadow: "0 1px 3px #000", padding: "3px 7px", borderRadius: 5,
            border: isSelected ? "1px solid #ffd486" : "1px solid #42516a",
            background: isSelected ? "#303040" : "#121b2c", opacity: isDimmed ? 0.6 : 1,
          }}>{truncLabel || "Untitled entity"}</button>
      </Html>
    </group>
  );
}
