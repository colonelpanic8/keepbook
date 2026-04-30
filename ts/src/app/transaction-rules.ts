import fs from 'node:fs/promises';
import path from 'node:path';

import type { ResolvedConfig } from '../config.js';
import { SystemClock, type Clock } from '../clock.js';
import { type Storage } from '../storage/storage.js';
import {
  type TransactionAnnotationPatchType,
  type TransactionAnnotationType,
  applyTransactionAnnotationPatch,
} from '../models/transaction-annotation.js';
import { formatRfc3339 } from './format.js';

export const TRANSACTION_RULES_FILE = 'transaction_category_rules.jsonl';

export interface TransactionAnnotationRule {
  category?: string;
  description_override?: string;
  account_id?: string;
  account_name?: string;
  description?: string;
  status?: string;
  amount?: string;
}

export interface TransactionAnnotationRuleInput {
  account_id: string;
  account_name: string;
  description: string;
  status: string;
  amount: string;
}

export interface TransactionAnnotationRuleAction {
  rule_index: number;
  category?: string;
  description_override?: string;
}

type CompiledTransactionAnnotationRule = {
  rule_index: number;
  category?: string;
  description_override?: string;
  account_id?: RegExp;
  account_name?: RegExp;
  description?: RegExp;
  status?: RegExp;
  amount?: RegExp;
};

export function transactionRulesPath(dataDir: string): string {
  return path.join(dataDir, TRANSACTION_RULES_FILE);
}

export function exactCaseInsensitivePattern(raw: string): string | undefined {
  const trimmed = raw.trim();
  if (trimmed === '') return undefined;
  return `(?i)^${escapeRegexLiteral(trimmed)}$`;
}

export async function loadTransactionAnnotationRules(rulesPath: string): Promise<{
  matcher: TransactionAnnotationRuleMatcher;
  warning?: string;
  skipped_invalid_rule_count: number;
}> {
  let raw: string;
  try {
    raw = await fs.readFile(rulesPath, 'utf-8');
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
      return {
        matcher: new TransactionAnnotationRuleMatcher([]),
        skipped_invalid_rule_count: 0,
      };
    }
    throw error;
  }

  const rules: CompiledTransactionAnnotationRule[] = [];
  let skipped = 0;
  raw.split(/\r?\n/).forEach((line, lineIndex) => {
    const trimmed = line.trim();
    if (trimmed === '') return;

    try {
      const parsed = JSON.parse(trimmed) as TransactionAnnotationRule;
      rules.push(compileRule(lineIndex, parsed));
    } catch {
      skipped += 1;
    }
  });

  return {
    matcher: new TransactionAnnotationRuleMatcher(rules),
    skipped_invalid_rule_count: skipped,
    ...(skipped > 0
      ? { warning: `Skipped ${skipped} invalid transaction rules from ${rulesPath}` }
      : {}),
  };
}

export async function appendTransactionAnnotationRule(
  rulesPath: string,
  rule: TransactionAnnotationRule,
): Promise<void> {
  await fs.mkdir(path.dirname(rulesPath), { recursive: true });
  await fs.appendFile(rulesPath, JSON.stringify(rule) + '\n', 'utf-8');
}

export class TransactionAnnotationRuleMatcher {
  constructor(private readonly rules: CompiledTransactionAnnotationRule[]) {}

  ruleCount(): number {
    return this.rules.length;
  }

  matchAnnotation(
    input: TransactionAnnotationRuleInput,
  ): TransactionAnnotationRuleAction | undefined {
    const rule = this.rules.find((candidate) => ruleMatches(candidate, input));
    if (rule === undefined) return undefined;
    return {
      rule_index: rule.rule_index,
      ...(rule.category !== undefined ? { category: rule.category } : {}),
      ...(rule.description_override !== undefined
        ? { description_override: rule.description_override }
        : {}),
    };
  }
}

export interface ApplyTransactionAnnotationRulesOptions {
  rules_path?: string;
  dry_run?: boolean;
  overwrite?: boolean;
}

export interface AppliedTransactionRuleChange {
  rule_index: number;
  account_id: string;
  account_name: string;
  transaction_id: string;
  timestamp: string;
  amount: string;
  original_description: string;
  category?: string;
  description?: string;
}

export async function applyTransactionAnnotationRules(
  storage: Storage,
  config: ResolvedConfig,
  options: ApplyTransactionAnnotationRulesOptions = {},
  clock: Clock = new SystemClock(),
): Promise<object> {
  const rulesPath = options.rules_path ?? transactionRulesPath(config.data_dir);
  const loaded = await loadTransactionAnnotationRules(rulesPath);
  const matcher = loaded.matcher;
  const dryRun = options.dry_run ?? false;
  const overwrite = options.overwrite ?? false;
  const now = clock.now();
  let accountsProcessed = 0;
  let transactionsExamined = 0;
  let transactionsMatched = 0;
  let annotationsWritten = 0;
  const changes: AppliedTransactionRuleChange[] = [];

  const accounts = await storage.listAccounts();
  for (const account of accounts) {
    accountsProcessed += 1;
    const transactions = await storage.getTransactions(account.id);
    const patches = await storage.getTransactionAnnotationPatches(account.id);
    const annByTx = new Map<string, TransactionAnnotationType>();
    for (const patch of patches) {
      const key = patch.transaction_id.asStr();
      const base = annByTx.get(key) ?? { transaction_id: patch.transaction_id };
      annByTx.set(key, applyTransactionAnnotationPatch(base, patch));
    }

    const patchesToAppend: TransactionAnnotationPatchType[] = [];
    for (const tx of transactions) {
      transactionsExamined += 1;
      const action = matcher.matchAnnotation({
        account_id: account.id.asStr(),
        account_name: account.name,
        description: tx.description,
        status: tx.status,
        amount: tx.amount,
      });
      if (action === undefined) continue;
      transactionsMatched += 1;

      const current = annByTx.get(tx.id.asStr()) ?? { transaction_id: tx.id };
      let categoryToSet: string | undefined;
      let descriptionToSet: string | undefined;
      if (
        action.category !== undefined &&
        (overwrite || current.category === undefined) &&
        current.category !== action.category
      ) {
        categoryToSet = action.category;
      }
      if (
        action.description_override !== undefined &&
        (overwrite || current.description === undefined) &&
        current.description !== action.description_override
      ) {
        descriptionToSet = action.description_override;
      }
      if (categoryToSet === undefined && descriptionToSet === undefined) continue;

      const patch: TransactionAnnotationPatchType = {
        transaction_id: tx.id,
        timestamp: now,
        ...(categoryToSet !== undefined ? { category: categoryToSet } : {}),
        ...(descriptionToSet !== undefined ? { description: descriptionToSet } : {}),
      };

      annByTx.set(tx.id.asStr(), applyTransactionAnnotationPatch(current, patch));
      patchesToAppend.push(patch);
      changes.push({
        rule_index: action.rule_index,
        account_id: account.id.asStr(),
        account_name: account.name,
        transaction_id: tx.id.asStr(),
        timestamp: formatRfc3339(tx.timestamp),
        amount: tx.amount,
        original_description: tx.description,
        ...(categoryToSet !== undefined ? { category: categoryToSet } : {}),
        ...(descriptionToSet !== undefined ? { description: descriptionToSet } : {}),
      });
    }

    if (!dryRun && patchesToAppend.length > 0) {
      await storage.appendTransactionAnnotationPatches(account.id, patchesToAppend);
      annotationsWritten += patchesToAppend.length;
    }
  }

  return {
    success: true,
    rules_path: rulesPath,
    dry_run: dryRun,
    overwrite,
    rules_loaded: matcher.ruleCount(),
    skipped_invalid_rule_count: loaded.skipped_invalid_rule_count,
    accounts_processed: accountsProcessed,
    transactions_examined: transactionsExamined,
    transactions_matched: transactionsMatched,
    annotations_written: annotationsWritten,
    changes,
  };
}

function compileRule(
  ruleIndex: number,
  rule: TransactionAnnotationRule,
): CompiledTransactionAnnotationRule {
  const category = normalizedNonempty(rule.category);
  const descriptionOverride = normalizedNonempty(rule.description_override);
  if (category === undefined && descriptionOverride === undefined) {
    throw new Error(`transaction rule ${ruleIndex} must set category or description_override`);
  }

  const compiled: CompiledTransactionAnnotationRule = {
    rule_index: ruleIndex,
    ...(category !== undefined ? { category } : {}),
    ...(descriptionOverride !== undefined ? { description_override: descriptionOverride } : {}),
    account_id: compileField(ruleIndex, 'account_id', rule.account_id),
    account_name: compileField(ruleIndex, 'account_name', rule.account_name),
    description: compileField(ruleIndex, 'description', rule.description),
    status: compileField(ruleIndex, 'status', rule.status),
    amount: compileField(ruleIndex, 'amount', rule.amount),
  };

  const hasMatcher =
    compiled.account_id !== undefined ||
    compiled.account_name !== undefined ||
    compiled.description !== undefined ||
    compiled.status !== undefined ||
    compiled.amount !== undefined;
  if (!hasMatcher) {
    throw new Error(`transaction rule ${ruleIndex} must specify at least one matcher`);
  }
  return compiled;
}

function compileField(
  ruleIndex: number,
  field: keyof Pick<
    CompiledTransactionAnnotationRule,
    'account_id' | 'account_name' | 'description' | 'status' | 'amount'
  >,
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
          `Invalid transaction rule ${ruleIndex}.${String(field)} regex: unsupported inline flag '${flag}'`,
        );
      }
      flags.add(flag);
    }
    source = source.slice(match[0].length);
  }

  return new RegExp(source, [...flags].sort().join(''));
}

function ruleMatches(
  rule: CompiledTransactionAnnotationRule,
  input: TransactionAnnotationRuleInput,
): boolean {
  return (
    matchField(rule.account_id, input.account_id) &&
    matchField(rule.account_name, input.account_name) &&
    matchField(rule.description, input.description) &&
    matchField(rule.status, input.status) &&
    matchField(rule.amount, input.amount)
  );
}

function matchField(pattern: RegExp | undefined, value: string): boolean {
  return pattern ? pattern.test(value) : true;
}

function escapeRegexLiteral(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function normalizedNonempty(value: string | undefined): string | undefined {
  const trimmed = value?.trim();
  return trimmed === undefined || trimmed === '' ? undefined : trimmed;
}
