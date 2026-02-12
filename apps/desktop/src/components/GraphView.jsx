import { useRef, useEffect, useCallback } from "react";
import * as d3 from "d3";

// Entity type → color mapping (matches Tailwind entity colors)
const TYPE_COLORS = {
  Person: "#e599f7",
  Organization: "#74c0fc",
  Concept: "#69db7c",
  Decision: "#ffd43b",
  Task: "#ff8787",
  Asset: "#a9e34b",
  Goal: "#f783ac",
  Pattern: "#66d9e8",
  Insight: "#b197fc",
  Value: "#ffa94d",
  Pipeline: "#20c997",
  Conversation: "#87ceeb",
  Research: "#dda0dd",
  Product: "#98d8c8",
  Win: "#fff176",
  Bet: "#ff7043",
  Commitment: "#ab47bc",
  Problem: "#ef5350",
  Self: "#ffd700",
};

const DEFAULT_COLOR = "#868e96";

function getColor(type) {
  return TYPE_COLORS[type] || DEFAULT_COLOR;
}

function getRadius(node) {
  // Scale by source_count, min 4, max 20
  return Math.max(4, Math.min(20, 4 + node.source_count * 2));
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

    // Create container for zoom
    const g = svg.append("g");

    // Zoom behavior
    const zoom = d3
      .zoom()
      .scaleExtent([0.1, 8])
      .on("zoom", (event) => {
        g.attr("transform", event.transform);
      });
    svg.call(zoom);

    // Build node/link index maps for D3
    const nodeMap = new Map(data.nodes.map((n) => [n.id, { ...n }]));
    const nodes = Array.from(nodeMap.values());

    // Only include links where both endpoints exist
    const links = data.links
      .filter((l) => nodeMap.has(l.source) && nodeMap.has(l.target))
      .map((l) => ({ ...l }));

    // Force simulation
    const simulation = d3
      .forceSimulation(nodes)
      .force(
        "link",
        d3
          .forceLink(links)
          .id((d) => d.id)
          .distance(80)
      )
      .force("charge", d3.forceManyBody().strength(-120))
      .force("center", d3.forceCenter(width / 2, height / 2))
      .force("collision", d3.forceCollide().radius((d) => getRadius(d) + 2));

    simulationRef.current = simulation;

    // Draw links
    const link = g
      .append("g")
      .attr("class", "links")
      .selectAll("line")
      .data(links)
      .join("line")
      .attr("stroke", "#374151")
      .attr("stroke-opacity", 0.5)
      .attr("stroke-width", (d) => Math.max(0.5, d.confidence));

    // Draw link labels (predicate) — only for small graphs
    let linkLabel;
    if (links.length < 100) {
      linkLabel = g
        .append("g")
        .attr("class", "link-labels")
        .selectAll("text")
        .data(links)
        .join("text")
        .attr("class", "link-label")
        .attr("fill", "#6b7280")
        .attr("text-anchor", "middle")
        .attr("dy", -3)
        .text((d) => d.predicate);
    }

    // Draw nodes
    const node = g
      .append("g")
      .attr("class", "nodes")
      .selectAll("circle")
      .data(nodes)
      .join("circle")
      .attr("r", (d) => getRadius(d))
      .attr("fill", (d) => getColor(d.entity_type))
      .attr("stroke", (d) =>
        selectedEntity && selectedEntity.id === d.id ? "#fff" : "transparent"
      )
      .attr("stroke-width", 2)
      .attr("cursor", "pointer")
      .on("click", handleNodeClick)
      .call(drag(simulation));

    // Node hover tooltip
    node.append("title").text((d) => {
      const desc = d.description
        ? `\n${d.description.substring(0, 100)}`
        : "";
      return `${d.label} (${d.entity_type})${desc}`;
    });

    // Draw node labels
    const label = g
      .append("g")
      .attr("class", "node-labels")
      .selectAll("text")
      .data(nodes)
      .join("text")
      .attr("class", "node-label")
      .attr("fill", "#d1d5db")
      .attr("font-size", (d) => Math.max(8, Math.min(12, 8 + d.source_count)))
      .attr("text-anchor", "middle")
      .attr("dy", (d) => getRadius(d) + 12)
      .text((d) => {
        // Truncate long labels
        return d.label.length > 25 ? d.label.substring(0, 22) + "…" : d.label;
      });

    // Tick function
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

      node.attr("cx", (d) => d.x).attr("cy", (d) => d.y);

      label.attr("x", (d) => d.x).attr("y", (d) => d.y);
    });

    // Initial zoom to fit
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
    <svg ref={svgRef} className="w-full h-full bg-gray-950" />
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
