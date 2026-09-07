const TINYBARS_PER_HBAR = 100_000_000;

export function formatHbar(tinybars) {
  const amount = Number(tinybars);
  if (tinybars == null || !Number.isFinite(amount)) return "—";
  return `${(amount / TINYBARS_PER_HBAR).toLocaleString("en-US", {
    maximumFractionDigits: 8,
  })} HBAR`;
}

// Parse decimal HBAR without rounding away a tinybar or accepting partial input.
export function hbarToTinybars(input) {
  const value = String(input).trim();
  if (!/^\d+(?:\.\d{1,8})?$/.test(value)) {
    throw new Error("Enter a non-negative HBAR amount with up to 8 decimal places.");
  }
  const [whole, fraction = ""] = value.split(".");
  const tinybars = Number(whole) * TINYBARS_PER_HBAR + Number(fraction.padEnd(8, "0"));
  if (!Number.isSafeInteger(tinybars)) {
    throw new Error("This amount is too large. Enter a smaller HBAR amount.");
  }
  return tinybars;
}

export function displayTransactionTime(createdAt) {
  const date = new Date(createdAt);
  if (!createdAt || !Number.isFinite(date.getTime())) return "—";
  return date.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}
