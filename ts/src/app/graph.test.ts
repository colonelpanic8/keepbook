import { describe, expect, it } from 'vitest';

import { renderNetWorthSvg, type ResolvedGraphOptions } from './graph.js';
import type { HistoryOutput } from './types.js';

describe('renderNetWorthSvg', () => {
  it('renders an SVG path for history points', () => {
    const history: HistoryOutput = {
      currency: 'USD',
      start_date: '2026-03-01',
      end_date: '2026-03-03',
      granularity: 'daily',
      points: [
        {
          timestamp: '2026-03-01T00:00:00+00:00',
          date: '2026-03-01',
          total_value: '100',
          percentage_change_from_previous: null,
        },
        {
          timestamp: '2026-03-03T00:00:00+00:00',
          date: '2026-03-03',
          total_value: '125',
          percentage_change_from_previous: '25.00',
        },
      ],
    };
    const options: ResolvedGraphOptions = {
      start: '2026-03-01',
      end: '2026-03-03',
      granularity: 'daily',
      includePrices: true,
      output: 'graph.html',
      svgOutput: 'graph.svg',
      title: 'Test Graph',
      width: 800,
      height: 500,
    };

    const svg = renderNetWorthSvg(history, options);

    expect(svg).toContain('<path d="M ');
    expect(svg).toContain('Test Graph');
    expect(svg).toContain('125 USD');
  });
});
