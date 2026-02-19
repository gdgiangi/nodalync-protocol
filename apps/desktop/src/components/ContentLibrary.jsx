import { useState, useEffect, useMemo } from "react";
import { getNodeLevel, getEntityColor, LEVEL_CONFIG } from "../lib/constants";

function ContentLibrary({ data, onNodeClick, loading }) {
  const [searchQuery, setSearchQuery] = useState("");
  const [sortField, setSortField] = useState("label");
  const [sortDirection, setSortDirection] = useState("asc");

  // Filter and sort the data
  const filteredAndSortedNodes = useMemo(() => {
    if (!data?.nodes) return [];

    let filtered = data.nodes;

    // Apply search filter
    if (searchQuery.trim()) {
      const query = searchQuery.toLowerCase();
      filtered = filtered.filter(node => 
        (node.label || node.id || '').toLowerCase().includes(query)
      );
    }

    // Apply sorting
    filtered.sort((a, b) => {
      let aValue, bValue;
      
      switch (sortField) {
        case 'label':
          aValue = (a.label || a.id || '').toLowerCase();
          bValue = (b.label || b.id || '').toLowerCase();
          break;
        case 'entity_type':
          aValue = (a.entity_type || '').toLowerCase();
          bValue = (b.entity_type || '').toLowerCase();
          break;
        case 'level':
          aValue = getNodeLevel(a);
          bValue = getNodeLevel(b);
          break;
        case 'source_count':
          aValue = a.source_count || 0;
          bValue = b.source_count || 0;
          break;
        default:
          return 0;
      }

      if (aValue < bValue) return sortDirection === 'asc' ? -1 : 1;
      if (aValue > bValue) return sortDirection === 'asc' ? 1 : -1;
      return 0;
    });

    return filtered;
  }, [data?.nodes, searchQuery, sortField, sortDirection]);

  const handleSort = (field) => {
    if (sortField === field) {
      setSortDirection(sortDirection === 'asc' ? 'desc' : 'asc');
    } else {
      setSortField(field);
      setSortDirection('asc');
    }
  };

  const getSortIcon = (field) => {
    if (sortField !== field) {
      return (
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" style={{ opacity: 0.3 }}>
          <path d="M8 9l4-4 4 4" />
          <path d="M16 15l-4 4-4-4" />
        </svg>
      );
    }
    
    if (sortDirection === 'asc') {
      return (
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M8 9l4-4 4 4" />
        </svg>
      );
    } else {
      return (
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M16 15l-4 4-4-4" />
        </svg>
      );
    }
  };

  if (loading) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <div className="text-center">
          <div
            className="w-1.5 h-1.5 rounded-full mx-auto mb-3"
            style={{
              background: 'var(--accent)',
              animation: 'pulse-subtle 1.2s ease-in-out infinite',
            }}
          />
          <span className="label-xs" style={{ color: 'var(--text-ghost)' }}>
            LOADING CONTENT
          </span>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 flex flex-col">
      {/* Search bar */}
      <div 
        className="p-4 border-b"
        style={{ borderColor: 'var(--border-subtle)' }}
      >
        <div className="relative">
          <input
            type="text"
            placeholder="Search content..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="w-full pl-10 pr-4 py-2 rounded-md text-sm"
            style={{
              background: 'var(--bg-elevated)',
              border: '1px solid var(--border-subtle)',
              color: 'var(--text-primary)',
            }}
            onFocus={(e) => {
              e.currentTarget.style.borderColor = 'var(--accent)';
            }}
            onBlur={(e) => {
              e.currentTarget.style.borderColor = 'var(--border-subtle)';
            }}
          />
          <svg 
            width="16" 
            height="16" 
            viewBox="0 0 24 24" 
            fill="none" 
            stroke="currentColor" 
            strokeWidth="2" 
            strokeLinecap="round" 
            strokeLinejoin="round"
            className="absolute left-3 top-1/2 transform -translate-y-1/2"
            style={{ color: 'var(--text-ghost)' }}
          >
            <circle cx="11" cy="11" r="8" />
            <path d="M21 21l-4.35-4.35" />
          </svg>
        </div>
      </div>

      {/* Table */}
      <div className="flex-1 overflow-auto">
        {filteredAndSortedNodes.length === 0 ? (
          <div className="flex items-center justify-center h-full">
            <div className="text-center">
              <div
                className="w-16 h-16 mx-auto mb-4 rounded-full flex items-center justify-center"
                style={{
                  background: 'var(--bg-elevated)',
                  border: '1px solid var(--border-subtle)',
                }}
              >
                <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" style={{ color: 'var(--text-ghost)' }}>
                  <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                  <polyline points="14 2 14 8 20 8" />
                  <line x1="16" y1="13" x2="8" y2="13" />
                  <line x1="16" y1="17" x2="8" y2="17" />
                  <polyline points="10 9 9 9 8 9" />
                </svg>
              </div>
              <p className="text-[13px] mb-1" style={{ color: 'var(--text-tertiary)' }}>
                {searchQuery ? 'No matching content found' : 'No content available'}
              </p>
              <p className="text-[11px]" style={{ color: 'var(--text-ghost)' }}>
                {searchQuery ? 'Try adjusting your search terms' : 'Import content to get started'}
              </p>
            </div>
          </div>
        ) : (
          <table className="w-full">
            <thead>
              <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                <th 
                  className="text-left p-4 cursor-pointer hover:bg-opacity-50 transition-colors"
                  style={{ 
                    background: 'var(--bg-elevated)',
                    color: 'var(--text-secondary)',
                  }}
                  onMouseEnter={(e) => {
                    e.currentTarget.style.background = 'var(--bg-hover)';
                  }}
                  onMouseLeave={(e) => {
                    e.currentTarget.style.background = 'var(--bg-elevated)';
                  }}
                  onClick={() => handleSort('label')}
                >
                  <div className="flex items-center gap-2">
                    <span className="label-sm">LABEL/TITLE</span>
                    {getSortIcon('label')}
                  </div>
                </th>
                <th 
                  className="text-left p-4 cursor-pointer hover:bg-opacity-50 transition-colors"
                  style={{ 
                    background: 'var(--bg-elevated)',
                    color: 'var(--text-secondary)',
                  }}
                  onMouseEnter={(e) => {
                    e.currentTarget.style.background = 'var(--bg-hover)';
                  }}
                  onMouseLeave={(e) => {
                    e.currentTarget.style.background = 'var(--bg-elevated)';
                  }}
                  onClick={() => handleSort('entity_type')}
                >
                  <div className="flex items-center gap-2">
                    <span className="label-sm">ENTITY TYPE</span>
                    {getSortIcon('entity_type')}
                  </div>
                </th>
                <th 
                  className="text-left p-4 cursor-pointer hover:bg-opacity-50 transition-colors"
                  style={{ 
                    background: 'var(--bg-elevated)',
                    color: 'var(--text-secondary)',
                  }}
                  onMouseEnter={(e) => {
                    e.currentTarget.style.background = 'var(--bg-hover)';
                  }}
                  onMouseLeave={(e) => {
                    e.currentTarget.style.background = 'var(--bg-elevated)';
                  }}
                  onClick={() => handleSort('level')}
                >
                  <div className="flex items-center gap-2">
                    <span className="label-sm">LEVEL</span>
                    {getSortIcon('level')}
                  </div>
                </th>
                <th 
                  className="text-left p-4 cursor-pointer hover:bg-opacity-50 transition-colors"
                  style={{ 
                    background: 'var(--bg-elevated)',
                    color: 'var(--text-secondary)',
                  }}
                  onMouseEnter={(e) => {
                    e.currentTarget.style.background = 'var(--bg-hover)';
                  }}
                  onMouseLeave={(e) => {
                    e.currentTarget.style.background = 'var(--bg-elevated)';
                  }}
                  onClick={() => handleSort('source_count')}
                >
                  <div className="flex items-center gap-2">
                    <span className="label-sm">SOURCES</span>
                    {getSortIcon('source_count')}
                  </div>
                </th>
              </tr>
            </thead>
            <tbody>
              {filteredAndSortedNodes.map((node, index) => {
                const level = getNodeLevel(node);
                const levelConfig = LEVEL_CONFIG[level] || LEVEL_CONFIG.L2;
                const entityColor = getEntityColor(node.entity_type);
                
                return (
                  <tr 
                    key={node.id || index}
                    className="cursor-pointer hover:bg-opacity-50 transition-colors border-b"
                    style={{ borderColor: 'var(--border-subtle)' }}
                    onMouseEnter={(e) => {
                      e.currentTarget.style.background = 'var(--bg-hover)';
                    }}
                    onMouseLeave={(e) => {
                      e.currentTarget.style.background = 'transparent';
                    }}
                    onClick={() => onNodeClick(node)}
                  >
                    <td className="p-4">
                      <div className="flex items-center gap-3">
                        <div 
                          className="w-2.5 h-2.5 rounded-full flex-shrink-0"
                          style={{ 
                            background: level === 'L2' ? entityColor : levelConfig.color,
                            opacity: levelConfig.opacity,
                          }}
                        />
                        <span 
                          className="text-sm truncate"
                          style={{ color: 'var(--text-primary)' }}
                        >
                          {node.label || node.id || 'Unnamed'}
                        </span>
                      </div>
                    </td>
                    <td className="p-4">
                      <span 
                        className="text-sm"
                        style={{ color: 'var(--text-secondary)' }}
                      >
                        {node.entity_type || '—'}
                      </span>
                    </td>
                    <td className="p-4">
                      <span 
                        className="text-xs px-2 py-1 rounded-full font-medium"
                        style={{ 
                          background: `${level === 'L2' ? entityColor : levelConfig.color}20`,
                          color: level === 'L2' ? entityColor : levelConfig.color,
                          border: `1px solid ${level === 'L2' ? entityColor : levelConfig.color}40`,
                        }}
                      >
                        {level}
                      </span>
                    </td>
                    <td className="p-4">
                      <span 
                        className="text-sm"
                        style={{ color: 'var(--text-secondary)' }}
                      >
                        {node.source_count || 0}
                      </span>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>
      
      {/* Table stats */}
      {filteredAndSortedNodes.length > 0 && (
        <div 
          className="px-4 py-2 border-t text-right"
          style={{ 
            borderColor: 'var(--border-subtle)',
            background: 'var(--bg-elevated)',
          }}
        >
          <span className="text-xs" style={{ color: 'var(--text-ghost)' }}>
            {filteredAndSortedNodes.length} of {data?.nodes?.length || 0} items
            {searchQuery && ` matching "${searchQuery}"`}
          </span>
        </div>
      )}
    </div>
  );
}

export default ContentLibrary;