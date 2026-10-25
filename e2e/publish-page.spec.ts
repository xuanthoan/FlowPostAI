import { test, expect, type Page } from '@playwright/test';

// Mock-only suite: the Vite dev server is used as the static host, while all
// IPC commands are replaced before the React app boots.
test.skip(process.env.E2E_MOCK !== 'true', 'Publish page E2E runs in mock mode only');

const mockCode = `
window.__TAURI_MOCK__ = true;
window.__IPC_CALLS__ = [];
window.__PAGE_LIST_CALLS__ = [];
window.__PUBLISH_ARGS__ = [];
window.__TAURI_INTERNALS__ = {
  invoke: (cmd, args) => window.__TAURI_MOCK_IMPL__(cmd, args),
  convertFileSrc: (path) => 'asset://localhost/' + encodeURIComponent(path),
};
window.__TAURI_MOCK_IMPL__ = async (cmd, args) => {
  window.__IPC_CALLS__.push(cmd);
  if (cmd === 'credential_status') return { hasFacebookToken: true, hasAiProviderKey: true };
  if (cmd === 'get_active_provider') {
    return { id: 'p1', displayName: 'Mock provider', baseUrl: 'http://127.0.0.1:20128/v1', protocol: 'openai-completions', models: ['text-model'], defaultModel: 'text-model', hasApiKey: true };
  }
  if (cmd === 'list_providers') return [];
  if (cmd === 'get_image_config') return { providerId: '', model: '', saveDir: '', styleId: '', size: '1024x1024' };
  if (cmd === 'list_styles') return [];
  if (cmd === 'list_schedule') return [];
  if (cmd === 'get_scheduler_config') return { selectedPageId: 'page-2' };
  if (cmd === 'list_content') {
    return [{ id: 'c1', title: 'Bài mẫu', body: 'Nội dung từ kho', mediaIds: [], status: 'approved' }];
  }
  if (cmd === 'list_media') {
    return Array.from({ length: 11 }, (_, i) => ({
      id: 'm' + (i + 1),
      name: 'i' + (i + 1) + '.jpg',
      path: 'C:\\\\m\\\\i.jpg',
      mediaType: 'image',
      extension: 'jpg',
      size: 1024,
    }));
  }
  if (cmd === 'list_facebook_pages') {
    window.__PAGE_LIST_CALLS__.push(args || null);
    return [
      { id: 'page-1', name: 'Sống Tích Cực' },
      { id: 'page-2', name: 'Daily Motivation' },
    ];
  }
  if (cmd === 'publish_content') {
    window.__PUBLISH_ARGS__.push(args || null);
    return { id: 'post-1' };
  }
  throw new Error('Unknown mock command: ' + cmd);
};
`;

const publishForm = (page: Page) => page.locator('.publish-page .publish-form');
const pageListCalls = (page: Page): Promise<number[]> => page.evaluate(() => (window as any).__PAGE_LIST_CALLS__);
const ipcCalls = (page: Page): Promise<string[]> => page.evaluate(() => (window as any).__IPC_CALLS__);

async function openPublishPage(page: Page) {
  await page.getByRole('button', { name: 'Đăng bài', exact: true }).click();
  await expect(page.locator('section.publish-page')).toBeVisible();
  await expect(page.getByRole('heading', { level: 1, name: 'Đăng bài' })).toBeVisible();
}

async function loadPages(page: Page) {
  await publishForm(page).getByRole('button', { name: 'Tải danh sách Trang', exact: true }).click();
  await expect.poll(async () => (await pageListCalls(page)).length).toBe(1);
}

test.describe('Publish page mock E2E', () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(mockCode);
    await page.goto('/');
  });

  test('renders a dedicated Đăng bài page without calling Graph', async ({ page }) => {
    await openPublishPage(page);

    await expect(page.locator('section.publish-page')).toBeVisible();
    await expect(page.locator('.welcome')).toHaveCount(0);
    await expect(page.locator('.stats-grid')).toHaveCount(0);
    expect(await pageListCalls(page)).toHaveLength(0);
    await expect(publishForm(page).getByRole('button', { name: 'Đăng ngay', exact: true })).toBeDisabled();
  });

  test('keeps the Tổng quan dashboard intact across navigation', async ({ page }) => {
    await expect(page.locator('.welcome')).toBeVisible();
    await expect(page.locator('.stats-grid')).toBeVisible();

    await openPublishPage(page);
    await page.getByRole('button', { name: 'Tổng quan', exact: true }).click();
    await expect(page.locator('.welcome')).toBeVisible();
    await expect(page.locator('.stats-grid')).toBeVisible();
    await expect(page.locator('section.publish-page')).toHaveCount(0);

    await openPublishPage(page);
    await page.getByRole('button', { name: 'Tổng quan', exact: true }).click();
    await expect(page.locator('.welcome')).toBeVisible();
    await expect(page.locator('.stats-grid')).toBeVisible();
  });

  test('loads the page list only on the explicit button', async ({ page }) => {
    await openPublishPage(page);
    const form = publishForm(page);
    const select = form.getByLabel('Chọn Trang');

    await expect(select.locator('option')).toHaveCount(1);
    await expect(form.getByText('Bấm "Tải danh sách Trang" để tải danh sách Trang từ Facebook.')).toBeVisible();
    expect(await pageListCalls(page)).toHaveLength(0);

    await loadPages(page);
    await expect(select.locator('option')).toHaveCount(3);
    await select.selectOption('page-1');
    await expect(select).toHaveValue('page-1');
    expect(await pageListCalls(page)).toHaveLength(1);
  });

  test('loads the content library on demand and caches it', async ({ page }) => {
    await openPublishPage(page);
    const form = publishForm(page);
    const contentButton = form.getByRole('button', { name: 'Chọn từ Kho nội dung', exact: true });
    const contentCalls = async () => (await ipcCalls(page)).filter(cmd => cmd === 'list_content').length;

    // The dashboard's local reload (list_content) settles before we measure the delta.
    await expect.poll(contentCalls).toBeGreaterThan(0);
    const before = await contentCalls();

    await contentButton.click();
    await expect.poll(contentCalls).toBe(before + 1);
    await expect(form.getByText('Bài mẫu')).toBeVisible();
    await expect(form.getByText('Đã duyệt')).toBeVisible();

    await form.getByRole('button', { name: /Bài mẫu/ }).click();
    await expect(form.getByLabel('Nội dung bài đăng')).toHaveValue('Nội dung từ kho');
    await expect(form.getByText('Bài mẫu')).toHaveCount(0);

    await contentButton.click();
    await expect(form.getByText('Bài mẫu')).toBeVisible();
    await contentButton.click();
    await expect(form.getByText('Bài mẫu')).toHaveCount(0);
    await contentButton.click();
    await expect(form.getByText('Bài mẫu')).toBeVisible();
    expect(await contentCalls()).toBe(before + 1);
  });

  test('submit stays disabled until a page and a message are set', async ({ page }) => {
    await openPublishPage(page);
    const form = publishForm(page);
    const submit = form.getByRole('button', { name: 'Đăng ngay', exact: true });

    await expect(submit).toBeDisabled();

    await loadPages(page);
    await form.getByLabel('Chọn Trang').selectOption('page-2');
    await form.getByLabel('Nội dung bài đăng').fill('   ');
    await expect(submit).toBeDisabled();

    await form.getByLabel('Nội dung bài đăng').fill('Xin chào');
    await expect(submit).toBeEnabled();
  });

  test('publishes with the expected payload and stays on the page', async ({ page }) => {
    await openPublishPage(page);
    const form = publishForm(page);

    await loadPages(page);
    await form.getByLabel('Chọn Trang').selectOption('page-2');
    await form.getByLabel('Nội dung bài đăng').fill('Xin chào');
    await form.getByRole('button', { name: 'Đăng ngay', exact: true }).click();

    await expect(form.getByText(/Đã đăng thành công \(post ID: post-1\)/)).toBeVisible();
    const args: any[] = await page.evaluate(() => (window as any).__PUBLISH_ARGS__);
    expect(args[0]).toEqual({ page_id: 'page-2', message: 'Xin chào', image_path: null, image_paths: [] });

    await page.waitForTimeout(1600);
    await expect(page.getByRole('heading', { level: 1, name: 'Đăng bài' })).toBeVisible();
    await expect(page.locator('.publish-modal')).toHaveCount(0);
    await expect(form.getByLabel('Chọn Trang')).toHaveValue('page-2');
  });

  test('caps the selection at 10 images', async ({ page }) => {
    await openPublishPage(page);
    const form = publishForm(page);

    await form.getByRole('button', { name: 'Chọn ảnh', exact: true }).click();
    for (let i = 1; i <= 11; i += 1) {
      await form.getByText(`i${i}.jpg`, { exact: true }).click();
    }

    await expect(form.getByText('Chỉ chọn tối đa 10 ảnh mỗi bài')).toBeVisible();
    await expect(form.getByText('Đã chọn 10 ảnh')).toBeVisible();
  });

  test('Settings modal reuses the form without another page-list call', async ({ page }) => {
    await page.getByRole('button', { name: 'Cài đặt', exact: true }).click();
    await expect(page.getByRole('heading', { level: 1, name: 'Cài đặt' })).toBeVisible();

    await page.locator('input[type="password"]').fill('mock_token');
    const before = (await pageListCalls(page)).length;
    await page.getByRole('button', { name: 'Tải Trang', exact: true }).click();
    await expect(page.getByText(/Đã tải 2 Trang/)).toBeVisible();
    await expect.poll(async () => (await pageListCalls(page)).length).toBe(before + 1);

    const afterList = (await pageListCalls(page)).length;
    await page.getByRole('button', { name: 'Đăng ngay', exact: true }).click();
    const modal = page.locator('.publish-modal');
    await expect(modal).toBeVisible();
    await expect(modal.getByRole('heading', { name: 'Đăng bài ngay' })).toBeVisible();
    expect((await pageListCalls(page)).length).toBe(afterList);

    await modal.getByLabel('Chọn Trang').selectOption('page-1');
    await modal.getByLabel('Nội dung bài đăng').fill('Đăng từ Cài đặt');
    await modal.getByRole('button', { name: 'Đăng ngay', exact: true }).click();
    await expect(modal.getByText(/Đã đăng thành công/)).toBeVisible();
    await expect(modal).toHaveCount(0, { timeout: 4000 });
  });
});
