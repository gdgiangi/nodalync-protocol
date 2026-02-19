/**
 * GraphNode — Individual 3D node with glow, hover pulse, and label.
 */
import { useRef, useState, useMemo, useCallback } from "react";
import { useFrame, useThree } from "@react-three/fiber";
import { Html, Billboard, Text } from "@react-three/drei";
import * as THREE from "three";
import { LAYER_COLORS, LAYER_HEX } from "./GraphScene";
import { getEntityColor } from "../../lib/constants";

// Node sizes by level
const NODE_SIZE = {
  L0: 0.35,
  L2: 0.6,
  L3: 0.5,
};

// Emissive intensities
const EMISSIVE_BASE = {
  L0: 0.6,
  L2: 1.2,
  L3: 0.9,
};

export function GraphNode({
  node,
  position,
  level,
  isSelected,
  isHovered,
  isConnectedToHover,
  isDimmed,
  zoomLevel = 1.0,
  inCluster = false,
  onClick,
  onHover,
  onUnhover,
  controlsRef,
}) {
  const meshRef = useRef();
  const glowRef = useRef();
  const { camera, gl } = useThree();

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

  const hexColor = useMemo(() => {
    if (level === "L2") {
      const baseHex = "#f59e0b";
      if (node.entity_type) {
        // For hex, we'll use the base amber with slight adjustment
        const entityHex = getEntityColor(node.entity_type);
        // Simple blend - in practice this would use the Three.js color above
        return baseHex; // Keep it simple for hex version
      }
      return baseHex;
    }
    return LAYER_HEX[level] || LAYER_HEX.L2;
  }, [level, node.entity_type]);

  // Adaptive sizing based on zoom level and cluster membership
  const baseSize = NODE_SIZE[level] || 0.5;
  const size = useMemo(() => {
    let adjustedSize = baseSize;
    
    if (inCluster && level === "L2") {
      // Nodes in clusters are smaller
      adjustedSize *= 0.7;
      
      // Fade in with zoom - invisible when far away, visible when close
      if (zoomLevel > 0.6) {
        adjustedSize *= 0.3; // Very small when far
      } else if (zoomLevel > 0.3) {
        adjustedSize *= (1 - zoomLevel) + 0.3; // Smooth transition
      }
      // Full size when zoomed in (zoomLevel < 0.3)
    }
    
    return adjustedSize;
  }, [baseSize, inCluster, level, zoomLevel]);

  const emissiveBase = EMISSIVE_BASE[level] || 1.0;

  // Animate: pulse on hover, breathe gently
  useFrame(({ clock }) => {
    if (!meshRef.current) return;

    const t = clock.getElapsedTime();

    // Gentle breathing
    const breathe = 1 + Math.sin(t * 1.5 + node.id * 0.7) * 0.03;

    // Hover pulse
    const hoverScale = isHovered ? 1.4 : isSelected ? 1.2 : 1.0;
    const targetScale = size * hoverScale * breathe;

    // Smooth interpolation
    const currentScale = meshRef.current.scale.x;
    const newScale = THREE.MathUtils.lerp(currentScale, targetScale, 0.1);
    meshRef.current.scale.setScalar(newScale);

    // Emissive intensity
    const mat = meshRef.current.material;
    if (mat) {
      const targetIntensity = isHovered
        ? emissiveBase * 2.5
        : isSelected
        ? emissiveBase * 2.0
        : isDimmed
        ? emissiveBase * 0.2
        : emissiveBase;
      mat.emissiveIntensity = THREE.MathUtils.lerp(
        mat.emissiveIntensity,
        targetIntensity,
        0.1
      );

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

      // Smooth camera transition to node
      if (controlsRef?.current) {
        const controls = controlsRef.current;
        const targetPos = new THREE.Vector3(...position);

        // Animate target
        const startTarget = controls.target.clone();
        const startTime = performance.now();
        const duration = 800;

        function animate() {
          const elapsed = performance.now() - startTime;
          const t = Math.min(elapsed / duration, 1);
          const eased = 1 - Math.pow(1 - t, 3); // ease out cubic

          controls.target.lerpVectors(startTarget, targetPos, eased);
          controls.update();

          if (t < 1) requestAnimationFrame(animate);
        }
        animate();
      }
    },
    [node, onClick, position, controlsRef]
  );

  const handlePointerOver = useCallback(
    (e) => {
      e.stopPropagation();
      gl.domElement.style.cursor = "pointer";

      // Get screen position for tooltip
      const vec = new THREE.Vector3(...position);
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

  // Zoom-based visibility for clustered nodes
  const nodeOpacity = useMemo(() => {
    if (!inCluster) return 1.0; // Non-clustered nodes always visible
    
    if (level === "L2") {
      if (zoomLevel > 0.7) return 0.0; // Hidden when far away
      if (zoomLevel > 0.4) return (0.7 - zoomLevel) / 0.3; // Fade in
      return 1.0; // Fully visible when close
    }
    
    return 1.0; // L0/L3 always visible
  }, [inCluster, level, zoomLevel]);

  const label = node.label || node.canonical_label || "";
  const truncLabel = label.length > 20 ? label.substring(0, 17) + "…" : label;

  // Don't render if completely invisible
  if (nodeOpacity <= 0.01) {
    return null;
  }

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
        ref={meshRef}
        onClick={handleClick}
        onPointerOver={handlePointerOver}
        onPointerOut={handlePointerOut}
        scale={size}
      >
        <sphereGeometry args={[1, 24, 24]} />
        <meshStandardMaterial
          color={color}
          emissive={color}
          emissiveIntensity={emissiveBase}
          transparent
          opacity={1}
          roughness={0.3}
          metalness={0.1}
          toneMapped={false}
        />
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

      {/* Text label */}
      {!isDimmed && nodeOpacity > 0.3 && (
        <Billboard follow={true} lockX={false} lockY={false} lockZ={false}>
          <Text
            position={[0, -(size + 0.6), 0]}
            fontSize={0.35}
            color={isHovered || isSelected ? "#ffffff" : "rgba(255,255,255,0.55)"}
            anchorX="center"
            anchorY="top"
            outlineWidth={0.02}
            outlineColor="#000000"
            font={undefined}
            maxWidth={8}
          >
            {truncLabel}
          </Text>
        </Billboard>
      )}
    </group>
  );
}
