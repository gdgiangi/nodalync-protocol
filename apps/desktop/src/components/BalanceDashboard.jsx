import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { formatHbar, hbarToTinybars, displayTransactionTime } from "../lib/fees";

const PAGE_SIZE = 20;
const errorMessage = (error) => typeof error === "string" ? error : error?.message || "The request failed. Please try again.";
const panelStyle = { background: "var(--bg-surface)", border: "1px solid var(--border-subtle)", borderRadius: 12 };
const fieldStyle = { color: "var(--text-primary)", fontSize: 14 };

export default function BalanceDashboard({ isOpen, onClose }) {
  const [feeConfig, setFeeConfig] = useState(null);
  const [history, setHistory] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [page, setPage] = useState(0);
  const [editingRate, setEditingRate] = useState(false);
  const [ratePercent, setRatePercent] = useState("5");
  const [saving, setSaving] = useState(false);
  const [rateError, setRateError] = useState("");
  const [savedMessage, setSavedMessage] = useState("");
  const [quotePrice, setQuotePrice] = useState("");
  const [quote, setQuote] = useState(null);
  const [quoting, setQuoting] = useState(false);
  const [quoteError, setQuoteError] = useState("");
  const requestRef = useRef(0);
  const quoteRequestRef = useRef(0);
  const closeRef = useRef(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  const loadData = useCallback(async () => {
    const request = ++requestRef.current;
    setLoading(true);
    setError("");
    const results = await Promise.allSettled([
      invoke("get_fee_config"),
      invoke("get_transaction_history", { limit: PAGE_SIZE, offset: page * PAGE_SIZE }),
    ]);
    if (request !== requestRef.current) return;
    const failures = [];
    if (results[0].status === "fulfilled") {
      const config = results[0].value;
      setFeeConfig(config);
      setRatePercent(String(config.rate_percent ?? config.rate * 100));
    } else failures.push(`Fee configuration: ${errorMessage(results[0].reason)}`);
    if (results[1].status === "fulfilled" && Array.isArray(results[1].value?.transactions)) {
      const response = results[1].value;
      setHistory(response);
      const lastPage = Math.max(0, Math.ceil(response.total_count / PAGE_SIZE) - 1);
      if (page > lastPage) setPage(lastPage);
    } else {
      setHistory(null);
      failures.push(`Transaction history: ${results[1].status === "rejected" ? errorMessage(results[1].reason) : "Unexpected response. Refresh to try again."}`);
    }
    setError(failures.join(" "));
    setLoading(false);
  }, [page]);

  useEffect(() => {
    if (!isOpen) return;
    loadData();
    return () => { requestRef.current += 1; };
  }, [isOpen, loadData]);

  useEffect(() => {
    if (!isOpen) return;
    const previousFocus = document.activeElement;
    setQuoting(false);
    closeRef.current?.focus();
    const handleKey = (event) => {
      if (event.key === "Escape") onCloseRef.current();
    };
    document.addEventListener("keydown", handleKey);
    return () => {
      document.removeEventListener("keydown", handleKey);
      quoteRequestRef.current += 1;
      previousFocus?.focus?.();
    };
  }, [isOpen]);

  async function saveRate(event) {
    event.preventDefault();
    setRateError("");
    setSavedMessage("");
    const amount = Number(ratePercent);
    if (!ratePercent.trim() || !Number.isFinite(amount) || amount < 0 || amount > 50) {
      setRateError("Enter a fee rate between 0% and 50%.");
      return;
    }
    setSaving(true);
    try {
      const updated = await invoke("set_fee_rate", { ratePercent: amount });
      setFeeConfig(updated);
      setEditingRate(false);
      quoteRequestRef.current += 1;
      setQuote(null);
      setQuoting(false);
      setSavedMessage(`Fee rate saved at ${updated.rate_percent ?? updated.rate * 100}%.`);
    } catch (failure) {
      setRateError(errorMessage(failure));
    } finally { setSaving(false); }
  }

  async function calculateQuote(event) {
    event.preventDefault();
    setQuoteError("");
    setQuote(null);
    let contentPrice;
    try { contentPrice = hbarToTinybars(quotePrice); }
    catch (failure) { setQuoteError(errorMessage(failure)); return; }
    const request = ++quoteRequestRef.current;
    setQuoting(true);
    try {
      const response = await invoke("get_fee_quote", { contentPrice });
      if (request === quoteRequestRef.current) setQuote(response);
    } catch (failure) {
      if (request === quoteRequestRef.current) setQuoteError(errorMessage(failure));
    } finally {
      if (request === quoteRequestRef.current) setQuoting(false);
    }
  }

  if (!isOpen) return null;
  const transactions = history?.transactions ?? [];
  const totalPages = Math.max(1, Math.ceil((history?.total_count ?? 0) / PAGE_SIZE));

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 sm:p-6"
      style={{ background: "rgba(0,0,0,0.65)", backdropFilter: "blur(6px)" }}
      onClick={(event) => { if (event.target === event.currentTarget) onClose(); }}>
      <section role="dialog" aria-modal="true" aria-labelledby="balance-title"
        className="w-full max-w-5xl flex flex-col overflow-hidden"
        style={{ ...panelStyle, maxHeight: "92vh", background: "var(--bg-base, #10131b)", boxShadow: "0 24px 80px #0008" }}>
        <header className="flex items-start justify-between gap-4 p-5 sm:p-6" style={{ borderBottom: "1px solid var(--border-subtle)" }}>
          <div>
            <h2 id="balance-title" className="text-xl font-semibold" style={{ color: "var(--text-primary)" }}>Fees & transaction history</h2>
            <p className="mt-1 text-sm" style={{ color: "var(--text-secondary)" }}>Review recorded payments and configure Studio’s application fee.</p>
          </div>
          <button ref={closeRef} onClick={onClose} className="btn" aria-label="Close fees and transaction history" style={{ fontSize: 14 }}>Close</button>
        </header>
        <div className="overflow-y-auto p-5 sm:p-6 space-y-6" aria-busy={loading}>
          <div className="flex items-center justify-between gap-3">
            <p className="text-sm" style={{ color: "var(--text-secondary)" }}>Amounts are shown in HBAR.</p>
            <button onClick={loadData} disabled={loading || saving || editingRate} className="btn" style={{ fontSize: 13 }}>{loading ? "Refreshing…" : "Refresh"}</button>
          </div>
          {error && <Notice error>{error}</Notice>}
          {loading && !feeConfig && !history && <p role="status" className="text-sm">Loading fees and transactions…</p>}
          {history && <>
            <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
              <Metric label="Recorded total" value={formatHbar(history.total_amount)} />
              <Metric label="Recorded application fees" value={formatHbar(history.total_app_fees)} />
              <Metric label="Transactions" value={history.total_count.toLocaleString()} />
            </div>
            <p className="text-xs" style={{ color: "var(--text-secondary)" }}>Totals include all recorded transactions, including pending and failed entries.</p>
          </>}
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <section className="p-4 space-y-4" style={panelStyle}>
              <h3 className="text-base font-medium" style={{ color: "var(--text-primary)" }}>Application fee</h3>
              {feeConfig ? <>
                {editingRate ? <form onSubmit={saveRate} className="space-y-3">
                  <label className="block text-sm" htmlFor="studio-fee-rate">Fee rate (%)</label>
                  <input id="studio-fee-rate" className="input w-full" style={fieldStyle} type="number" min="0" max="50" step="0.1"
                    value={ratePercent} onChange={(event) => { setRatePercent(event.target.value); setRateError(""); }} disabled={saving} />
                  <p className="text-xs" style={{ color: "var(--text-secondary)" }}>Applied on top of the content price. Choose 0% to charge no application fee.</p>
                  <div className="flex gap-2">
                    <button className="btn btn-accent" type="submit" disabled={saving}>{saving ? "Saving…" : "Save fee rate"}</button>
                    <button className="btn" type="button" disabled={saving} onClick={() => { setEditingRate(false); setRateError(""); }}>Cancel</button>
                  </div>
                </form> : <div className="flex items-center justify-between gap-3">
                  <p className="text-2xl font-semibold" style={{ color: "var(--text-primary)" }}>{(feeConfig.rate_percent ?? feeConfig.rate * 100).toLocaleString()}%</p>
                  <button className="btn" disabled={loading} onClick={() => { setRatePercent(String(feeConfig.rate_percent ?? feeConfig.rate * 100)); setEditingRate(true); setSavedMessage(""); }}>Edit fee rate</button>
                </div>}
                <p className="text-xs" style={{ color: "var(--text-secondary)" }}>Updated {displayTransactionTime(feeConfig.updated_at)}</p>
              </> : <p className="text-sm" style={{ color: "var(--text-secondary)" }}>Fee settings are unavailable. Refresh to try again.</p>}
              {rateError && <Notice error>{rateError}</Notice>}
              {savedMessage && <Notice>{savedMessage}</Notice>}
            </section>
            <section className="p-4 space-y-4" style={panelStyle}>
              <h3 className="text-base font-medium" style={{ color: "var(--text-primary)" }}>Fee calculator</h3>
              <form onSubmit={calculateQuote} className="space-y-3">
                <label className="block text-sm" htmlFor="studio-quote-price">Content price (HBAR)</label>
                <div className="flex gap-2">
                  <input id="studio-quote-price" className="input w-full min-w-0" style={fieldStyle} type="text" inputMode="decimal" placeholder="0.01"
                    value={quotePrice} onChange={(event) => { setQuotePrice(event.target.value); setQuote(null); setQuoteError(""); quoteRequestRef.current += 1; setQuoting(false); }} />
                  <button className="btn btn-accent" type="submit" disabled={quoting || !quotePrice.trim() || saving}>{quoting ? "Calculating…" : "Calculate"}</button>
                </div>
              </form>
              <p className="text-xs" style={{ color: "var(--text-secondary)" }}>Calculating a quote does not make a payment.</p>
              {quoteError && <Notice error>{quoteError}</Notice>}
              {quote && <dl className="space-y-2 text-sm">
                <AmountRow label="Content" amount={quote.content_cost} />
                <AmountRow label={`Application fee (${quote.fee_rate_percent}%)`} amount={quote.app_fee} />
                <AmountRow label="Total" amount={quote.total} bold />
              </dl>}
            </section>
          </div>
          <section style={panelStyle} className="overflow-hidden">
            <div className="p-4 flex items-center justify-between gap-2" style={{ borderBottom: "1px solid var(--border-subtle)" }}>
              <h3 className="text-base font-medium" style={{ color: "var(--text-primary)" }}>Transaction history</h3>
              <span className="text-xs" style={{ color: "var(--text-secondary)" }}>Newest first</span>
            </div>
            {!history ? <p className="p-5 text-sm" style={{ color: "var(--text-secondary)" }}>{loading ? "Loading transaction history…" : "Transaction history could not be loaded."}</p>
              : transactions.length === 0 ? <div className="p-8 text-center space-y-2">
                <p className="text-base" style={{ color: "var(--text-primary)" }}>No transactions yet</p>
                <p className="text-sm" style={{ color: "var(--text-secondary)" }}>Content queries made through Studio will appear here.</p>
              </div> : <>
                <div className="overflow-x-auto">
                  <table className="w-full text-sm" style={{ minWidth: 700 }}>
                    <thead style={{ color: "var(--text-secondary)", background: "var(--bg-elevated)" }}>
                      <tr>{["Content", "Content cost", "App fee", "Total", "Status", "Date"].map((label) => <th key={label} scope="col" className="px-4 py-3 text-left font-medium">{label}</th>)}</tr>
                    </thead>
                    <tbody>{transactions.map((transaction) => <tr key={transaction.id} style={{ borderTop: "1px solid var(--border-subtle)" }}>
                      <td className="px-4 py-3"><div className="font-medium" style={{ color: "var(--text-primary)" }}>{transaction.content_title || "Untitled content"}</div><div className="mt-1 text-xs font-mono" title={transaction.content_hash} style={{ color: "var(--text-secondary)" }}>{transaction.content_hash?.slice(0, 12)}…</div></td>
                      <td className="px-4 py-3 whitespace-nowrap">{formatHbar(transaction.content_cost)}</td>
                      <td className="px-4 py-3 whitespace-nowrap">{formatHbar(transaction.app_fee)}</td>
                      <td className="px-4 py-3 whitespace-nowrap font-medium">{formatHbar(transaction.total)}</td>
                      <td className="px-4 py-3"><Status value={transaction.status} /></td>
                      <td className="px-4 py-3 whitespace-nowrap" title={transaction.created_at}>{displayTransactionTime(transaction.created_at)}</td>
                    </tr>)}</tbody>
                  </table>
                </div>
                <div className="p-4 flex items-center justify-between gap-3 text-sm" style={{ borderTop: "1px solid var(--border-subtle)" }}>
                  <span style={{ color: "var(--text-secondary)" }}>Page {page + 1} of {totalPages} · {history.total_count} transactions</span>
                  <div className="flex gap-2"><button className="btn" disabled={loading || page === 0 || editingRate || saving} onClick={() => setPage((value) => value - 1)}>Previous</button><button className="btn" disabled={loading || page >= totalPages - 1 || editingRate || saving} onClick={() => setPage((value) => value + 1)}>Next</button></div>
                </div>
              </>}
          </section>
        </div>
      </section>
    </div>
  );
}

function Metric({ label, value }) {
  return <div className="p-4" style={panelStyle}><p className="text-xs mb-2" style={{ color: "var(--text-secondary)" }}>{label}</p><p className="text-xl font-semibold break-words" style={{ color: "var(--text-primary)" }}>{value}</p></div>;
}

function AmountRow({ label, amount, bold }) {
  return <div className={`flex items-center justify-between gap-3 ${bold ? "font-semibold" : ""}`}><dt>{label}</dt><dd>{formatHbar(amount)}</dd></div>;
}

function Notice({ children, error = false }) {
  return <p role={error ? "alert" : "status"} className="text-sm rounded-lg p-3" style={{ color: error ? "var(--red, #fca5a5)" : "var(--green, #86efac)", background: error ? "rgba(248,113,113,0.08)" : "rgba(74,222,128,0.06)" }}>{children}</p>;
}

function Status({ value }) {
  const status = String(value || "Unknown");
  const kind = status.toLowerCase();
  const color = kind === "settled" || kind === "free" ? "var(--green, #86efac)" : kind === "failed" ? "var(--red, #fca5a5)" : "var(--yellow, #fde68a)";
  return <span className="text-xs font-medium" style={{ color }}>{status}</span>;
}
