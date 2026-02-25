import type { IgnoreConfig, SpendingConfig, TransactionIgnoreRule } from '../config.js';

export type TransactionIgnoreInput = {
  account_id: string;
  account_name: string;
  connection_id: string;
  connection_name: string;
  synchronizer: string;
  description: string;
  status: string;
  amount: string;
};

const EMPTY_SPENDING_CONFIG: SpendingConfig = {
  ignore_accounts: [],
  ignore_connections: [],
  ignore_tags: [],
};

type CompiledTransactionIgnoreRule = {
  account_id?: RegExp;
  account_name?: RegExp;
  connection_id?: RegExp;
  connection_name?: RegExp;
  synchronizer?: RegExp;
  description?: RegExp;
  status?: RegExp;
  amount?: RegExp;
};

function compileField(
  ruleIndex: number,
  field: keyof CompiledTransactionIgnoreRule,
  pattern: string | undefined,
): RegExp | undefined {
  if (pattern === undefined) return undefined;
  const trimmed = pattern.trim();
  if (trimmed === '') return undefined;

  let source = trimmed;
  const flags = new Set<string>();
  while (true) {
    const match = source.match(/^\(\?([a-z]+)\)/);
    if (match === null) break;

    for (const flag of match[1]) {
      if (flag !== 'i' && flag !== 'm' && flag !== 's') {
        throw new Error(
          `Invalid ignore.transaction_rules[${String(ruleIndex)}].${String(field)} regex: ${trimmed} (unsupported inline flag '${flag}')`,
        );
      }
      flags.add(flag);
    }
    source = source.slice(match[0].length);
  }

  const sortedFlags = [...flags].sort().join('');
  try {
    return new RegExp(source, sortedFlags);
  } catch (error) {
    const msg = error instanceof Error ? error.message : String(error);
    throw new Error(`Invalid ignore.transaction_rules[${String(ruleIndex)}].${String(field)} regex: ${trimmed} (${msg})`);
  }
}

function compileRule(ruleIndex: number, rule: TransactionIgnoreRule): CompiledTransactionIgnoreRule {
  const compiled: CompiledTransactionIgnoreRule = {
    account_id: compileField(ruleIndex, 'account_id', rule.account_id),
    account_name: compileField(ruleIndex, 'account_name', rule.account_name),
    connection_id: compileField(ruleIndex, 'connection_id', rule.connection_id),
    connection_name: compileField(ruleIndex, 'connection_name', rule.connection_name),
    synchronizer: compileField(ruleIndex, 'synchronizer', rule.synchronizer),
    description: compileField(ruleIndex, 'description', rule.description),
    status: compileField(ruleIndex, 'status', rule.status),
    amount: compileField(ruleIndex, 'amount', rule.amount),
  };

  const hasAnyField = Object.values(compiled).some((v) => v !== undefined);
  if (!hasAnyField) {
    throw new Error(`ignore.transaction_rules[${String(ruleIndex)}] must specify at least one regex field`);
  }
  return compiled;
}

export function compileTransactionIgnoreRules(config: IgnoreConfig): CompiledTransactionIgnoreRule[] {
  return compileTransactionIgnoreRulesWithSpending(config, EMPTY_SPENDING_CONFIG);
}

function escapeRegexLiteral(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function exactCaseInsensitivePattern(raw: string): string | undefined {
  const trimmed = raw.trim();
  if (trimmed === '') return undefined;
  return `(?i)^${escapeRegexLiteral(trimmed)}$`;
}

function synthesizeSpendingIgnoreRules(spending: SpendingConfig): TransactionIgnoreRule[] {
  const derived: TransactionIgnoreRule[] = [];

  for (const value of spending.ignore_accounts) {
    const pattern = exactCaseInsensitivePattern(value);
    if (pattern === undefined) continue;
    derived.push({ account_id: pattern });
    derived.push({ account_name: pattern });
  }

  for (const value of spending.ignore_connections) {
    const pattern = exactCaseInsensitivePattern(value);
    if (pattern === undefined) continue;
    derived.push({ connection_id: pattern });
    derived.push({ connection_name: pattern });
  }

  return derived;
}

export function compileTransactionIgnoreRulesWithSpending(
  config: IgnoreConfig,
  spending: SpendingConfig,
): CompiledTransactionIgnoreRule[] {
  const combined: TransactionIgnoreRule[] = [
    ...config.transaction_rules,
    ...synthesizeSpendingIgnoreRules(spending),
  ];
  return combined.map((rule, idx) => compileRule(idx, rule));
}

function matchField(pattern: RegExp | undefined, value: string): boolean {
  return pattern ? pattern.test(value) : true;
}

export function shouldIgnoreTransaction(
  compiledRules: CompiledTransactionIgnoreRule[],
  input: TransactionIgnoreInput,
): boolean {
  return compiledRules.some(
    (rule) =>
      matchField(rule.account_id, input.account_id) &&
      matchField(rule.account_name, input.account_name) &&
      matchField(rule.connection_id, input.connection_id) &&
      matchField(rule.connection_name, input.connection_name) &&
      matchField(rule.synchronizer, input.synchronizer) &&
      matchField(rule.description, input.description) &&
      matchField(rule.status, input.status) &&
      matchField(rule.amount, input.amount),
  );
}
