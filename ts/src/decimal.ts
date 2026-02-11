import { Decimal } from 'decimal.js';

// Match Rust `rust_decimal` behavior more closely.
// Values in this app are typically represented with ~29 significant digits
// (up to 28 fractional decimal places plus integer digits).
Decimal.set({ precision: 29 });

export { Decimal };
