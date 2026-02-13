import { useRef, useEffect, useCallback } from "react";
import * as d3 from "d3";
import { getEntityColor, GRAPH_CONFIG } from "../lib/constants";

const { BG_COLOR, LINK_COLOR, LINK_HOVER_COLOR, LABEL_COLOR, LABEL_DIM_COLOR, LINK_LABEL_THRESHOLD } = GRAPH_CONFIG;

function getRadius(node) {
  // Scale by source_count: min 4, max 20
  const base = Math.max(4, Math.min(20, 4 + (node.source_count || 1) * 2));
  return base;
}

function getOpacity(node) {
  // Fresher (higher source_count) nodes are more opaque
  return 0.5 + Math.min(0.5, (node.source_count || 1) * 0.05);
}

export default function GraphView({ data, onNodeClick, selectedEntity }) {
  const svgRef = useRef(null);
  const simulationRef = useRef(null);

  const handleNodeClick = useCallback(
    (event, d) => {
      if (onNodeClick) onNodeClick(d);
    },
    [onNodeClick]
  );

  useEffect(() => {
    if (!data || !data.nodes.length) return;

    const svg = d3.select(svgRef.current);
    const width = svgRef.current.clientWidth;
    const height = svgRef.current.clientHeight;

    // Clear previous
    svg.selectAll("*").remove();

    // Background gradient (subtle radial)
    const defs = svg.append("defs");

    // Radial gradient for ambient glow in center
    const radialGrad = defs.append("radialGradient")
      .attr("id", "ambient-glow")
      .attr("cx", "50%").attr("cy", "50%").attr("r", "50%");
    radialGrad.append("stop")
      .attr("offset", "0%")
      .attr("stop-color", "rgba(92, 124, 250, 0.03)");
    radialGrad.append("stop")
      .attr("offset", "100%")
      .attr("stop-color", "transparent");

    // Background
    svg.append("rect")
      .attr("width", width)
      .attr("height", height)
      .attr("fill", BG_COLOR);

    // Ambient glow
    svg.append("rect")
      .attr("width", width)
      .attr("height", height)
      .attr("fill", "url(#ambient-glow)");

    // Glow filter for selected/hovered nodes
    const glowFilter = defs.append("filter")
      .attr("id", "node-glow")
      .attr("x", "-50%").attr("y", "-50%")
      .attr("width", "200%").attr("height", "200%");
    glowFilter.append("feGaussianBlur")
      .attr("stdDeviation", "4")
      .attr("result", "blur");
    const feMerge = glowFilter.append("feMerge");
    feMerge.append("feMergeNode").attr("in", "blur");
    feMerge.append("feMergeNode").attr("in", "SourceGraphic");

    // Container for zoom
    const g = svg.append("g");

    // Zoom behavior
    const zoom = d3
      .zoom()
      .scaleExtent([0.1, 8])
      .on("zoom", (event) => {
        g.attr("transform", event.transform);
      });
    svg.call(zoom);

    // Build node/link data
    const nodeMap = new Map(data.nodes.map((n) => [n.id, { ...n }]));
    const nodes = Array.from(nodeMap.values());
    const links = data.links
      .filter((l) => nodeMap.has(l.source) && nodeMap.has(l.target))
      .map((l) => ({ ...l }));

    // Force simulation
    const simulation = d3
      .forceSimulation(nodes)
      .force(
        "link",
        d3.forceLink(links)
          .id((d) => d.id)
          .distance(100)
      )
      .force("charge", d3.forceManyBody().strength(-150))
      .force("center", d3.forceCenter(width / 2, height / 2))
      .force("collision", d3.forceCollide().radius((d) => getRadius(d) + 4));

    simulationRef.current = simulation;

    // Links
    const link = g
      .append("g")
      .selectAll("line")
      .data(links)
      .join("line")
      .attr("stroke", LINK_COLOR)
      .attr("stroke-width", (d) => Math.max(0.5, (d.confidence || 0.5) * 1.5));

    // Link labels — only for small graphs
    let linkLabel;
    if (links.length < LINK_LABEL_THRESHOLD) {
      linkLabel = g
        .append("g")
        .selectAll("text")
        .data(links)
        .join("text")
        .attr("class", "link-label")
        .attr("fill", LABEL_DIM_COLOR)
        .attr("text-anchor", "middle")
        .attr("dy", -4)
        .attr("font-size", "9px")
        .attr("font-family", "'SF Mono', 'Fira Code', monospace")
        .text((d) => d.predicate);
    }

    // Node glow (background circle for ambient effect)
    const nodeGlow = g
      .append("g")
      .selectAll("circle")
      .data(nodes)
      .join("circle")
      .attr("r", (d) => getRadius(d) + 8)
      .attr("fill", (d) => {
        const color = getEntityColor(d.entity_type);
        return color + "15"; // ~8% opacity hex
      });

    // Nodes
    const node = g
      .append("g")
      .selectAll("circle")
      .data(nodes)
      .join("circle")
      .attr("r", (d) => getRadius(d))
      .attr("fill", (d) => getEntityColor(d.entity_type))
      .attr("fill-opacity", (d) => getOpacity(d))
      .attr("stroke", "transparent")
      .attr("stroke-width", 2)
      .attr("cursor", "pointer")
      .on("click", handleNodeClick)
      .on("mouseover", function (event, d) {
        d3.select(this)
          .attr("stroke", getEntityColor(d.entity_type))
          .attr("stroke-opacity", 0.5)
          .attr("filter", "url(#node-glow)");
        // Highlight connected links
        link.attr("stroke", (l) =>
          l.source.id === d.id || l.target.id === d.id
            ? LINK_HOVER_COLOR
            : LINK_COLOR
        );
      })
      .on("mouseout", function (event, d) {
        const isSelected = selectedEntity && selectedEntity.id === d.id;
        d3.select(this)
          .attr("stroke", isSelected ? "#fff" : "transparent")
          .attr("stroke-opacity", isSelected ? 0.3 : 0)
          .attr("filter", isSelected ? "url(#node-glow)" : null);
        link.attr("stroke", LINK_COLOR);
      })
      .call(drag(simulation));

    // Update selected state
    node
      .attr("stroke", (d) =>
        selectedEntity && selectedEntity.id === d.id ? "#fff" : "transparent"
      )
      .attr("stroke-opacity", (d) =>
        selectedEntity && selectedEntity.id === d.id ? 0.3 : 0
      )
      .attr("filter", (d) =>
        selectedEntity && selectedEntity.id === d.id ? "url(#node-glow)" : null
      );

    // Node hover tooltip
    node.append("title").text((d) => {
      const desc = d.description
        ? `\n${d.description.substring(0, 100)}`
        : "";
      return `${d.label} (${d.entity_type})${desc}`;
    });

    // Node labels
    const label = g
      .append("g")
      .selectAll("text")
      .data(nodes)
      .join("text")
      .attr("class", "node-label")
      .attr("fill", LABEL_COLOR)
      .attr("font-size", (d) =>
        Math.max(9, Math.min(12, 9 + (d.source_count || 1) * 0.5)) + "px"
      )
      .attr("font-family", "'SF Pro Display', -apple-system, 'Segoe UI', sans-serif")
      .attr("font-weight", "300")
      .attr("letter-spacing", "0.3px")
      .attr("text-anchor", "middle")
      .attr("dy", (d) => getRadius(d) + 14)
      .text((d) =>
        d.label.length > 25 ? d.label.substring(0, 22) + "…" : d.label
      );

    // Tick
    simulation.on("tick", () => {
      link
        .attr("x1", (d) => d.source.x)
        .attr("y1", (d) => d.source.y)
        .attr("x2", (d) => d.target.x)
        .attr("y2", (d) => d.target.y);

      if (linkLabel) {
        linkLabel
          .attr("x", (d) => (d.source.x + d.target.x) / 2)
          .attr("y", (d) => (d.source.y + d.target.y) / 2);
      }

      nodeGlow.attr("cx", (d) => d.x).attr("cy", (d) => d.y);
      node.attr("cx", (d) => d.x).attr("cy", (d) => d.y);
      label.attr("x", (d) => d.x).attr("y", (d) => d.y);
    });

    // Zoom to fit
    const initialScale = Math.min(
      1,
      Math.min(width, height) / (nodes.length * 3 + 200)
    );
    svg.call(
      zoom.transform,
      d3.zoomIdentity
        .translate(width / 2, height / 2)
        .scale(Math.max(0.3, initialScale))
        .translate(-width / 2, -height / 2)
    );

    return () => {
      simulation.stop();
    };
  }, [data, handleNodeClick, selectedEntity]);

  return (
    <svg
      ref={svgRef}
      className="w-full h-full"
      style={{ background: BG_COLOR }}
    />
  );
}

// D3 drag behavior
function drag(simulation) {
  function dragstarted(event) {
    if (!event.active) simulation.alphaTarget(0.3).restart();
    event.subject.fx = event.subject.x;
    event.subject.fy = event.subject.y;
  }

  function dragged(event) {
    event.subject.fx = event.x;
    event.subject.fy = event.y;
  }

  function dragended(event) {
    if (!event.active) simulation.alphaTarget(0);
    event.subject.fx = null;
    event.subject.fy = null;
  }

  return d3
    .drag()
    .on("start", dragstarted)
    .on("drag", dragged)
    .on("end", dragended);
}
