import { useEffect, useMemo, useRef, useState } from "react";
import { Canvas, useFrame } from "@react-three/fiber";
import {
    OrthographicCamera,
    OrbitControls,
    Html,
    Line,
    RoundedBox,
    Edges,
    Text,
} from "@react-three/drei";
import * as THREE from "three";
import { motion, AnimatePresence } from "framer-motion";
import { invoke } from "@tauri-apps/api/core";
import {
    ShieldCheck,
    Cpu,
    Lock,
    Fingerprint,
    Layers,
    Network,
} from "lucide-react";

const THEME = {
    bg: "#09090B",
    grid: "#27272A",
    l0_slate: "#18181B",
    l0_accent: "#3B82F6",
    l1_cyan: "#06B6D4",
    l2_glass: "#F59E0B",
    l3_apex: "#A855F7",
    trace_idle: "#3F3F46",
    flow: "#10B981",
};

const Y_LEVELS = { L0: 0, L1: 2, L2: 6, L3: 10 };
const L3_TYPES = new Set(["Insight", "Pattern", "Decision", "Goal", "Metric", "Value"]);
const FOCUS_LAYERS = ["ALL", "L0", "L2", "L3"];

const DEMO_PROTOCOL_DATA = {
    nodes: [
        { id: "l0_a1", label: "Primary Research", entity_type: "Document", source_count: 8, confidence: 1, layer: "L0", x: -6, z: -2, owner: "Alice", hash: "0x8f2...a1" },
        { id: "l0_a2", label: "Dataset Beta", entity_type: "Dataset", source_count: 6, confidence: 1, layer: "L0", x: -2, z: -2, owner: "Alice", hash: "0x3c9...b2" },
        { id: "l0_c1", label: "Field Notes", entity_type: "Document", source_count: 5, confidence: 1, layer: "L0", x: 2, z: -2, owner: "Carol", hash: "0x7e1...c3" },
        { id: "l0_b1", label: "Lab Transcript", entity_type: "Document", source_count: 7, confidence: 1, layer: "L0", x: 6, z: -2, owner: "Bob", hash: "0x1a4...d4" },
        { id: "l2_1", label: "Entity Graph (A+C)", entity_type: "Concept", source_count: 3, confidence: 0.9, layer: "L2", x: -3, z: 2, owner: "Bob", hash: "0x555...l2" },
        { id: "l2_2", label: "Entity Graph (B)", entity_type: "Concept", source_count: 3, confidence: 0.9, layer: "L2", x: 4, z: 2, owner: "Bob", hash: "0x666...l2" },
        { id: "l3_1", label: "Market Inefficiency Thesis", entity_type: "Insight", source_count: 4, confidence: 0.95, layer: "L3", x: 0, z: 4, owner: "Bob", hash: "0xff9...cc" },
    ],
    links: [
        { id: "f1", source: "l3_1", target: "l2_1", predicate: "synthesizedFrom", confidence: 1 },
        { id: "f2", source: "l3_1", target: "l2_2", predicate: "synthesizedFrom", confidence: 1 },
        { id: "f3", source: "l2_1", target: "l0_a1", predicate: "rootedIn", confidence: 1 },
        { id: "f4", source: "l2_1", target: "l0_a2", predicate: "rootedIn", confidence: 1 },
        { id: "f5", source: "l2_1", target: "l0_c1", predicate: "rootedIn", confidence: 1 },
        { id: "f6", source: "l2_2", target: "l0_b1", predicate: "rootedIn", confidence: 1 },
    ],
};

const DEMO_L0_ITEMS = [
    { content_id: "c_a1", content_hash: "0x8f2...a1", content_type: "L0", title: "Primary Research" },
    { content_id: "c_a2", content_hash: "0x3c9...b2", content_type: "L0", title: "Dataset Beta" },
    { content_id: "c_c1", content_hash: "0x7e1...c3", content_type: "L0", title: "Field Notes" },
    { content_id: "c_b1", content_hash: "0x1a4...d4", content_type: "L0", title: "Lab Transcript" },
];

const DEMO_L0_EDGES = [
    { entity_id: "l2_1", content_id: "c_a1", content_hash: "0x8f2...a1" },
    { entity_id: "l2_1", content_id: "c_a2", content_hash: "0x3c9...b2" },
    { entity_id: "l2_1", content_id: "c_c1", content_hash: "0x7e1...c3" },
    { entity_id: "l2_2", content_id: "c_b1", content_hash: "0x1a4...d4" },
];

function hashPreview(value) {
    const text = String(value ?? "node");
    if (text.length <= 12) {
        return text;
    }
    return `${text.slice(0, 6)}...${text.slice(-4)}`;
}

function seededUnit(value) {
    const text = String(value || "seed");
    let hash = 2166136261;
    for (let i = 0; i < text.length; i++) {
        hash ^= text.charCodeAt(i);
        hash = Math.imul(hash, 16777619);
    }
    return (hash >>> 0) / 4294967295;
}

function inferLayer(node) {
    if (L3_TYPES.has(node.entity_type)) {
        return "L3";
    }
    return "L2";
}

function getNodeAnchor(node, direction = "center") {
    const yBase = Y_LEVELS[node.layer] + (node.yOffset || 0);

    if (node.layer === "L0") {
        const y = yBase;
        if (direction === "up") return new THREE.Vector3(node.x, y + 0.62, node.z);
        if (direction === "down") return new THREE.Vector3(node.x, y + 0.02, node.z);
        return new THREE.Vector3(node.x, y + 0.3, node.z);
    }
    if (node.layer === "L2") {
        const y = yBase;
        if (direction === "up") return new THREE.Vector3(node.x, y + 0.95, node.z);
        if (direction === "down") return new THREE.Vector3(node.x, y + 0.02, node.z);
        return new THREE.Vector3(node.x, y + 0.4, node.z);
    }
    if (node.layer === "L3") {
        const y = yBase;
        if (direction === "up") return new THREE.Vector3(node.x, y + 2.1, node.z);
        if (direction === "down") return new THREE.Vector3(node.x, y + 0.2, node.z);
        return new THREE.Vector3(node.x, y + 1, node.z);
    }

    return new THREE.Vector3(node.x, yBase, node.z);
}

function positionLayerNodes(nodes, layer) {
    const sortNodes = [...nodes].sort(
        (a, b) => (b.source_count || 0) - (a.source_count || 0)
    );

    const visibleNodes = sortNodes.slice(0, 28);

    const config = {
        L0: { cols: 4, sx: 4.2, sz: 2.8, oz: -4.2 },
        L1: { cols: 5, sx: 3.4, sz: 2.4, oz: -0.8 },
        L2: { cols: 5, sx: 3.7, sz: 2.6, oz: 2.4 },
        L3: { cols: 3, sx: 4.8, sz: 3.0, oz: 6.4 },
    }[layer];

    return visibleNodes.map((node, index) => {
        const col = index % config.cols;
        const row = Math.floor(index / config.cols);
        const rows = Math.ceil(visibleNodes.length / config.cols);
        const x = (col - (config.cols - 1) / 2) * config.sx;
        const z = (row - (rows - 1) / 2) * config.sz + config.oz;
        return { ...node, layer, x, z };
    });
}

function positionL2ByRelation(l2Nodes, links, allNodeIds, spread = 1) {
    const nodes = [...l2Nodes]
        .sort((a, b) => (b.source_count || 0) - (a.source_count || 0))
        .slice(0, 28);

    if (nodes.length <= 1) {
        return {
            nodes: nodes.map((node) => ({ ...node, layer: "L2", x: 0, z: 2.4 })),
            graphLinks: [],
        };
    }

    const nodeIds = new Set(nodes.map((node) => node.id));
    const neighbors = new Map(nodes.map((node) => [node.id, new Set()]));

    for (const link of links) {
        if (allNodeIds.has(link.source) && allNodeIds.has(link.target)) {
            if (nodeIds.has(link.source)) {
                neighbors.get(link.source)?.add(link.target);
            }
            if (nodeIds.has(link.target)) {
                neighbors.get(link.target)?.add(link.source);
            }
        }
    }

    const ids = nodes.map((node) => node.id);
    const relationScore = new Map();
    const l2GraphLinks = [];
    for (let i = 0; i < ids.length; i++) {
        for (let j = i + 1; j < ids.length; j++) {
            const a = ids[i];
            const b = ids[j];
            const aNeighbors = neighbors.get(a) || new Set();
            const bNeighbors = neighbors.get(b) || new Set();
            let shared = 0;
            for (const n of aNeighbors) {
                if (bNeighbors.has(n)) {
                    shared += 1;
                }
            }
            const direct = aNeighbors.has(b) || bNeighbors.has(a) ? 3 : 0;
            const score = direct + shared;
            relationScore.set(`${a}|${b}`, score);
            if (score >= 2) {
                l2GraphLinks.push({ source: a, target: b, weight: score });
            }
        }
    }

    if (l2GraphLinks.length === 0) {
        const ranked = [];
        for (let i = 0; i < ids.length; i++) {
            for (let j = i + 1; j < ids.length; j++) {
                const a = ids[i];
                const b = ids[j];
                const key = `${a}|${b}`;
                ranked.push({ source: a, target: b, weight: relationScore.get(key) || 0 });
            }
        }
        ranked
            .sort((a, b) => b.weight - a.weight)
            .slice(0, Math.min(14, Math.max(4, Math.floor(nodes.length * 0.8))))
            .forEach((edge) => l2GraphLinks.push({ ...edge, weight: Math.max(edge.weight, 1) }));
    }

    const positions = new Map();
    const velocities = new Map();

    const spreadScale = Math.max(0.6, Math.min(2.0, spread));

    ids.forEach((id) => {
        const rx = (seededUnit(`${id}-x`) * 10 - 5) * spreadScale;
        const rz = (seededUnit(`${id}-z`) * 7 - 3.5) * spreadScale;
        positions.set(id, {
            x: rx,
            z: 2.4 + rz,
        });
        velocities.set(id, { x: 0, z: 0 });
    });

    const iterations = 180;
    const repulsion = 0.03 * spreadScale;
    const damping = 0.84;
    const attractBase = 0.01;
    const centerPull = 0.003;

    for (let iter = 0; iter < iterations; iter++) {
        const forces = new Map(ids.map((id) => [id, { x: 0, z: 0 }]));

        for (let i = 0; i < ids.length; i++) {
            for (let j = i + 1; j < ids.length; j++) {
                const a = ids[i];
                const b = ids[j];
                const pa = positions.get(a);
                const pb = positions.get(b);
                const dx = pa.x - pb.x;
                const dz = pa.z - pb.z;
                const dist2 = Math.max(0.18, dx * dx + dz * dz);
                const dist = Math.sqrt(dist2);

                const repel = repulsion / dist2;
                const rx = (dx / dist) * repel;
                const rz = (dz / dist) * repel;
                forces.get(a).x += rx;
                forces.get(a).z += rz;
                forces.get(b).x -= rx;
                forces.get(b).z -= rz;

                const key = a < b ? `${a}|${b}` : `${b}|${a}`;
                const score = relationScore.get(key) || 0;
                if (score > 0) {
                    const desired = Math.max(1.2 * spreadScale, (3.5 - Math.min(2.2, score * 0.42)) * spreadScale);
                    const delta = dist - desired;
                    const attract = attractBase * score * delta;
                    const ax = (dx / dist) * attract;
                    const az = (dz / dist) * attract;
                    forces.get(a).x -= ax;
                    forces.get(a).z -= az;
                    forces.get(b).x += ax;
                    forces.get(b).z += az;
                }
            }
        }

        for (const id of ids) {
            const p = positions.get(id);
            forces.get(id).x += -p.x * centerPull;
            forces.get(id).z += -(p.z - 2.4) * centerPull;
        }

        for (const id of ids) {
            const force = forces.get(id);
            const velocity = velocities.get(id);
            const position = positions.get(id);

            velocity.x = (velocity.x + force.x) * damping;
            velocity.z = (velocity.z + force.z) * damping;
            position.x += velocity.x;
            position.z += velocity.z;

            const clampX = 10 * spreadScale;
            const clampZMin = -4.2 * spreadScale;
            const clampZMax = 9.2 * spreadScale;
            position.x = Math.max(-clampX, Math.min(clampX, position.x));
            position.z = Math.max(clampZMin, Math.min(clampZMax, position.z));
        }
    }

    const centroid = ids.reduce(
        (acc, id) => {
            const p = positions.get(id);
            acc.x += p.x;
            acc.z += p.z;
            return acc;
        },
        { x: 0, z: 0 }
    );
    centroid.x /= ids.length;
    centroid.z /= ids.length;

    return {
        nodes: nodes.map((node) => {
            const p = positions.get(node.id);
            return {
                ...node,
                layer: "L2",
                x: p.x - centroid.x,
                z: p.z - centroid.z + 2.4,
            };
        }),
        graphLinks: l2GraphLinks,
    };
}

function OrthogonalCircuit({ start, end, isFlowing }) {
    const lineRef = useRef(null);
    const particleRef = useRef(null);

    const points = useMemo(() => {
        const midY = end.y + 0.5;
        return [
            start,
            new THREE.Vector3(start.x, midY, start.z),
            new THREE.Vector3(end.x, midY, end.z),
            end,
        ];
    }, [start, end]);

    useFrame((state) => {
        if (!lineRef.current?.material) {
            return;
        }

        const targetColor = isFlowing ? THEME.flow : THEME.trace_idle;
        const targetOpacity = isFlowing ? 0.8 : 0.3;
        lineRef.current.material.color.lerp(new THREE.Color(targetColor), 0.1);
        lineRef.current.material.opacity = THREE.MathUtils.lerp(
            lineRef.current.material.opacity,
            targetOpacity,
            0.1
        );

        if (isFlowing && particleRef.current) {
            const t = 1 - ((state.clock.elapsedTime * 1.2) % 1);
            const [p1, p2, p3, p4] = points;
            const d1 = p1.distanceTo(p2);
            const d2 = p2.distanceTo(p3);
            const d3 = p3.distanceTo(p4);
            const total = d1 + d2 + d3;
            const d = t * total;

            if (d <= d1) {
                particleRef.current.position.lerpVectors(p1, p2, d / d1);
            } else if (d <= d1 + d2) {
                particleRef.current.position.lerpVectors(p2, p3, (d - d1) / d2);
            } else {
                particleRef.current.position.lerpVectors(p3, p4, Math.min(1, (d - d1 - d2) / d3));
            }
        }
    });

    return (
        <group>
            <Line ref={lineRef} points={points} color={THEME.trace_idle} transparent />
            {isFlowing && (
                <mesh ref={particleRef}>
                    <sphereGeometry args={[0.15, 8, 8]} />
                    <meshBasicMaterial color={THEME.flow} />
                </mesh>
            )}
        </group>
    );
}

function LayerGraphLink({ start, end, weight = 1 }) {
    const points = useMemo(() => [start, end], [start, end]);
    const opacity = Math.min(0.65, 0.15 + weight * 0.08);

    return (
        <Line
            points={points}
            color="#f59e0b"
            transparent
            opacity={opacity}
        />
    );
}

function LayerPlane({ y, color, label, isPrivate = false }) {
    return (
        <group position={[0, y, 0]}>
            <mesh rotation={[-Math.PI / 2, 0, 0]}>
                <planeGeometry args={[24, 16]} />
                <meshPhysicalMaterial
                    color={color}
                    transparent
                    opacity={isPrivate ? 0.05 : 0.02}
                    depthWrite={false}
                    side={THREE.DoubleSide}
                />
                <Edges color={color} transparent opacity={0.2} />
            </mesh>
            <gridHelper args={[24, 24, color, color]} position={[0, 0.01, 0]} />
            <Text
                position={[-11.5, 0.2, 7.5]}
                rotation={[-Math.PI / 2, 0, 0]}
                color={color}
                fontSize={0.38}
                anchorX="left"
                anchorY="bottom"
                fillOpacity={0.45}
            >
                {label}
            </Text>
            {isPrivate && (
                <Line
                    points={[
                        [-12, 0.1, -8],
                        [12, 0.1, -8],
                        [12, 0.1, 8],
                        [-12, 0.1, 8],
                        [-12, 0.1, -8],
                    ]}
                    color="#ef4444"
                    dashed
                    dashScale={5}
                    dashSize={1}
                    transparent
                    opacity={0.5}
                />
            )}
        </group>
    );
}

function ProtocolNode({
    node,
    isQueryActive,
    isQueryTarget,
    tokenSplit,
    denseMode,
    showLabel,
    onSelect,
    isSelected,
    showL3Card,
}) {
    const isL0 = node.layer === "L0";
    const isL3 = node.layer === "L3";
    const isReceivingValue = isQueryActive && isL0 && tokenSplit > 0;
    const renderLabel = !denseMode || showLabel || isReceivingValue || isQueryTarget;

    return (
        <group position={[node.x, Y_LEVELS[node.layer] + (node.yOffset || 0), node.z]}>
            {isL0 && (
                <group position={[0, 0.3, 0]}>
                    <RoundedBox args={[2.5, 0.6, 2.5]} radius={0.05} smoothness={4} onClick={() => onSelect?.(node)}>
                        <meshStandardMaterial
                            color={THEME.l0_slate}
                            roughness={0.7}
                            metalness={0.8}
                            emissive={THEME.flow}
                            emissiveIntensity={isReceivingValue ? 0.5 : isSelected ? 0.25 : 0}
                        />
                        <Edges scale={1.01} color={isReceivingValue ? THEME.flow : isSelected ? "#f8fafc" : THEME.grid} />
                    </RoundedBox>
                    {renderLabel && (
                        <Html
                            transform
                            rotation={[-Math.PI / 2, 0, 0]}
                            position={[0, 0.31, 0]}
                            style={{ pointerEvents: "none" }}
                            occlude
                        >
                            <div className="w-32 text-center transition-opacity duration-300" style={{ opacity: isReceivingValue ? 1 : 0.6 }}>
                                <div className="text-[8px] font-mono text-blue-400 mb-1 flex items-center justify-center gap-1">
                                    <Fingerprint size={8} /> {node.hash}
                                </div>
                                <div className="text-xs text-white font-medium">{node.label}</div>
                                <div className="text-[9px] text-zinc-500 uppercase mt-1">Owner: {node.owner}</div>
                            </div>
                        </Html>
                    )}
                    {isReceivingValue && (
                        <Html center position={[0, 1.5, 0]}>
                            <div className="text-[#10B981] text-[12px] font-mono font-bold bg-[#10B981]/10 px-3 py-1 rounded border border-[#10B981]/30 shadow-[0_0_15px_rgba(16,185,129,0.2)] animate-bounce">
                                +{tokenSplit.toFixed(1)} TKN
                            </div>
                        </Html>
                    )}
                </group>
            )}

            {node.layer === "L2" && (
                <group position={[0, 0.4, 0]}>
                    <mesh onClick={() => onSelect?.(node)}>
                        <octahedronGeometry args={[0.8, 0]} />
                        <meshPhysicalMaterial
                            color={THEME.l2_glass}
                            transmission={0.9}
                            opacity={1}
                            transparent
                            roughness={0.2}
                            thickness={1}
                            emissive={isSelected ? "#fde68a" : "#000000"}
                            emissiveIntensity={isSelected ? 0.35 : 0}
                        />
                        <Edges color={isSelected ? "#fde68a" : THEME.l2_glass} transparent opacity={0.6} />
                    </mesh>
                    {renderLabel && (
                        <Html
                            transform
                            rotation={[-Math.PI / 2, 0, 0]}
                            position={[0, -0.6, 0]}
                            occlude
                        >
                            <div className="text-[9px] font-mono text-amber-500/70 flex items-center justify-center gap-1 bg-black/60 px-2 py-0.5 rounded backdrop-blur-md whitespace-nowrap border border-amber-500/20 max-w-36 truncate">
                                <Lock size={8} /> {node.label}
                            </div>
                        </Html>
                    )}
                </group>
            )}

            {isL3 && (
                <group position={[0, 1, 0]}>
                    <mesh rotation={[0, Math.PI / 4, 0]}>
                        <icosahedronGeometry args={[1.2, 0]} />
                        <meshStandardMaterial
                            color={THEME.l3_apex}
                            roughness={0.1}
                            metalness={0.8}
                            emissive={THEME.l3_apex}
                            emissiveIntensity={isQueryTarget ? 1 : 0.2}
                        />
                        <Edges color="#ffffff" transparent opacity={0.2} />
                    </mesh>
                    {showL3Card && (
                    <Html center position={[0, 2, 0]} zIndexRange={[100, 0]} occlude>
                        <div
                            className={`backdrop-blur-xl border p-4 rounded-xl shadow-2xl w-56 text-center transition-all duration-300 ${isQueryTarget
                                    ? "bg-[#10B981]/10 border-[#10B981] shadow-[0_0_30px_rgba(16,185,129,0.2)]"
                                    : "bg-zinc-900/80 border-white/10"
                                }`}
                        >
                            <div className="flex justify-center items-center gap-1 text-[9px] uppercase tracking-widest text-emerald-400 font-bold mb-2">
                                <ShieldCheck size={12} /> MCP Agent Endpoint
                            </div>
                            <div className="text-sm font-medium text-white mb-2 leading-tight">{node.label}</div>
                            <div className="flex justify-between items-center border-t border-white/10 pt-2 mt-2">
                                <span className="text-[10px] text-zinc-400 font-mono">{node.hash}</span>
                                <span className="text-[10px] font-bold text-white bg-white/10 px-2 py-0.5 rounded">100 TKN</span>
                            </div>
                        </div>
                        {isQueryTarget && (
                            <div className="absolute -top-10 left-1/2 -translate-x-1/2 text-[#10B981] text-[11px] font-mono font-bold bg-[#10B981]/10 px-3 py-1 rounded border border-[#10B981]/30 whitespace-nowrap shadow-lg">
                                Synthesis Fee: +5.0 TKN
                            </div>
                        )}
                    </Html>
                    )}
                </group>
            )}
        </group>
    );
}

export default function ProtocolLedger25D() {
    const [activeQuery, setActiveQuery] = useState(false);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState("");
    const [liveGraph, setLiveGraph] = useState({ nodes: [], links: [] });
    const [l0ContentItems, setL0ContentItems] = useState([]);
    const [l0EntityEdges, setL0EntityEdges] = useState([]);
    const [l3Summaries, setL3Summaries] = useState([]);
    const [l2Spread, setL2Spread] = useState(1.0);
    const [focusLayer, setFocusLayer] = useState("ALL");
    const [selectedNodeId, setSelectedNodeId] = useState(null);
    const queryTimerRef = useRef(null);
    const cameraRef = useRef(null);
    const controlsRef = useRef(null);

    useEffect(() => {
        let mounted = true;

        async function loadGraph() {
            try {
                if (mounted) {
                    setError("");
                    setLoading(true);
                }
                const data = await invoke("get_graph_data");
                if (!mounted) {
                    return;
                }
                const nodes = Array.isArray(data?.nodes) ? data.nodes : [];
                const links = Array.isArray(data?.links) ? data.links : [];
                setLiveGraph({ nodes, links });

                try {
                    const contentItems = await invoke("get_l0_content_items");
                    if (mounted) {
                        setL0ContentItems(Array.isArray(contentItems) ? contentItems : []);
                    }
                } catch {
                    if (mounted) {
                        setL0ContentItems([]);
                    }
                }

                try {
                    const edges = await invoke("get_l0_entity_edges");
                    if (mounted) {
                        setL0EntityEdges(Array.isArray(edges) ? edges : []);
                    }
                } catch {
                    if (mounted) {
                        setL0EntityEdges([]);
                    }
                }

                try {
                    const summaries = await invoke("get_l3_summaries", { limit: 50 });
                    if (mounted) {
                        setL3Summaries(Array.isArray(summaries) ? summaries : []);
                    }
                } catch {
                    if (mounted) {
                        setL3Summaries([]);
                    }
                }
            } catch (err) {
                if (mounted) {
                    setError(`Failed to load graph data: ${String(err)}`);
                }
            } finally {
                if (mounted) {
                    setLoading(false);
                }
            }
        }

        loadGraph();
        const interval = setInterval(loadGraph, 15000);

        return () => {
            mounted = false;
            clearInterval(interval);
            if (queryTimerRef.current) {
                clearTimeout(queryTimerRef.current);
            }
        };
    }, []);

    const usingFallback = useMemo(
        () => !loading && (liveGraph.nodes?.length || 0) === 0 && (l0ContentItems?.length || 0) === 0,
        [l0ContentItems?.length, liveGraph.nodes, loading]
    );

    const protocolData = useMemo(() => {
        if ((liveGraph.nodes?.length || 0) === 0 && (l0ContentItems?.length || 0) === 0) {
            return {
                nodes: DEMO_PROTOCOL_DATA.nodes,
                links: DEMO_PROTOCOL_DATA.links,
                l2GraphLinks: [],
            };
        }

        const baseNodes = (liveGraph.nodes || []).map((node) => ({
            ...node,
            owner: node.entity_type || "Protocol",
            hash: hashPreview(node.id),
        }));

        const grouped = { L0: [], L2: [], L3: [] };

        const l0Items = (l0ContentItems?.length ? l0ContentItems : DEMO_L0_ITEMS).map((item) => ({
            id: `l0_${item.content_id}`,
            content_id: item.content_id,
            label: item.title || item.content_hash || item.content_id,
            entity_type: "RawSource",
            source_count: 1,
            confidence: 1,
            owner: "Source",
            hash: hashPreview(item.content_hash || item.content_id),
        }));
        grouped.L0 = l0Items;

        for (const node of baseNodes) {
            const layer = inferLayer(node);
            if (layer === "L2" || layer === "L3") {
                grouped[layer].push(node);
            }
        }

        const summaryById = new Map((l3Summaries || []).map((s) => [s.entity_id, s]));
        if (summaryById.size > 0) {
            grouped.L3 = grouped.L3.filter((node) => summaryById.has(node.id));
            if (grouped.L3.length === 0) {
                grouped.L3 = [...summaryById.entries()].map(([id, summary]) => ({
                    id,
                    label: summary.title,
                    entity_type: "summary",
                    description: summary.summary_text,
                    source_count: summary.source_entity_ids?.length || 1,
                    confidence: 1,
                    owner: "Synthesis",
                    hash: hashPreview(id),
                }));
            }
        }

        if (grouped.L3.length === 0 && grouped.L2.length > 0) {
            const promoted = [...grouped.L2]
                .sort((a, b) => (b.source_count || 0) - (a.source_count || 0))
                .slice(0, 1);
            const promotedIds = new Set(promoted.map((node) => node.id));
            grouped.L2 = grouped.L2.filter((node) => !promotedIds.has(node.id));
            grouped.L3 = promoted;
        }

        const l0EdgesRaw = l0EntityEdges?.length ? l0EntityEdges : DEMO_L0_EDGES;

        const l0ToL2Links = l0EdgesRaw
            .map((edge, index) => ({
                id: `l0e-${index}-${edge.content_id}-${edge.entity_id}`,
                source: `l0_${edge.content_id}`,
                target: edge.entity_id,
                predicate: "rootedIn",
                confidence: 1,
            }))
            .filter((link) => grouped.L0.some((n) => n.id === link.source) && grouped.L2.some((n) => n.id === link.target));

        const prelimIdSet = new Set([...grouped.L0, ...grouped.L2, ...grouped.L3].map((node) => node.id));

        const l2ToL3Links = (grouped.L3 || []).flatMap((l3Node, index) => {
            const summary = summaryById.get(l3Node.id);
            const sources = summary?.source_entity_ids || [];
            return sources.map((sourceId, j) => ({
                id: `l3e-${index}-${j}-${l3Node.id}-${sourceId}`,
                source: l3Node.id,
                target: sourceId,
                predicate: "synthesizes",
                confidence: 1,
            }));
        }).filter((link) => prelimIdSet.has(link.source) && prelimIdSet.has(link.target));

        const l2InternalLinks = (liveGraph.links || [])
            .filter((link) => {
                const sourceNode = grouped.L2.find((n) => n.id === link.source);
                const targetNode = grouped.L2.find((n) => n.id === link.target);
                return !!sourceNode && !!targetNode;
            })
            .map((link, index) => ({
                id: link.id || `l2-${index}-${link.source}-${link.target}`,
                source: link.source,
                target: link.target,
                predicate: link.predicate,
                confidence: link.confidence,
            }));

        const l2SeedLinks = [...l0ToL2Links, ...l2InternalLinks];
        const l2Layout = positionL2ByRelation(grouped.L2, l2SeedLinks, prelimIdSet, l2Spread);
        const positionedL0 = positionL0ByL2(grouped.L0, l2Layout.nodes, l0ToL2Links);

        const filteredLinks = [...l0ToL2Links, ...l2ToL3Links, ...l2InternalLinks];

        const nodes = [
            ...positionedL0,
            ...l2Layout.nodes,
            ...positionLayerNodes(grouped.L3, "L3"),
        ];
        const idSet = new Set(nodes.map((node) => node.id));

        const links = filteredLinks
            .filter((link) => idSet.has(link.source) && idSet.has(link.target))
            .map((link) => ({ ...link }));

        return { nodes, links, l2GraphLinks: l2Layout.graphLinks };
    }, [l0ContentItems, l0EntityEdges, l2Spread, l3Summaries, liveGraph]);

    const nodeById = useMemo(
        () => new Map(protocolData.nodes.map((node) => [node.id, node])),
        [protocolData]
    );

    const targetL3 = useMemo(
        () =>
            [...protocolData.nodes]
                .filter((node) => node.layer === "L3")
                .sort((a, b) => (b.source_count || 0) - (a.source_count || 0))[0] || null,
        [protocolData]
    );

    const expandedL3Id = useMemo(() => {
        if (selectedNodeId && nodeById.get(selectedNodeId)?.layer === "L3") {
            return selectedNodeId;
        }
        return targetL3?.id || null;
    }, [nodeById, selectedNodeId, targetL3]);

    const connectionStats = useMemo(() => {
        const stats = {};
        for (const node of protocolData.nodes) {
            stats[node.id] = { inCount: 0, outCount: 0, neighbors: new Set() };
        }
        for (const link of protocolData.links) {
            if (!stats[link.source] || !stats[link.target]) {
                continue;
            }
            stats[link.source].outCount += 1;
            stats[link.target].inCount += 1;
            stats[link.source].neighbors.add(link.target);
            stats[link.target].neighbors.add(link.source);
        }
        return stats;
    }, [protocolData]);

    const selectedNode = selectedNodeId ? nodeById.get(selectedNodeId) : null;
    const selectedConnections = useMemo(() => {
        if (!selectedNode || !connectionStats[selectedNode.id]) {
            return [];
        }
        return [...connectionStats[selectedNode.id].neighbors]
            .map((id) => nodeById.get(id))
            .filter(Boolean)
            .sort((a, b) => (b.source_count || 0) - (a.source_count || 0))
            .slice(0, 10);
    }, [connectionStats, nodeById, selectedNode]);

    const visibleNodes = useMemo(() => {
        if (focusLayer === "ALL") {
            return protocolData.nodes;
        }
        return protocolData.nodes.filter((node) => node.layer === focusLayer);
    }, [focusLayer, protocolData.nodes]);

    const visibleNodeIds = useMemo(
        () => new Set(visibleNodes.map((node) => node.id)),
        [visibleNodes]
    );

    const visibleLinks = useMemo(() => {
        if (focusLayer === "ALL") {
            return protocolData.links;
        }
        return protocolData.links.filter(
            (link) => visibleNodeIds.has(link.source) || visibleNodeIds.has(link.target)
        );
    }, [focusLayer, protocolData.links, visibleNodeIds]);

    const visibleL2GraphLinks = useMemo(() => {
        const all = protocolData.l2GraphLinks || [];
        if (focusLayer === "L0" || focusLayer === "L3") {
            return [];
        }
        return all.filter((link) => visibleNodeIds.has(link.source) && visibleNodeIds.has(link.target));
    }, [focusLayer, protocolData.l2GraphLinks, visibleNodeIds]);

    const tokenSplitMap = useMemo(() => {
        if (!activeQuery || !targetL3) {
            return {};
        }

        const adjacency = new Map();
        for (const link of protocolData.links) {
            if (!adjacency.has(link.source)) {
                adjacency.set(link.source, []);
            }
            if (!adjacency.has(link.target)) {
                adjacency.set(link.target, []);
            }
            adjacency.get(link.source).push(link.target);
            adjacency.get(link.target).push(link.source);
        }

        const queue = [{ id: targetL3.id, depth: 0 }];
        const visited = new Set([targetL3.id]);
        const counts = {};

        while (queue.length > 0) {
            const current = queue.shift();
            const currentNode = nodeById.get(current.id);
            if (current.depth > 0 && currentNode?.layer === "L0") {
                counts[current.id] = (counts[current.id] || 0) + 1;
            }
            if (current.depth >= 2) {
                continue;
            }
            for (const nextId of adjacency.get(current.id) || []) {
                if (visited.has(nextId)) {
                    continue;
                }
                visited.add(nextId);
                queue.push({ id: nextId, depth: current.depth + 1 });
            }
        }

        if (Object.keys(counts).length === 0) {
            const fallbacks = protocolData.nodes
                .filter((node) => node.layer === "L0")
                .sort((a, b) => (b.source_count || 0) - (a.source_count || 0))
                .slice(0, 3);
            for (const node of fallbacks) {
                counts[node.id] = 1;
            }
        }

        const totalWeight = Object.values(counts).reduce((sum, value) => sum + value, 0);
        if (totalWeight <= 0) {
            return {};
        }

        return Object.fromEntries(
            Object.entries(counts).map(([id, weight]) => [id, (95 * weight) / totalWeight])
        );
    }, [activeQuery, nodeById, protocolData, targetL3]);

    const rootRows = useMemo(() => {
        return Object.entries(tokenSplitMap)
            .map(([id, share]) => ({
                id,
                share,
                node: nodeById.get(id),
            }))
            .filter((entry) => !!entry.node)
            .sort((a, b) => b.share - a.share);
    }, [nodeById, tokenSplitMap]);

    const denseMode = useMemo(
        () => protocolData.nodes.length > 22 || protocolData.links.length > 60,
        [protocolData.links.length, protocolData.nodes.length]
    );

    const labelPriorityIds = useMemo(() => {
        const byLayer = { L0: [], L2: [], L3: [] };
        for (const node of protocolData.nodes) {
            if (byLayer[node.layer]) {
                byLayer[node.layer].push(node);
            }
        }

        const pickTop = (nodes, max) =>
            nodes
                .sort((a, b) => (b.source_count || 0) - (a.source_count || 0))
                .slice(0, max)
                .map((node) => node.id);

        return new Set([
            ...pickTop(byLayer.L0, 10),
            ...pickTop(byLayer.L2, 8),
            ...pickTop(byLayer.L3, 3),
        ]);
    }, [protocolData.nodes]);

    function simulateAgentQuery() {
        if (!targetL3) {
            return;
        }
        setActiveQuery(true);
        if (queryTimerRef.current) {
            clearTimeout(queryTimerRef.current);
        }
        queryTimerRef.current = setTimeout(() => setActiveQuery(false), 5000);
    }

    function resetView() {
        if (cameraRef.current) {
            cameraRef.current.position.set(26, 24, 26);
            cameraRef.current.zoom = 12;
            cameraRef.current.lookAt(0, 4, 0);
            cameraRef.current.updateProjectionMatrix();
        }
        if (controlsRef.current) {
            controlsRef.current.target.set(0, 4, 0);
            controlsRef.current.update();
        }
    }

    return (
        <div className="w-full h-screen bg-[#09090B] relative font-sans overflow-hidden select-none">
            <div className="absolute top-0 left-0 w-full p-6 z-10 pointer-events-none flex justify-between items-start border-b border-white/5 bg-black/20 backdrop-blur-sm">
                <div>
                    <h1 className="text-white font-medium text-xl tracking-wide flex items-center gap-3">
                        <Layers className="text-emerald-500" size={20} /> Knowledge Protocol
                    </h1>
                    <p className="text-zinc-500 text-xs mt-1 font-mono uppercase tracking-widest">
                        v1.0 / Isometric Provenance Ledger
                    </p>
                </div>
            </div>

            <AnimatePresence>
                <motion.div
                    initial={{ opacity: 0, x: 20 }}
                    animate={{ opacity: 1, x: 0 }}
                    className="absolute top-24 right-8 w-80 bg-[#111116]/95 backdrop-blur-2xl border border-white/10 rounded-2xl p-5 shadow-2xl z-20"
                >
                    <div className="flex items-center gap-2 text-emerald-400 mb-4 border-b border-white/10 pb-3">
                        <Network size={16} />
                        <span className="text-xs font-bold tracking-widest uppercase">Transaction Simulator</span>
                    </div>

                    <div className="mb-4">
                        <div className="text-[10px] text-zinc-500 uppercase tracking-wider mb-1">Target Asset</div>
                        <div className="text-sm text-white font-medium">{targetL3?.label || "No L3 entity available"}</div>
                    </div>

                    <div className="mb-4">
                        <div className="text-[10px] text-zinc-500 uppercase tracking-wider mb-2">Layer Focus</div>
                        <div className="grid grid-cols-4 gap-1.5">
                            {FOCUS_LAYERS.map((layer) => (
                                <button
                                    key={layer}
                                    onClick={() => setFocusLayer(layer)}
                                    className={`text-[10px] py-1 rounded border transition-colors ${focusLayer === layer
                                            ? "bg-emerald-500/20 border-emerald-500/60 text-emerald-300"
                                            : "bg-zinc-900/70 border-zinc-700 text-zinc-300 hover:bg-zinc-800"
                                        }`}
                                >
                                    {layer}
                                </button>
                            ))}
                        </div>
                    </div>

                    <div className="mb-4">
                        <div className="flex items-center justify-between text-[10px] text-zinc-500 uppercase tracking-wider mb-2">
                            <span>L2 Graph Spread</span>
                            <span className="text-zinc-400">{l2Spread.toFixed(2)}x</span>
                        </div>
                        <input
                            type="range"
                            min="0.7"
                            max="1.8"
                            step="0.05"
                            value={l2Spread}
                            onChange={(event) => setL2Spread(Number(event.target.value))}
                            className="w-full accent-amber-400"
                        />
                    </div>

                    <div className="bg-black/40 rounded-lg p-3 border border-black/50 mb-5">
                        <div className="text-[10px] uppercase text-zinc-500 mb-2 font-bold tracking-wider flex justify-between">
                            <span>root_L0L1[] Array</span>
                            <span>Weight</span>
                        </div>
                        <div className="space-y-1.5 font-mono text-[10px]">
                            {rootRows.length > 0 ? (
                                rootRows.map((row) => (
                                    <div className="flex justify-between" key={row.id}>
                                        <span className="text-zinc-300 truncate pr-2">{row.node.label}</span>
                                        <span className="text-emerald-500">{row.share.toFixed(1)}%</span>
                                    </div>
                                ))
                            ) : (
                                <div className="text-zinc-500">Run a query to compute live splits</div>
                            )}
                        </div>
                        <p className="text-[9px] text-zinc-500 mt-3 leading-relaxed border-t border-white/5 pt-2">
                            *Live protocol graph: 95% of value routes to foundational roots; L2 remains an internal synthesis boundary.
                        </p>
                    </div>

                    {loading && <div className="text-[10px] text-zinc-400 mb-3">Loading protocol graph...</div>}
                    {error && <div className="text-[10px] text-red-400 mb-3">{error}</div>}

                    <button
                        onClick={simulateAgentQuery}
                        disabled={activeQuery || !targetL3}
                        className={`w-full py-3 rounded-xl font-medium text-sm flex items-center justify-center gap-2 transition-all ${activeQuery
                                ? "bg-emerald-500/20 text-emerald-500 border border-emerald-500/50"
                                : targetL3
                                    ? "bg-white text-black hover:bg-zinc-200 shadow-lg"
                                    : "bg-zinc-800 text-zinc-400 border border-zinc-700"
                            }`}
                    >
                        <Cpu size={16} />
                        {activeQuery
                            ? "SETTLEMENT EXECUTING..."
                            : targetL3
                                ? "QUERY VIA MCP (100 TKN)"
                                : "NO L3 TARGET AVAILABLE"}
                    </button>

                    <button
                        onClick={resetView}
                        className="w-full mt-2 py-2 rounded-lg font-medium text-xs bg-zinc-800 text-zinc-200 border border-zinc-700 hover:bg-zinc-700 transition-colors"
                    >
                        RESET VIEW
                    </button>

                    {denseMode && (
                        <div className="text-[10px] text-zinc-500 mt-3">
                            Dense graph detected: labels are auto-prioritized to avoid overlap.
                        </div>
                    )}
                </motion.div>
            </AnimatePresence>

            {usingFallback && (
                <div className="absolute bottom-6 left-6 z-20 bg-amber-500/10 border border-amber-500/30 text-amber-300 text-xs font-mono px-3 py-2 rounded-lg backdrop-blur-md">
                    Live graph has no entities yet — showing protocol demo stack (L0→L3).
                </div>
            )}

            {selectedNode && (selectedNode.layer === "L0" || selectedNode.layer === "L2") && (
                <div className="absolute left-6 top-24 w-80 z-20 bg-[#111116]/95 backdrop-blur-2xl border border-white/10 rounded-2xl p-4 shadow-2xl">
                    <div className="flex items-center justify-between mb-3 border-b border-white/10 pb-2">
                        <div className="text-xs tracking-widest uppercase text-zinc-400">Node Inspector</div>
                        <button
                            onClick={() => setSelectedNodeId(null)}
                            className="text-zinc-400 hover:text-white text-xs"
                        >
                            Close
                        </button>
                    </div>
                    <div className="text-sm text-white font-medium mb-1">{selectedNode.label}</div>
                    <div className="text-[10px] text-zinc-400 mb-3 font-mono">{selectedNode.id}</div>
                    <div className="grid grid-cols-2 gap-2 text-[11px] mb-3">
                        <div className="bg-black/40 rounded p-2 border border-white/10">
                            <div className="text-zinc-500 uppercase text-[9px]">Layer</div>
                            <div className="text-white">{selectedNode.layer}</div>
                        </div>
                        <div className="bg-black/40 rounded p-2 border border-white/10">
                            <div className="text-zinc-500 uppercase text-[9px]">Type</div>
                            <div className="text-white">{selectedNode.entity_type || "Unknown"}</div>
                        </div>
                        <div className="bg-black/40 rounded p-2 border border-white/10">
                            <div className="text-zinc-500 uppercase text-[9px]">Incoming</div>
                            <div className="text-white">{connectionStats[selectedNode.id]?.inCount || 0}</div>
                        </div>
                        <div className="bg-black/40 rounded p-2 border border-white/10">
                            <div className="text-zinc-500 uppercase text-[9px]">Outgoing</div>
                            <div className="text-white">{connectionStats[selectedNode.id]?.outCount || 0}</div>
                        </div>
                    </div>
                    <div className="text-[10px] text-zinc-500 uppercase tracking-wider mb-1">Connected Nodes</div>
                    <div className="max-h-36 overflow-y-auto space-y-1 pr-1">
                        {selectedConnections.length > 0 ? (
                            selectedConnections.map((node) => (
                                <button
                                    key={node.id}
                                    onClick={() => setSelectedNodeId(node.id)}
                                    className="w-full text-left text-[11px] bg-zinc-900/70 border border-zinc-700 rounded px-2 py-1 hover:bg-zinc-800"
                                >
                                    <div className="text-zinc-200 truncate">{node.label}</div>
                                    <div className="text-zinc-500 text-[9px]">{node.layer} · {node.entity_type || "Entity"}</div>
                                </button>
                            ))
                        ) : (
                            <div className="text-[11px] text-zinc-500">No direct connections in current view</div>
                        )}
                    </div>
                </div>
            )}

            <Canvas>
                <OrthographicCamera
                    makeDefault
                    ref={cameraRef}
                    position={[26, 24, 26]}
                    zoom={12}
                    near={0.1}
                    far={1000}
                    onUpdate={(camera) => camera.lookAt(0, 4, 0)}
                />
                <OrbitControls
                    ref={controlsRef}
                    makeDefault
                    target={[0, 4, 0]}
                    enableDamping
                    dampingFactor={0.08}
                    minZoom={2}
                    maxZoom={30}
                    maxPolarAngle={Math.PI / 2.1}
                />
                <ambientLight intensity={0.5} />
                <directionalLight position={[10, 20, 5]} intensity={1.5} color="#ffffff" />

                <LayerPlane y={Y_LEVELS.L0 - 0.8} color={THEME.l0_accent} label="L0: RAW SOURCES (ROOTS UNDER L2)" />
                <LayerPlane
                    y={Y_LEVELS.L2}
                    color={THEME.l2_glass}
                    label="L2: ENTITY GRAPH (PRIVATE SYNTHESIS)"
                    isPrivate
                />
                <LayerPlane y={Y_LEVELS.L3} color={THEME.l3_apex} label="L3: EMERGENT INSIGHTS (PUBLIC MARKET)" />

                {visibleLinks.map((link, index) => {
                    const source = nodeById.get(link.source);
                    const target = nodeById.get(link.target);
                    if (!source || !target) {
                        return null;
                    }

                    let start = getNodeAnchor(source, "center");
                    let end = getNodeAnchor(target, "center");

                    if (source.layer === "L0" && target.layer === "L2") {
                        start = getNodeAnchor(source, "up");
                        end = getNodeAnchor(target, "down");
                    } else if (source.layer === "L2" && target.layer === "L0") {
                        start = getNodeAnchor(source, "down");
                        end = getNodeAnchor(target, "up");
                    } else if (source.layer === "L2" && target.layer === "L3") {
                        start = getNodeAnchor(source, "up");
                        end = getNodeAnchor(target, "down");
                    } else if (source.layer === "L3" && target.layer === "L2") {
                        start = getNodeAnchor(source, "down");
                        end = getNodeAnchor(target, "up");
                    }

                    return (
                        <OrthogonalCircuit
                            key={`${link.source}-${link.target}-${index}`}
                            start={start}
                            end={end}
                            isFlowing={activeQuery && (source.layer === "L0" || target.layer === "L0")}
                        />
                    );
                })}

                {visibleL2GraphLinks.map((link, index) => {
                    const source = nodeById.get(link.source);
                    const target = nodeById.get(link.target);
                    if (!source || !target) {
                        return null;
                    }

                    return (
                        <LayerGraphLink
                            key={`l2-${link.source}-${link.target}-${index}`}
                            start={new THREE.Vector3(source.x, Y_LEVELS.L2 + 0.06, source.z)}
                            end={new THREE.Vector3(target.x, Y_LEVELS.L2 + 0.06, target.z)}
                            weight={link.weight}
                        />
                    );
                })}

                {visibleNodes.map((node) => (
                    <ProtocolNode
                        key={node.id}
                        node={node}
                        isQueryActive={activeQuery}
                        isQueryTarget={activeQuery && targetL3?.id === node.id}
                        tokenSplit={tokenSplitMap[node.id] || 0}
                        denseMode={denseMode}
                        showLabel={labelPriorityIds.has(node.id)}
                        onSelect={(selected) => {
                            if (selected.layer === "L0" || selected.layer === "L2") {
                                setSelectedNodeId(selected.id);
                            }
                        }}
                        isSelected={selectedNodeId === node.id}
                        showL3Card={expandedL3Id === node.id}
                    />
                ))}
            </Canvas>
        </div>
    );
}

function seededSigned(value) {
    return seededUnit(value) * 2 - 1;
}

function positionL0ByL2(l0Nodes, l2Nodes, l0ToL2Links) {
    const nodes = [...l0Nodes].slice(0, 48);
    if (nodes.length === 0) {
        return [];
    }

    const l2ById = new Map(l2Nodes.map((node) => [node.id, node]));
    const targets = new Map();

    for (const l0 of nodes) {
        const connected = l0ToL2Links
            .filter((link) => link.source === l0.id)
            .map((link) => l2ById.get(link.target))
            .filter(Boolean);

        if (connected.length > 0) {
            const cx = connected.reduce((sum, node) => sum + node.x, 0) / connected.length;
            const cz = connected.reduce((sum, node) => sum + node.z, 0) / connected.length;
            targets.set(l0.id, {
                x: cx + seededSigned(`${l0.id}-jx`) * 0.5,
                z: cz - 5.2 + seededSigned(`${l0.id}-jz`) * 0.6,
            });
        } else {
            targets.set(l0.id, {
                x: seededSigned(`${l0.id}-fx`) * 7.5,
                z: -5.8 + seededSigned(`${l0.id}-fz`) * 1.3,
            });
        }
    }

    const positions = new Map();
    const velocities = new Map();
    for (const node of nodes) {
        const t = targets.get(node.id);
        positions.set(node.id, { x: t.x, z: t.z });
        velocities.set(node.id, { x: 0, z: 0 });
    }

    for (let iter = 0; iter < 120; iter++) {
        const forces = new Map(nodes.map((node) => [node.id, { x: 0, z: 0 }]));

        for (let i = 0; i < nodes.length; i++) {
            for (let j = i + 1; j < nodes.length; j++) {
                const a = nodes[i].id;
                const b = nodes[j].id;
                const pa = positions.get(a);
                const pb = positions.get(b);
                const dx = pa.x - pb.x;
                const dz = pa.z - pb.z;
                const dist2 = Math.max(0.2, dx * dx + dz * dz);
                const dist = Math.sqrt(dist2);
                const repel = 0.08 / dist2;
                const rx = (dx / dist) * repel;
                const rz = (dz / dist) * repel;
                forces.get(a).x += rx;
                forces.get(a).z += rz;
                forces.get(b).x -= rx;
                forces.get(b).z -= rz;
            }
        }

        for (const node of nodes) {
            const id = node.id;
            const p = positions.get(id);
            const t = targets.get(id);
            forces.get(id).x += (t.x - p.x) * 0.04;
            forces.get(id).z += (t.z - p.z) * 0.04;
        }

        for (const node of nodes) {
            const id = node.id;
            const v = velocities.get(id);
            const f = forces.get(id);
            const p = positions.get(id);
            v.x = (v.x + f.x) * 0.82;
            v.z = (v.z + f.z) * 0.82;
            p.x += v.x;
            p.z += v.z;
            p.x = Math.max(-11, Math.min(11, p.x));
            p.z = Math.max(-10, Math.min(2, p.z));
        }
    }

    return nodes.map((node) => {
        const p = positions.get(node.id);
        return {
            ...node,
            layer: "L0",
            x: p.x,
            z: p.z,
            yOffset: -0.8 - seededUnit(`${node.id}-root-depth`) * 1.15,
        };
    });
}