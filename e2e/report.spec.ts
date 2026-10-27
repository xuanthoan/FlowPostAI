import { test, expect, type Page } from '@playwright/test';

// Mock-only suite: the Tauri dev server is used as the static host, while all
// IPC commands are replaced before the React app boots.
test.skip(process.env.E2E_MOCK !== 'true', 'Report E2E runs in mock mode only');

const mockCode = `
window.__TAURI_MOCK__ = true;
window.__GRAPH_CALLS__ = [];
window.__REPORT_ARGS__ = [];
window.__SAVED_CSV__ = null;
window.__PRINT_CALLED__ = false;
window.print = () => { window.__PRINT_CALLED__ = true; };
window.__TAURI_INTERNALS__ = {
  invoke: (cmd, args) => window.__TAURI_MOCK_IMPL__(cmd, args),
  convertFileSrc: (path) => 'asset://localhost/' + encodeURIComponent(path),
};
window.__TAURI_MOCK_IMPL__ = async (cmd, args) => {
  if (cmd === 'credential_status') return { hasFacebookToken: true, hasAiProviderKey: true };
  if (cmd === 'get_active_provider') {
    return { id: 'p1', displayName: 'Mock provider', baseUrl: 'http://127.0.0.1:20128/v1', protocol: 'openai-completions', models: ['text-model'], defaultModel: 'text-model', hasApiKey: true };
  }
  if (cmd === 'list_styles') return [];
  if (cmd === 'list_content') return [];
  if (cmd === 'list_schedule') return [];
  if (cmd === 'get_scheduler_config') {
    return { selectedPageId: 'page-2', intervalSeconds: 30, enabled: true };
  }
  if (cmd === 'list_facebook_pages') {
    window.__GRAPH_CALLS__.push(cmd);
    return [
      { id: 'page-1', name: 'Sống Tích Cực' },
      { id: 'page-2', name: 'Daily Motivation' },
    ];
  }
  if (cmd === 'get_page_report') {
    window.__GRAPH_CALLS__.push(cmd);
    window.__REPORT_ARGS__.push(args || null);
    const pageId = args?.pageId || '';
    const days = Number(args?.days) || 7;
    return {
      pageId,
      pageName: args?.pageName || (pageId === 'page-2' ? 'Daily Motivation' : 'Sống Tích Cực'),
      days,
      since: 1790000000,
      until: 1790000000 + days * 86400,
      postCount: 2,
      totalReactions: 40,
      totalComments: 20,
      totalShares: 10,
      totalInteractions: 70,
      partial: false,
      truncated: false,
      posts: [
        {
          id: pageId + '_1',
          message: 'Xin chào, "báo cáo"\\nDòng 2',
          createdTime: '2026-10-20T08:00:00+0000',
          permalinkUrl: 'https://example.test/p/1',
          reactions: 30,
          comments: 15,
          shares: 5,
        },
        {
          id: pageId + '_2',
          message: 'Bài viết thường',
          createdTime: '2026-10-19T08:00:00+0000',
          permalinkUrl: 'https://example.test/p/2',
          reactions: 10,
          comments: 5,
          shares: 5,
        },
      ],
    };
  }
  if (cmd === 'get_page_report_range') {
    window.__GRAPH_CALLS__.push(cmd);
    window.__REPORT_ARGS__.push(args || null);
    const pageId = args?.pageId || '';
    const rangeKey = args?.rangeKey || 'month';
    const from = args?.fromDate || '';
    const to = args?.toDate || '';
    const fmt = value => {
      const p = String(value || '').split('-');
      return p.length === 3 ? p[2] + '/' + p[1] + '/' + p[0] : '';
    };
    const span = (a, b) => {
      const pa = String(a || '').split('-').map(Number);
      const pb = String(b || '').split('-').map(Number);
      if (pa.length !== 3 || pb.length !== 3) return 0;
      return Math.round((Date.UTC(pb[0], pb[1] - 1, pb[2]) - Date.UTC(pa[0], pa[1] - 1, pa[2])) / 86400000) + 1;
    };
    const days = rangeKey === 'month' ? 25 : span(from, to);
    const label = rangeKey === 'month' ? 'Tháng này' : 'Tùy chọn: ' + fmt(from) + ' – ' + fmt(to);
    return {
      pageId,
      pageName: args?.pageName || (pageId === 'page-2' ? 'Daily Motivation' : 'Sống Tích Cực'),
      days,
      since: 1790000000,
      until: 1790000000 + days * 86400,
      postCount: 2,
      totalReactions: 40,
      totalComments: 20,
      totalShares: 10,
      totalInteractions: 70,
      partial: false,
      truncated: false,
      rangeKey,
      rangeLabel: label,
      fromDate: rangeKey === 'custom' ? from : null,
      toDate: rangeKey === 'custom' ? to : null,
      posts: [
        {
          id: pageId + '_1',
          message: 'Xin chào, "báo cáo"\\nDòng 2',
          createdTime: '2026-10-20T08:00:00+0000',
          permalinkUrl: 'https://example.test/p/1',
          reactions: 30,
          comments: 15,
          shares: 5,
        },
        {
          id: pageId + '_2',
          message: 'Bài viết thường',
          createdTime: '2026-10-19T08:00:00+0000',
          permalinkUrl: 'https://example.test/p/2',
          reactions: 10,
          comments: 5,
          shares: 5,
        },
      ],
    };
  }
  if (cmd === 'save_report_csv') {
    window.__SAVED_CSV__ = { defaultName: args?.defaultName, contents: args?.contents };
    return 'C:/tmp/bao-cao.csv';
  }
  throw new Error('Unknown mock command: ' + cmd);
};
`;

async function openReport(page: Page) {
  await page.getByRole('button', { name: 'Báo cáo', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Báo cáo hiệu suất' })).toBeVisible();
}

async function graphCalls(page: Page): Promise<string[]> {
  return page.evaluate(() => (window as any).__GRAPH_CALLS__);
}

async function loadReport(page: Page, expectedCalls = 2) {
  await page.getByRole('button', { name: 'Tải báo cáo', exact: true }).click();
  await expect.poll(async () => (await graphCalls(page)).length).toBe(expectedCalls);
}

test.describe('Facebook report mock E2E', () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(mockCode);
    await page.goto('/');
  });

  test('never calls Graph on mount, navigation or filter change', async ({ page }) => {
    await openReport(page);

    expect(await graphCalls(page)).toEqual([]);
    await expect(page.getByRole('button', { name: 'Xuất CSV', exact: true })).toBeDisabled();
    await expect(page.getByRole('button', { name: 'Xuất PDF', exact: true })).toBeDisabled();
    await expect(page.getByLabel('Chọn Trang')).toBeDisabled();
    await expect(page.getByLabel('Chọn Trang')).toContainText('Chưa tải danh sách Trang');

    await page.getByLabel('Khoảng thời gian').selectOption('30');
    await expect(page.getByLabel('Khoảng thời gian')).toHaveValue('30');
    expect(await graphCalls(page)).toEqual([]);

    await page.getByRole('button', { name: 'Tổng quan', exact: true }).click();
    await page.getByRole('button', { name: 'Báo cáo', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Báo cáo hiệu suất' })).toBeVisible();
    expect(await graphCalls(page)).toEqual([]);
  });

  test('loads the configured page only on click and honors 7/30 days', async ({ page }) => {
    await openReport(page);
    await loadReport(page);

    expect(await graphCalls(page)).toEqual(['list_facebook_pages', 'get_page_report']);
    const firstArgs: any[] = await page.evaluate(() => (window as any).__REPORT_ARGS__);
    expect(firstArgs[0]).toMatchObject({ pageId: 'page-2', pageName: 'Daily Motivation', days: 7 });
    await expect(page.getByLabel('Chọn Trang')).toHaveValue('page-2');

    const postCard = page.locator('.stat-card').filter({ hasText: 'Tổng số bài' });
    const interactionCard = page.locator('.stat-card').filter({ hasText: 'Tổng tương tác' });
    await expect(postCard.locator('.stat-row strong')).toHaveText('2');
    await expect(interactionCard.locator('.stat-row strong')).toHaveText('70');

    await page.getByLabel('Khoảng thời gian').selectOption('30');
    await expect(page.getByText('Bộ lọc đã thay đổi — bấm Tải báo cáo để cập nhật')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Xuất CSV', exact: true })).toBeDisabled();
    expect(await graphCalls(page)).toHaveLength(2);

    await loadReport(page, 3);
    expect(await graphCalls(page)).toEqual([
      'list_facebook_pages',
      'get_page_report',
      'get_page_report',
    ]);
    const reportArgs: any[] = await page.evaluate(() => (window as any).__REPORT_ARGS__);
    expect(reportArgs[1]).toMatchObject({ pageId: 'page-2', days: 30 });
    await expect(page.getByRole('button', { name: 'Xuất CSV', exact: true })).toBeEnabled();
  });

  test('exports the loaded report to CSV without Graph', async ({ page }) => {
    await openReport(page);
    await loadReport(page);
    const before = await graphCalls(page);

    await page.getByRole('button', { name: 'Xuất CSV', exact: true }).click();
    await expect.poll(() => page.evaluate(() => (window as any).__SAVED_CSV__?.defaultName || '')).toBeTruthy();

    const saved: any = await page.evaluate(() => (window as any).__SAVED_CSV__);
    expect(saved.defaultName).toMatch(/^bao-cao-daily-motivation-7ngay-\d{4}-\d{2}-\d{2}\.csv$/);
    expect(saved.contents.charCodeAt(0)).toBe(0xfeff);
    expect(saved.contents.startsWith('\uFEFF\uFEFF')).toBe(false);
    expect(saved.contents).toContain('Tổng số bài,2');
    expect(saved.contents).toContain('Tổng tương tác,70');
    expect(saved.contents).toContain('"Xin chào, ""báo cáo""');

    const rows = saved.contents.match(/^page-2_\d+,/gm) || [];
    expect(rows).toHaveLength(2);
    expect(await graphCalls(page)).toEqual(before);
    await expect(page.getByText('Đã lưu CSV: C:/tmp/bao-cao.csv')).toBeVisible();
  });

  test('prints the loaded report as PDF without Graph', async ({ page }) => {
    await openReport(page);
    await expect(page.getByRole('button', { name: 'Xuất PDF', exact: true })).toBeDisabled();

    await loadReport(page);
    const before = await graphCalls(page);
    await page.evaluate(() => { (window as any).__PRINT_CALLED__ = false; });

    await page.getByRole('button', { name: 'Xuất PDF', exact: true }).click();
    await expect.poll(() => page.evaluate(() => (window as any).__PRINT_CALLED__)).toBe(true);
    await expect(page.locator('.report-print-hint')).toHaveText('Chọn "Save as PDF" trong hộp thoại in để lưu PDF.');
    expect(await graphCalls(page)).toEqual(before);
  });

  test('month range loads month-to-date only on click and never on filter change', async ({ page }) => {
    await openReport(page);
    expect(await graphCalls(page)).toEqual([]);

    await page.getByLabel('Khoảng thời gian').selectOption('month');
    expect(await graphCalls(page)).toEqual([]);
    await expect(page.getByRole('button', { name: 'Xuất CSV', exact: true })).toBeDisabled();

    await loadReport(page);
    expect(await graphCalls(page)).toEqual(['list_facebook_pages', 'get_page_report_range']);
    const args: any[] = await page.evaluate(() => (window as any).__REPORT_ARGS__);
    expect(args[0]).toMatchObject({ pageId: 'page-2', rangeKey: 'month' });
    await expect(page.locator('.report-period')).toContainText('Tháng này');
    await expect(page.getByRole('button', { name: 'Xuất CSV', exact: true })).toBeEnabled();

    await page.getByLabel('Khoảng thời gian').selectOption('30');
    await expect(page.getByText('Bộ lọc đã thay đổi — bấm Tải báo cáo để cập nhật')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Xuất CSV', exact: true })).toBeDisabled();
    expect(await graphCalls(page)).toHaveLength(2);
  });

  test('custom range validates locally and loads the chosen dates', async ({ page }) => {
    await openReport(page);
    await page.getByLabel('Khoảng thời gian').selectOption('custom');
    expect(await graphCalls(page)).toEqual([]);

    await page.getByLabel('Từ ngày').fill('2026-10-20');
    await page.getByLabel('Đến ngày').fill('2026-10-01');
    await expect(page.getByRole('button', { name: 'Tải báo cáo', exact: true })).toBeDisabled();
    await expect(page.getByRole('button', { name: 'Xuất CSV', exact: true })).toBeDisabled();
    expect(await graphCalls(page)).toEqual([]);

    await page.getByLabel('Từ ngày').fill('2026-10-01');
    await page.getByLabel('Đến ngày').fill('2026-10-20');
    await loadReport(page);
    expect(await graphCalls(page)).toEqual(['list_facebook_pages', 'get_page_report_range']);
    const args: any[] = await page.evaluate(() => (window as any).__REPORT_ARGS__);
    expect(args[0]).toMatchObject({
      pageId: 'page-2',
      rangeKey: 'custom',
      fromDate: '2026-10-01',
      toDate: '2026-10-20',
    });
    await expect(page.locator('.report-period')).toContainText('Tùy chọn: 01/10/2026 – 20/10/2026');

    await page.getByRole('button', { name: 'Xuất CSV', exact: true }).click();
    await expect.poll(() => page.evaluate(() => (window as any).__SAVED_CSV__?.defaultName || '')).toBeTruthy();
    const saved: any = await page.evaluate(() => (window as any).__SAVED_CSV__);
    expect(saved.defaultName).toMatch(/^bao-cao-daily-motivation-tu-2026-10-01-den-2026-10-20-\d{4}-\d{2}-\d{2}\.csv$/);
    expect(saved.contents).toContain('Khoảng thời gian,Tùy chọn: 01/10/2026 – 20/10/2026');
    expect(await graphCalls(page)).toHaveLength(2);
  });
});
