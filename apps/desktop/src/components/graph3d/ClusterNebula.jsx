/**
 * ClusterNebula — Renders a knowledge cluster as a glowing nebula/cloud
 * Each cluster is visualized as a collection of tiny particles forming a nebula
 * Size reflects node count, brightness reflects edge density, color reflects dominant entity type
 */
import { useRef, useMemo, useEffect } from "react";
import { useFrame } from "@react-three/fiber";
import * as THREE from "three";
import { getEntityColor } from "../../lib/constants";

// Base amber/gold color for all L2 clusters
const BASE_L2_COLOR = new THREE.Color("#f59e0b");

export function ClusterNebula({
  clusterId,
  nodes,
  centroid,
  metrics,
  level = "L2",
  isHovered = false,
  isDimmed = false,
  zoomLevel = 1.0,
  onClick,
  onHover,
  onUnhover,
  children
}) {
  const groupRef = useRef();
  const nebulaRef = useRef();
  const coreRef = useRef();
  const labelRef = useRef();

  // Calculate nebula properties from cluster metrics
  const { size, brightness, color, particleCount } = useMemo(() => {
    const nodeCount = metrics.nodeCount || 1;
    const density = metrics.density || 0;
    const dominantType = metrics.dominantEntityType || 'Concept';

    // Size based on node count (2-15 units)
    const baseSize = Math.sqrt(nodeCount) * 1.5 + 2;
    const size = Math.min(baseSize, 15);

    // Brightness based on edge density (0.3-1.5)
    const brightness = 0.3 + (density * 1.2);

    // Color: amber/gold base with subtle entity type tint
    const entityColor = new THREE.Color(getEntityColor(dominantType));
    const color = BASE_L2_COLOR.clone();
    color.lerp(entityColor, 0.15); // Very subtle tint

    // Particle count for nebula effect (50-500 particles)
    const particleCount = Math.floor(nodeCount * 8 + Math.random() * nodeCount * 4 + 50);

    return { size, brightness, color, particleCount };
  }, [metrics]);

  // Generate nebula particle positions
  const nebulaGeometry = useMemo(() => {
    const positions = new Float32Array(particleCount * 3);
    const colors = new Float32Array(particleCount * 3);
    const sizes = new Float32Array(particleCount);

    for (let i = 0; i < particleCount; i++) {
      // Random position within sphere with denser core
      const radius = Math.pow(Math.random(), 1.5) * size;
      const theta = Math.random() * Math.PI * 2;
      const phi = Math.acos(2 * Math.random() - 1);

      positions[i * 3] = radius * Math.sin(phi) * Math.cos(theta);
      positions[i * 3 + 1] = radius * Math.sin(phi) * Math.sin(theta);
      positions[i * 3 + 2] = radius * Math.cos(phi);

      // Color variation with some randomness
      const variation = 0.8 + Math.random() * 0.4;
      colors[i * 3] = color.r * variation;
      colors[i * 3 + 1] = color.g * variation;
      colors[i * 3 + 2] = color.b * variation;

      // Size variation (0.5-3.0)
      sizes[i] = 0.5 + Math.random() * 2.5;
    }

    return { positions, colors, sizes };
  }, [particleCount, size, color]);

  // Animation and interaction
  useFrame(({ clock }) => {
    const t = clock.getElapsedTime();

    if (groupRef.current) {
      // Gentle rotation
      groupRef.current.rotation.y += 0.001;
      
      // Breathing effect
      const breathe = 1 + Math.sin(t * 0.5 + clusterId.charCodeAt(0) * 0.1) * 0.03;
      groupRef.current.scale.setScalar(breathe);
    }

    if (nebulaRef.current) {
      const material = nebulaRef.current.material;
      if (material) {
        // Opacity based on zoom and hover state
        let targetOpacity;
        if (zoomLevel < 0.3) {
          // Far away: nebula very visible, nodes hidden
          targetOpacity = isHovered ? 0.8 : isDimmed ? 0.15 : 0.4;
        } else if (zoomLevel < 0.7) {
          // Medium distance: both visible
          targetOpacity = isHovered ? 0.6 : isDimmed ? 0.1 : 0.25;
        } else {
          // Close up: nebula faint, nodes prominent
          targetOpacity = isHovered ? 0.3 : isDimmed ? 0.05 : 0.1;
        }

        material.opacity = THREE.MathUtils.lerp(material.opacity, targetOpacity, 0.05);
        
        // Brightness modulation
        const glimmer = 1 + Math.sin(t * 2 + clusterId.length * 0.3) * 0.1;
        material.uniforms.brightness.value = brightness * glimmer * (isHovered ? 1.5 : 1.0);
      }
    }

    if (coreRef.current) {
      // Core sphere brightness
      const material = coreRef.current.material;
      if (material) {
        const targetIntensity = isHovered ? brightness * 0.8 : isDimmed ? brightness * 0.1 : brightness * 0.3;
        material.emissiveIntensity = THREE.MathUtils.lerp(
          material.emissiveIntensity, 
          targetIntensity, 
          0.08
        );
      }
    }
  });

  const handleClick = (e) => {
    e.stopPropagation();
    onClick?.(clusterId, nodes);
  };

  const handlePointerOver = (e) => {
    e.stopPropagation();
    onHover?.(clusterId, nodes, metrics);
  };

  const handlePointerOut = (e) => {
    e.stopPropagation();
    onUnhover?.();
  };

  return (
    <group 
      ref={groupRef} 
      position={centroid}
      onClick={handleClick}
      onPointerOver={handlePointerOver}
      onPointerOut={handlePointerOut}
    >
      {/* Nebula particles */}
      <points ref={nebulaRef}>
        <bufferGeometry>
          <bufferAttribute
            attach="attributes-position"
            array={nebulaGeometry.positions}
            count={particleCount}
            itemSize={3}
          />
          <bufferAttribute
            attach="attributes-color"
            array={nebulaGeometry.colors}
            count={particleCount}
            itemSize={3}
          />
          <bufferAttribute
            attach="attributes-size"
            array={nebulaGeometry.sizes}
            count={particleCount}
            itemSize={1}
          />
        </bufferGeometry>
        <shaderMaterial
          transparent
          depthWrite={false}
          blending={THREE.AdditiveBlending}
          vertexColors
          uniforms={{
            brightness: { value: brightness }
          }}
          vertexShader={`
            attribute float size;
            varying vec3 vColor;
            uniform float brightness;
            
            void main() {
              vColor = color * brightness;
              vec4 mvPosition = modelViewMatrix * vec4(position, 1.0);
              gl_PointSize = size * (300.0 / -mvPosition.z);
              gl_Position = projectionMatrix * mvPosition;
            }
          `}
          fragmentShader={`
            varying vec3 vColor;
            
            void main() {
              float dist = distance(gl_PointCoord, vec2(0.5));
              if (dist > 0.5) discard;
              
              float alpha = 1.0 - (dist * 2.0);
              alpha = smoothstep(0.0, 1.0, alpha);
              alpha *= alpha; // More falloff
              
              gl_FragColor = vec4(vColor, alpha * 0.6);
            }
          `}
        />
      </points>

      {/* Core sphere for cluster center */}
      <mesh ref={coreRef}>
        <sphereGeometry args={[size * 0.15, 16, 16]} />
        <meshBasicMaterial
          color={color}
          emissive={color}
          emissiveIntensity={brightness * 0.3}
          transparent
          opacity={0.6}
          depthWrite={false}
        />
      </mesh>

      {/* Individual nodes (fade in on zoom) */}
      <group>
        {children}
      </group>

      {/* Cluster label */}
      {zoomLevel < 0.8 && (
        <group position={[0, -size * 0.8, 0]}>
          <mesh>
            <planeGeometry args={[size * 1.2, 0.8]} />
            <meshBasicMaterial
              color="#000000"
              transparent
              opacity={0.3}
              depthWrite={false}
            />
          </mesh>
          {/* Text will be added via HTML overlay or THREE.Text */}
        </group>
      )}
    </group>
  );
}