import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * Format satoshis/sats to human-readable with unit
 */
function formatSats(amount) {
  if (amount == null) return "—";
  const sats = Number(amount);
  if (sats >= 100_000_000) {
    return `${(sats / 100_000_000).toFixed(4)} BTC`;
  }
  if (sats >= 1_000_000) {
    return `${(sats / 1_000_000).toFixed(2)}M sats`;
  }
  if (sats >= 1_000) {
    return `${(sats / 1_000).toFixed(1)}K sats`;
  }
  return `${sats.toLocaleString()} sats`;
}

/**
 * Format timestamp to relative/short date
 */
function formatTime(ts) {
  if (!ts) return "—";
  const d = new Date(ts);
  const now = new Date();
  const diff = now - d;
  if (diff < 60_000) return "just now";
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`;
  if (diff < 604_800_000) return `${Math.floor(diff / 86_400_000)}d ago`;
  return d.toLocaleDateString("en-US", { month: "short", day: "numeric" });
}

/**
 * Transaction status → pill style
 */
function statusStyle(status) {
  const s = (status || "").toLowerCase();
  if (s === "completed" || s === "complete" || s === "success") return "pill-green";
  if (s === "pending" || s === "processing") return "pill-yellow";
  if (s === "failed" || s === "error" || s === "rejected") return "pill-red";
  return "pill-neutral";
}

/**
 * BalanceDashboard — x402 status, fee config, transaction history
 *
 * Full-width panel that can be opened as a page/tab in the app.
 * Shows:
 * - Fee configuration (rate, min, max) with editable rate slider
 * - Transaction summary stats (total earned, total spent, count)
 * - Transaction history table with sorting
 * - Fee calculator
 */
export default function BalanceDashboard({ isOpen, onClose }) {
  const [feeConfig, setFeeConfig] = useState(null);
  const [transactions, setTransactions] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [editingRate, setEditingRate] = useState(false);
  const [newRate, setNewRate] = useState(0.05);
  const [savingRate, setSavingRate] = useState(false);
  const [quotePrice, setQuotePrice] = useState("");
  const [feeQuote, setFeeQuote] = useState(null);
  const [animateIn, setAnimateIn] = useState(false);
  const [sortField, setSortField] = useState("timestamp");
  const [sortDir, setSortDir] = useState("desc");
  const [txPage, setTxPage] = useState(0);
  const TX_PER_PAGE = 20;

  useEffect(() => {
    if (isOpen) {
      loadData();
      requestAnimationFrame(() => setAnimateIn(true));
    } else {
      setAnimateIn(false);
    }
  }, [isOpen]);

  async function loadData() {
    setLoading(true);
    setError(null);
    try {
      const [config, txHistory] = await Promise.all([
        invoke("get_fee_config").catch(() => null),
        invoke("get_transaction_history", { limit: 200 }).catch(() => []),
      ]);
      setFeeConfig(config);
      if (config) setNewRate(config.rate || 0.05);
      setTransactions(Array.isArray(txHistory) ? txHistory : []);
    } catch (err) {
      console.error("Failed to load balance data:", err);
      setError(typeof err === "string" ? err : err?.message || "Failed to load data");
    } finally {
      setLoading(false);
    }
  }

  // Save fee rate
  const handleSaveRate = useCallback(async () => {
    try {
      setSavingRate(true);
      const updated = await invoke("set_fee_rate", { rate: newRate });
      setFeeConfig(updated);
      setEditingRate(false);
    } catch (err) {
      console.error("Failed to set fee rate:", err);
    } finally {
      setSavingRate(false);
    }
  }, [newRate]);

  // Fee quote calculator
  const handleQuote = useCallback(async () => {
    const price = parseInt(quotePrice, 10);
    if (isNaN(price) || price <= 0) return;
    try {
      const quote = await invoke("get_fee_quote", { contentPrice: price });
      setFeeQuote(quote);
    } catch (err) {
      console.error("Fee quote failed:", err);
    }
  }, [quotePrice]);

  const handleClose = useCallback(() => {
    setAnimateIn(false);
    setTimeout(onClose, 250);
  }, [onClose]);

  // Computed stats
  const totalEarned = transactions
    .filter((tx) => tx.app_fee > 0)
    .reduce((sum, tx) => sum + (tx.app_fee || 0), 0);
  const totalSpent = transactions
    .reduce((sum, tx) => sum + (tx.total_cost || 0), 0);
  const completedCount = transactions.filter(
    (tx) => (tx.status || "").toLowerCase() === "completed" || (tx.status || "").toLowerCase() === "complete"
  ).length;

  // Sort transactions
  const sortedTx = [...transactions].sort((a, b) => {
    let aVal = a[sortField];
    let bVal = b[sortField];
    if (sortField === "timestamp") {
      aVal = new Date(aVal || 0).getTime();
      bVal = new Date(bVal || 0).getTime();
    }
    if (typeof aVal === "string") aVal = aVal.toLowerCase();
    if (typeof bVal === "string") bVal = bVal.toLowerCase();
    if (aVal < bVal) return sortDir === "asc" ? -1 : 1;
    if (aVal > bVal) return sortDir === "asc" ? 1 : -1;
    return 0;
  });

  const pagedTx = sortedTx.slice(txPage * TX_PER_PAGE, (txPage + 1) * TX_PER_PAGE);
  const totalPages = Math.ceil(sortedTx.length / TX_PER_PAGE);

  function toggleSort(field) {
    if (sortField === field) {
      setSortDir((d) => (d === "asc" ? "desc" : "asc"));
    } else {
      setSortField(field);
      setSortDir("desc");
    }
    setTxPage(0);
  }

  if (!isOpen) return null;

  return (
    <>
      {/* Backdrop */}
      <div
        className="fixed inset-0 z-50"
        style={{
          background: animateIn ? "rgba(0, 0, 0, 0.4)" : "transparent",
          backdropFilter: animateIn ? "blur(4px)" : "none",
          transition: "all 0.25s ease",
        }}
        onClick={handleClose}
      />

      {/* Panel */}
      <div
        className="fixed inset-0 z-50 flex items-stretch justify-center p-4"
        style={{ pointerEvents: "none" }}
      >
        <div
          className="w-full max-w-4xl flex flex-col"
          style={{
            pointerEvents: "auto",
            background: "rgba(10, 10, 16, 0.96)",
            backdropFilter: "blur(32px)",
            WebkitBackdropFilter: "blur(32px)",
            border: "1px solid var(--border-subtle)",
            borderRadius: "var(--radius-lg)",
            boxShadow: "0 24px 80px rgba(0, 0, 0, 0.5)",
            opacity: animateIn ? 1 : 0,
            transform: animateIn ? "translateY(0) scale(1)" : "translateY(12px) scale(0.98)",
            transition: "all 0.3s cubic-bezier(0.4, 0, 0.2, 1)",
          }}
          onClick={(e) => e.stopPropagation()}
        >
          {/* Header */}
          <div
            className="flex items-center justify-between px-6 py-3.5 flex-shrink-0"
            style={{ borderBottom: "1px solid var(--border-subtle)" }}
          >
            <div className="flex items-center gap-3">
              <div
                className="w-7 h-7 rounded-md flex items-center justify-center"
                style={{
                  background: "rgba(250, 204, 21, 0.08)",
                  border: "1px solid rgba(250, 204, 21, 0.2)",
                }}
              >
                <svg
                  width="14"
                  height="14"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  style={{ color: "var(--yellow)" }}
                >
                  <line x1="12" y1="1" x2="12" y2="23" />
                  <path d="M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6" />
                </svg>
              </div>
              <div>
                <h2
                  className="text-[13px] font-medium"
                  style={{ color: "var(--text-primary)", letterSpacing: "0.3px" }}
                >
                  Balance & Transactions
                </h2>
                <span className="text-[9px]" style={{ color: "var(--text-ghost)" }}>
                  x402 Protocol — Fee Management
                </span>
              </div>
            </div>

            <div className="flex items-center gap-2">
              <button
                onClick={loadData}
                className="btn text-[10px]"
                disabled={loading}
                title="Refresh data"
              >
                <svg
                  width="10"
                  height="10"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  style={{ animation: loading ? "spin 1s linear infinite" : "none" }}
                >
                  <polyline points="23 4 23 10 17 10" />
                  <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" />
                </svg>
                Refresh
              </button>
              <button
                onClick={handleClose}
                className="w-7 h-7 flex items-center justify-center rounded-md"
                style={{ color: "var(--text-ghost)", transition: "all 0.15s ease" }}
                onMouseEnter={(e) => {
                  e.currentTarget.style.background = "var(--bg-hover)";
                  e.currentTarget.style.color = "var(--text-secondary)";
                }}
                onMouseLeave={(e) => {
                  e.currentTarget.style.background = "transparent";
                  e.currentTarget.style.color = "var(--text-ghost)";
                }}
              >
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <line x1="18" y1="6" x2="6" y2="18" />
                  <line x1="6" y1="6" x2="18" y2="18" />
                </svg>
              </button>
            </div>
          </div>

          {/* Content */}
          <div className="flex-1 overflow-y-auto px-6 py-4" style={{ minHeight: 0 }}>
            {loading && !feeConfig && (
              <div className="flex items-center justify-center py-20">
                <div className="flex items-center gap-3">
                  <div
                    className="w-4 h-4 rounded-full border-2"
                    style={{
                      borderColor: "rgba(92, 124, 250, 0.15)",
                      borderTopColor: "var(--accent)",
                      animation: "spin 0.8s linear infinite",
                    }}
                  />
                  <span className="text-[11px]" style={{ color: "var(--text-tertiary)" }}>
                    Loading balance data...
                  </span>
                </div>
              </div>
            )}

            {error && (
              <div
                className="rounded-lg p-4 mb-4"
                style={{
                  background: "var(--red-dim)",
                  border: "1px solid rgba(248, 113, 113, 0.2)",
                }}
              >
                <span className="text-[11px]" style={{ color: "var(--red)" }}>
                  {error}
                </span>
              </div>
            )}

            {!loading && feeConfig && (
              <>
                {/* Stats cards row */}
                <div className="grid grid-cols-4 gap-3 mb-5">
                  <StatCard
                    label="FEE RATE"
                    value={`${((feeConfig.rate || 0) * 100).toFixed(1)}%`}
                    accent="yellow"
                    sub={feeConfig.fee_recipient || "app"}
                  />
                  <StatCard
                    label="TOTAL EARNED"
                    value={formatSats(totalEarned)}
                    accent="green"
                    sub={`from ${completedCount} txns`}
                  />
                  <StatCard
                    label="TOTAL VOLUME"
                    value={formatSats(totalSpent)}
                    accent="accent"
                    sub={`${transactions.length} transactions`}
                  />
                  <StatCard
                    label="AVG FEE"
                    value={transactions.length > 0 ? formatSats(Math.round(totalEarned / transactions.length)) : "—"}
                    accent="neutral"
                    sub="per transaction"
                  />
                </div>

                {/* Fee config + Quote — side by side */}
                <div className="grid grid-cols-2 gap-4 mb-5">
                  {/* Fee Configuration */}
                  <div
                    className="rounded-lg p-4"
                    style={{
                      background: "var(--bg-surface)",
                      border: "1px solid var(--border-subtle)",
                    }}
                  >
                    <div className="flex items-center justify-between mb-3">
                      <span className="label-sm" style={{ color: "var(--text-label)" }}>
                        FEE CONFIGURATION
                      </span>
                      {!editingRate && (
                        <button
                          onClick={() => setEditingRate(true)}
                          className="text-[9px] px-2 py-0.5 rounded"
                          style={{
                            color: "var(--accent)",
                            background: "var(--accent-dim)",
                            border: "1px solid rgba(92, 124, 250, 0.2)",
                            cursor: "pointer",
                          }}
                        >
                          Edit
                        </button>
                      )}
                    </div>

                    {editingRate ? (
                      <div className="space-y-3">
                        <div>
                          <label className="text-[10px] block mb-1.5" style={{ color: "var(--text-tertiary)" }}>
                            Application Fee Rate
                          </label>
                          <div className="flex items-center gap-3">
                            <input
                              type="range"
                              min="0"
                              max="0.25"
                              step="0.005"
                              value={newRate}
                              onChange={(e) => setNewRate(parseFloat(e.target.value))}
                              className="flex-1 h-1 rounded-full appearance-none"
                              style={{
                                background: `linear-gradient(to right, var(--accent) ${(newRate / 0.25) * 100}%, rgba(255,255,255,0.06) ${(newRate / 0.25) * 100}%)`,
                                cursor: "pointer",
                              }}
                            />
                            <span
                              className="mono text-[14px] font-light w-14 text-right"
                              style={{ color: "var(--text-primary)" }}
                            >
                              {(newRate * 100).toFixed(1)}%
                            </span>
                          </div>
                        </div>

                        <div className="flex items-center gap-2 pt-1">
                          <button
                            onClick={handleSaveRate}
                            disabled={savingRate}
                            className="btn text-[10px] btn-accent"
                          >
                            {savingRate ? "Saving..." : "Save Rate"}
                          </button>
                          <button
                            onClick={() => {
                              setEditingRate(false);
                              setNewRate(feeConfig.rate || 0.05);
                            }}
                            className="btn text-[10px]"
                          >
                            Cancel
                          </button>
                        </div>
                      </div>
                    ) : (
                      <div className="space-y-2">
                        <ConfigRow label="Rate" value={`${((feeConfig.rate || 0) * 100).toFixed(1)}%`} />
                        <ConfigRow label="Min Fee" value={feeConfig.min_fee ? formatSats(feeConfig.min_fee) : "None"} />
                        <ConfigRow label="Max Fee" value={feeConfig.max_fee ? formatSats(feeConfig.max_fee) : "No cap"} />
                        <ConfigRow label="Recipient" value={feeConfig.fee_recipient || "nodalync-app"} />
                      </div>
                    )}
                  </div>

                  {/* Fee Calculator */}
                  <div
                    className="rounded-lg p-4"
                    style={{
                      background: "var(--bg-surface)",
                      border: "1px solid var(--border-subtle)",
                    }}
                  >
                    <span className="label-sm block mb-3" style={{ color: "var(--text-label)" }}>
                      FEE CALCULATOR
                    </span>

                    <div className="flex items-center gap-2 mb-3">
                      <input
                        type="number"
                        value={quotePrice}
                        onChange={(e) => {
                          setQuotePrice(e.target.value);
                          setFeeQuote(null);
                        }}
                        onKeyDown={(e) => e.key === "Enter" && handleQuote()}
                        placeholder="Content price (sats)"
                        className="input text-[12px] flex-1"
                        min="0"
                      />
                      <button
                        onClick={handleQuote}
                        disabled={!quotePrice}
                        className="btn btn-accent text-[10px]"
                      >
                        Calculate
                      </button>
                    </div>

                    {feeQuote && (
                      <div
                        className="rounded-md p-3 animate-fade-in"
                        style={{
                          background: "var(--bg-elevated)",
                          border: "1px solid var(--border-subtle)",
                        }}
                      >
                        <div className="space-y-1.5">
                          <QuoteRow label="Content Price" value={formatSats(feeQuote.content_price || quotePrice)} />
                          <QuoteRow label="App Fee" value={formatSats(feeQuote.app_fee)} accent />
                          <div className="divider my-1.5" />
                          <QuoteRow label="Total Cost" value={formatSats(feeQuote.total_cost)} bold />
                        </div>
                      </div>
                    )}

                    {!feeQuote && (
                      <div className="text-center py-4">
                        <span className="text-[10px]" style={{ color: "var(--text-ghost)" }}>
                          Enter a content price to see the fee breakdown
                        </span>
                      </div>
                    )}
                  </div>
                </div>

                {/* Transaction History */}
                <div
                  className="rounded-lg"
                  style={{
                    background: "var(--bg-surface)",
                    border: "1px solid var(--border-subtle)",
                  }}
                >
                  <div
                    className="flex items-center justify-between px-4 py-2.5"
                    style={{ borderBottom: "1px solid var(--border-subtle)" }}
                  >
                    <span className="label-sm" style={{ color: "var(--text-label)" }}>
                      TRANSACTION HISTORY
                    </span>
                    <span className="mono text-[9px]" style={{ color: "var(--text-ghost)" }}>
                      {transactions.length} total
                    </span>
                  </div>

                  {transactions.length === 0 ? (
                    <div className="text-center py-12">
                      <div
                        className="w-12 h-12 mx-auto mb-3 rounded-full flex items-center justify-center"
                        style={{
                          background: "var(--bg-elevated)",
                          border: "1px solid var(--border-subtle)",
                        }}
                      >
                        <svg
                          width="20"
                          height="20"
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          strokeWidth="1.5"
                          strokeLinecap="round"
                          strokeLinejoin="round"
                          style={{ color: "var(--text-ghost)" }}
                        >
                          <rect x="1" y="4" width="22" height="16" rx="2" ry="2" />
                          <line x1="1" y1="10" x2="23" y2="10" />
                        </svg>
                      </div>
                      <p className="text-[11px]" style={{ color: "var(--text-tertiary)" }}>
                        No transactions yet
                      </p>
                      <p className="text-[9px] mt-0.5" style={{ color: "var(--text-ghost)" }}>
                        Transactions will appear here when content is queried via x402
                      </p>
                    </div>
                  ) : (
                    <>
                      {/* Table header */}
                      <div
                        className="grid px-4 py-2"
                        style={{
                          gridTemplateColumns: "1fr 100px 100px 80px 80px 80px",
                          gap: "8px",
                          borderBottom: "1px solid var(--border-subtle)",
                        }}
                      >
                        <SortHeader label="Content" field="content_title" current={sortField} dir={sortDir} onSort={toggleSort} />
                        <SortHeader label="Cost" field="content_cost" current={sortField} dir={sortDir} onSort={toggleSort} />
                        <SortHeader label="App Fee" field="app_fee" current={sortField} dir={sortDir} onSort={toggleSort} />
                        <SortHeader label="Total" field="total_cost" current={sortField} dir={sortDir} onSort={toggleSort} />
                        <SortHeader label="Status" field="status" current={sortField} dir={sortDir} onSort={toggleSort} />
                        <SortHeader label="Time" field="timestamp" current={sortField} dir={sortDir} onSort={toggleSort} />
                      </div>

                      {/* Rows */}
                      {pagedTx.map((tx, idx) => (
                        <div
                          key={tx.id || idx}
                          className="grid px-4 py-2"
                          style={{
                            gridTemplateColumns: "1fr 100px 100px 80px 80px 80px",
                            gap: "8px",
                            borderBottom: "1px solid rgba(255,255,255,0.02)",
                            transition: "background 0.1s ease",
                          }}
                          onMouseEnter={(e) => (e.currentTarget.style.background = "var(--bg-hover)")}
                          onMouseLeave={(e) => (e.currentTarget.style.background = "transparent")}
                        >
                          <div className="flex flex-col justify-center min-w-0">
                            <span
                              className="text-[11px] truncate"
                              style={{ color: "var(--text-primary)" }}
                              title={tx.content_title}
                            >
                              {tx.content_title || "Untitled"}
                            </span>
                            <span
                              className="mono text-[7px] truncate"
                              style={{ color: "var(--text-ghost)" }}
                              title={tx.content_hash}
                            >
                              {tx.content_hash ? tx.content_hash.slice(0, 12) + "…" : "—"}
                            </span>
                          </div>
                          <div className="flex items-center">
                            <span className="mono text-[10px]" style={{ color: "var(--text-secondary)" }}>
                              {formatSats(tx.content_cost)}
                            </span>
                          </div>
                          <div className="flex items-center">
                            <span className="mono text-[10px]" style={{ color: "var(--yellow)" }}>
                              {formatSats(tx.app_fee)}
                            </span>
                          </div>
                          <div className="flex items-center">
                            <span className="mono text-[10px] font-medium" style={{ color: "var(--text-primary)" }}>
                              {formatSats(tx.total_cost)}
                            </span>
                          </div>
                          <div className="flex items-center">
                            <span className={`pill ${statusStyle(tx.status)}`} style={{ fontSize: 7 }}>
                              {tx.status || "—"}
                            </span>
                          </div>
                          <div className="flex items-center">
                            <span className="text-[9px]" style={{ color: "var(--text-ghost)" }}>
                              {formatTime(tx.timestamp)}
                            </span>
                          </div>
                        </div>
                      ))}

                      {/* Pagination */}
                      {totalPages > 1 && (
                        <div
                          className="flex items-center justify-between px-4 py-2"
                          style={{ borderTop: "1px solid var(--border-subtle)" }}
                        >
                          <span className="mono text-[9px]" style={{ color: "var(--text-ghost)" }}>
                            Page {txPage + 1} of {totalPages}
                          </span>
                          <div className="flex items-center gap-1">
                            <button
                              onClick={() => setTxPage((p) => Math.max(0, p - 1))}
                              disabled={txPage === 0}
                              className="btn text-[9px] px-2 py-1"
                              style={{ opacity: txPage === 0 ? 0.3 : 1 }}
                            >
                              ← Prev
                            </button>
                            <button
                              onClick={() => setTxPage((p) => Math.min(totalPages - 1, p + 1))}
                              disabled={txPage >= totalPages - 1}
                              className="btn text-[9px] px-2 py-1"
                              style={{ opacity: txPage >= totalPages - 1 ? 0.3 : 1 }}
                            >
                              Next →
                            </button>
                          </div>
                        </div>
                      )}
                    </>
                  )}
                </div>
              </>
            )}
          </div>
        </div>
      </div>
    </>
  );
}

/**
 * Stat card — top-level metric
 */
function StatCard({ label, value, accent, sub }) {
  const colors = {
    yellow: { bg: "rgba(250, 204, 21, 0.06)", border: "rgba(250, 204, 21, 0.15)", text: "var(--yellow)" },
    green: { bg: "rgba(74, 222, 128, 0.06)", border: "rgba(74, 222, 128, 0.15)", text: "var(--green)" },
    accent: { bg: "var(--accent-dim)", border: "rgba(92, 124, 250, 0.15)", text: "var(--accent)" },
    neutral: { bg: "var(--bg-surface)", border: "var(--border-subtle)", text: "var(--text-secondary)" },
  };
  const c = colors[accent] || colors.neutral;

  return (
    <div
      className="rounded-lg p-3 animate-slide-up"
      style={{
        background: c.bg,
        border: `1px solid ${c.border}`,
      }}
    >
      <span className="label-xs block mb-1.5">{label}</span>
      <span
        className="mono text-[18px] font-light block"
        style={{ color: c.text, letterSpacing: "-0.5px" }}
      >
        {value}
      </span>
      {sub && (
        <span className="text-[8px] block mt-0.5" style={{ color: "var(--text-ghost)" }}>
          {sub}
        </span>
      )}
    </div>
  );
}

/**
 * Config row — key/value display
 */
function ConfigRow({ label, value }) {
  return (
    <div className="flex items-center justify-between">
      <span className="text-[10px]" style={{ color: "var(--text-ghost)" }}>
        {label}
      </span>
      <span className="mono text-[10px]" style={{ color: "var(--text-secondary)" }}>
        {value}
      </span>
    </div>
  );
}

/**
 * Quote row — fee breakdown display
 */
function QuoteRow({ label, value, accent, bold }) {
  return (
    <div className="flex items-center justify-between">
      <span className="text-[10px]" style={{ color: "var(--text-tertiary)" }}>
        {label}
      </span>
      <span
        className={`mono text-[11px] ${bold ? "font-medium" : ""}`}
        style={{ color: accent ? "var(--yellow)" : bold ? "var(--text-primary)" : "var(--text-secondary)" }}
      >
        {value}
      </span>
    </div>
  );
}

/**
 * Sortable column header
 */
function SortHeader({ label, field, current, dir, onSort }) {
  const active = current === field;
  return (
    <button
      onClick={() => onSort(field)}
      className="flex items-center gap-1 text-left"
      style={{
        background: "transparent",
        border: "none",
        cursor: "pointer",
        padding: 0,
      }}
    >
      <span
        className="label-xs"
        style={{
          color: active ? "var(--text-secondary)" : "var(--text-ghost)",
          letterSpacing: 1.5,
        }}
      >
        {label}
      </span>
      {active && (
        <svg
          width="8"
          height="8"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          style={{
            color: "var(--text-ghost)",
            transform: dir === "asc" ? "rotate(180deg)" : "rotate(0)",
            transition: "transform 0.15s ease",
          }}
        >
          <polyline points="6 9 12 15 18 9" />
        </svg>
      )}
    </button>
  );
}
