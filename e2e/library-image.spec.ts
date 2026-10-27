import { test, expect, type Page } from '@playwright/test';

// Mock-only suite: the Vite dev server hosts the UI, while every IPC command is
// replaced before the React app boots. No Facebook/Graph command may run.
test.skip(process.env.E2E_MOCK !== 'true', 'Library image E2E runs in mock mode only');

const mockCode = `
window.__TAURI_MOCK__ = true;
window.__CALLS__ = [];
window.__GRAPH_CALLS__ = [];
window.__FAIL_IMAGE__ = false;
window.__IMAGE_DEFER__ = false;
window.__RESOLVE_IMAGE__ = null;
window.__CONTENTS__ = [
  { id: 'c1', title: 'Bài chưa có ảnh', body: 'Nội dung A', prompt: 'prompt A', style: 'cinematic', topic: null, customStyle: null, mediaIds: [], status: 'draft', createdAt: '2026-10-27T07:00:00Z' },
  { id: 'c2', title: 'Bài đã có ảnh', body: 'Nội dung B', prompt: 'prompt B', style: 'cinematic', topic: null, customStyle: null, mediaIds: ['old-1'], status: 'draft', createdAt: '2026-10-27T06:00:00Z' },
];
window.__MEDIA__ = [
  { id: 'old-1', path: 'C:\\\\tmp\\\\old.png', name: 'old.png', mediaType: 'image', extension: 'png', size: 100 },
  { id: 'img-1', path: 'C:\\\\tmp\\\\new.png', name: 'new.png', mediaType: 'image', extension: 'png', size: 200 },
];
window.__TAURI_INTERNALS__ = {
  invoke: (cmd, args) => window.__TAURI_MOCK_IMPL__(cmd, args),
  convertFileSrc: (path) => 'asset://localhost/' + encodeURIComponent(path),
};
window.__TAURI_MOCK_IMPL__ = async (cmd, args) => {
  switch (cmd) {
    case 'list_styles':
      return [{ id: 'cinematic', name: 'Điện ảnh', content: 'Ánh sáng điện ảnh, không có chữ.', builtin: true }];
    case 'list_content':
      return window.__CONTENTS__.map((c) => ({ ...c, mediaIds: [...c.mediaIds] }));
    case 'list_schedule':
      return [];
    case 'credential_status':
      return { hasFacebookToken: false, hasAiProviderKey: true };
    case 'get_active_provider':
      return { id: 'p1', displayName: 'Provider ảnh', baseUrl: 'http://127.0.0.1:20128/v1', protocol: 'openai-completions', models: ['img-model'], hasApiKey: true };
    case 'list_facebook_pages':
      window.__GRAPH_CALLS__.push(cmd);
      return [];
    case 'list_media':
      return window.__MEDIA__.map((m) => ({ ...m }));
    case 'list_image_styles':
      return [{ id: 'cinematic', name: 'Điện ảnh', content: 'Ánh sáng điện ảnh, không có chữ.', builtin: true }];
    case 'get_image_config':
      return { providerId: 'p1', model: 'img-model', saveDir: '', styleId: 'cinematic', size: '1024x1024' };
    case 'list_providers':
      return [{ id: 'p1', displayName: 'Provider ảnh', baseUrl: 'http://127.0.0.1:20128/v1', protocol: 'openai-completions', models: ['img-model'], hasApiKey: true }];
    case 'generate_image': {
      window.__CALLS__.push({ cmd, args });
      if (window.__FAIL_IMAGE__) throw new Error('AI tạo ảnh đang gặp sự cố, vui lòng thử lại.');
      const result = { mediaId: 'img-1', path: 'C:\\\\tmp\\\\new.png', name: 'new.png', extension: 'png', size: 1234, model: 'img-2', styleId: 'cinematic' };
      if (window.__IMAGE_DEFER__) {
        return new Promise((resolve) => { window.__RESOLVE_IMAGE__ = () => resolve(result); });
      }
      return result;
    }
    case 'update_content': {
      window.__CALLS__.push({ cmd, args });
      const p = args?.payload || {};
      const item = window.__CONTENTS__.find((c) => c.id === p.contentId);
      if (!item) throw new Error('Không tìm thấy ' + p.contentId);
      Object.assign(item, {
        title: p.title, body: p.body, prompt: p.prompt, style: p.style,
        customStyle: p.customStyle ?? null, topic: p.topic ?? null,
        mediaIds: [...(p.mediaIds || [])],
      });
      return { ...item, mediaIds: [...item.mediaIds] };
    }
    default:
      throw new Error('Unknown mock command: ' + cmd);
  }
};
`;

const contentRow = (page: Page, title: string) => page.locator('article', { hasText: title });
const ipcCalls = (page: Page): Promise<any[]> => page.evaluate(() => (window as any).__CALLS__);
const graphCalls = (page: Page): Promise<string[]> => page.evaluate(() => (window as any).__GRAPH_CALLS__);

async function lastPayload(page: Page, cmd: string): Promise<any> {
  const calls = (await ipcCalls(page)).filter((call) => call.cmd === cmd);
  return calls.length ? calls[calls.length - 1].args?.payload : null;
}

async function openWarehouse(page: Page) {
  await page.locator('nav button', { hasText: 'Kho nội dung' }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'Kho nội dung' })).toBeVisible();
  await expect(contentRow(page, 'Bài chưa có ảnh')).toBeVisible();
  await expect(contentRow(page, 'Bài đã có ảnh')).toBeVisible();
}

test.describe('Content warehouse AI image mock E2E', () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(mockCode);
    await page.goto('/');
  });

  test('no-media row generates an image and attaches exactly that image', async ({ page }) => {
    await openWarehouse(page);
    const rowA = contentRow(page, 'Bài chưa có ảnh');
    const rowB = contentRow(page, 'Bài đã có ảnh');
    await expect(rowA).toContainText('0 media');

    await page.evaluate(() => { (window as any).__IMAGE_DEFER__ = true; });
    await rowA.getByRole('button', { name: 'Tạo ảnh AI' }).click();

    // AC6: only the active row shows the in-flight label; every image button is disabled.
    await expect(rowA.getByRole('button', { name: 'Đang tạo ảnh...' })).toBeVisible();
    await expect(rowA.getByRole('button', { name: 'Đang tạo ảnh...' })).toBeDisabled();
    await expect(rowB.getByRole('button', { name: 'Đổi ảnh AI' })).toBeDisabled();
    await expect(rowB.getByRole('button', { name: 'Đổi ảnh AI' })).not.toHaveText('Đang tạo ảnh...');

    const generatePayload = await lastPayload(page, 'generate_image');
    expect(generatePayload).toEqual({
      title: 'Bài chưa có ảnh',
      body: 'Nội dung A',
      styleId: 'cinematic',
      providerId: 'p1',
      model: 'img-model',
      size: '1024x1024',
    });

    await page.evaluate(() => { (window as any).__RESOLVE_IMAGE__(); });

    await expect(page.getByText('Đã tạo ảnh AI và cập nhật bài viết')).toBeVisible();
    await expect(rowA).toContainText('1 media');

    const updatePayload = await lastPayload(page, 'update_content');
    expect(updatePayload).toEqual({
      contentId: 'c1',
      title: 'Bài chưa có ảnh',
      body: 'Nội dung A',
      prompt: 'prompt A',
      style: 'cinematic',
      customStyle: null,
      topic: null,
      mediaIds: ['img-1'],
    });
    expect((await ipcCalls(page)).filter((call) => call.cmd === 'save_content')).toHaveLength(0);
    expect(await graphCalls(page)).toHaveLength(0);
  });

  test('media row opens the confirm dialog before generating and closes without generating', async ({ page }) => {
    await openWarehouse(page);
    const rowB = contentRow(page, 'Bài đã có ảnh');
    const button = rowB.getByRole('button', { name: 'Đổi ảnh AI' });
    await expect(button).toBeVisible();

    await button.click();
    const dialog = page.getByRole('dialog', { name: 'Đổi ảnh AI' });
    await expect(dialog).toBeVisible();
    await expect(dialog).toContainText('Nội dung đã có 1 ảnh');
    await expect(dialog.getByRole('button', { name: 'Hủy' })).toBeVisible();
    await expect(dialog.getByRole('button', { name: 'Thêm ảnh' })).toBeVisible();
    await expect(dialog.getByRole('button', { name: 'Thay ảnh' })).toBeVisible();
    expect(await ipcCalls(page)).toHaveLength(0);

    // Escape closes without generating.
    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden();

    // Overlay click closes without generating.
    await button.click();
    await expect(dialog).toBeVisible();
    await page.locator('.modal-overlay').click({ position: { x: 5, y: 5 } });
    await expect(dialog).toBeHidden();

    // Hủy closes without generating.
    await button.click();
    await expect(dialog).toBeVisible();
    await dialog.getByRole('button', { name: 'Hủy' }).click();
    await expect(dialog).toBeHidden();

    expect(await ipcCalls(page)).toHaveLength(0);
    expect(await graphCalls(page)).toHaveLength(0);
  });

  test('Thêm ảnh appends the new image after the existing one', async ({ page }) => {
    await openWarehouse(page);
    const rowB = contentRow(page, 'Bài đã có ảnh');
    await expect(rowB).toContainText('1 media');

    await rowB.getByRole('button', { name: 'Đổi ảnh AI' }).click();
    const dialog = page.getByRole('dialog', { name: 'Đổi ảnh AI' });
    await expect(dialog).toBeVisible();
    expect(await lastPayload(page, 'generate_image')).toBeNull();
    await dialog.getByRole('button', { name: 'Thêm ảnh' }).click();

    await expect(page.getByText('Đã thêm ảnh AI vào bài viết')).toBeVisible();
    await expect(rowB).toContainText('2 media');

    const updatePayload = await lastPayload(page, 'update_content');
    expect(updatePayload.mediaIds).toEqual(['old-1', 'img-1']);
    expect(updatePayload.style).toBe('cinematic');
    expect(updatePayload.prompt).toBe('prompt B');
    expect(await graphCalls(page)).toHaveLength(0);
  });

  test('Thay ảnh replaces all existing images', async ({ page }) => {
    await openWarehouse(page);
    const rowB = contentRow(page, 'Bài đã có ảnh');
    await expect(rowB).toContainText('1 media');

    await rowB.getByRole('button', { name: 'Đổi ảnh AI' }).click();
    const dialog = page.getByRole('dialog', { name: 'Đổi ảnh AI' });
    await expect(dialog).toBeVisible();
    await dialog.getByRole('button', { name: 'Thay ảnh' }).click();

    await expect(page.getByText('Đã tạo ảnh AI và cập nhật bài viết')).toBeVisible();
    await expect(rowB).toContainText('1 media');

    const updatePayload = await lastPayload(page, 'update_content');
    expect(updatePayload.mediaIds).toEqual(['img-1']);
    expect(updatePayload.title).toBe('Bài đã có ảnh');
    expect(updatePayload.body).toBe('Nội dung B');
    expect(updatePayload.style).toBe('cinematic');
    expect(updatePayload.topic).toBeNull();
    expect(updatePayload.customStyle).toBeNull();
    expect((await ipcCalls(page)).filter((call) => call.cmd === 'save_content')).toHaveLength(0);
    expect(await graphCalls(page)).toHaveLength(0);
  });

  test('failed image generation keeps the post unchanged and re-enables the button', async ({ page }) => {
    await openWarehouse(page);
    await page.evaluate(() => { (window as any).__FAIL_IMAGE__ = true; });

    const rowA = contentRow(page, 'Bài chưa có ảnh');
    await rowA.getByRole('button', { name: 'Tạo ảnh AI' }).click();

    await expect(page.locator('.error-box')).toBeVisible();
    await expect(page.locator('.error-box')).toContainText('AI tạo ảnh đang gặp sự cố');

    expect((await ipcCalls(page)).filter((call) => call.cmd === 'update_content')).toHaveLength(0);
    await expect(rowA).toContainText('0 media');
    const button = rowA.getByRole('button', { name: 'Tạo ảnh AI' });
    await expect(button).toBeVisible();
    await expect(button).toBeEnabled();
    expect(await graphCalls(page)).toHaveLength(0);
  });
});
