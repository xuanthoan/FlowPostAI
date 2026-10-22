import { test, expect } from '@playwright/test';
import { invoke, isTauri } from './helpers/tauri';
import { ensureProvider } from './helpers/provider';

const BASE_URL = process.env.E2E_BASE_URL || 'http://127.0.0.1:20128/v1';

const mockImplCode = `
window.__TAURI_MOCK__ = true;
window.__TAURI_INTERNALS__ = { invoke: (cmd, args) => window.__TAURI_MOCK_IMPL__(cmd, args), convertFileSrc: (p) => 'asset://localhost/' + encodeURIComponent(p) };
window.__TAURI_MOCK_IMPL__ = async (cmd, args) => {
  const baseUrl = ${JSON.stringify(BASE_URL)};
  if (cmd === 'generate_content') {
    const payload = args?.payload;
    window.__LAST_GENERATE_PAYLOAD__ = payload ?? null;
    if (!payload?.prompt || !payload.prompt.trim()) {
      throw new Error('Vui lòng nhập prompt');
    }
    return {
      title: 'Tiêu đề từ AI',
      body: 'Nội dung được tạo bởi mock',
      isMock: false,
      prompt: payload.prompt,
      style: payload.style,
      styleContent: payload?.styleContent ?? null,
    };
  }
  if (cmd === 'list_styles') {
    return [
      {
        id: 's1',
        name: 'Viral',
        content: 'Viết nội dung ngắn gọn, hấp dẫn cho mạng xã hội.',
        builtin: true,
      },
    ];
  }
  if (cmd === 'list_topics') {
    return [];
  }
  if (cmd === 'update_provider') {
    return {
      id: 'custom-provider',
      displayName: 'Custom provider',
      baseUrl: baseUrl,
      protocol: 'openai-completions',
      models: ['xoay-vong-worker-web-128k'],
      hasApiKey: true,
    };
  }
  if (cmd === 'get_active_provider') {
    return {
      id: 'p1',
      displayName: 'P1',
      baseUrl: 'http://127.0.0.1:20128/v1',
      protocol: 'openai-completions',
      models: ['aihorde/2DN', 'text-model'],
      defaultModel: 'text-model',
      hasApiKey: true,
    };
  }
  if (cmd === 'list_image_styles') {
    return [
      { id: 'cinematic', name: 'Điện ảnh', content: 'Ánh sáng điện ảnh, không có chữ.', builtin: true },
      { id: 'minimal', name: 'Tối giản', content: 'Nền tối giản, không có chữ.', builtin: true },
      { id: 'illustration', name: 'Minh hoạ phẳng', content: 'Minh hoạ phẳng, không có chữ.', builtin: true },
    ];
  }
  if (cmd === 'get_image_config') {
    return { providerId: 'p1', model: 'img-model', saveDir: '', styleId: 'cinematic', size: '1024x1024' };
  }
  if (cmd === 'set_image_config') {
    window.__LAST_IMAGE_CONFIG__ = args?.config ?? null;
    return args?.config ?? {};
  }
  if (cmd === 'pick_image_folder') return 'C:\\\\tmp\\\\flowpost';
  if (cmd === 'generate_image') {
    window.__LAST_IMAGE_PAYLOAD__ = args?.payload ?? null;
    if (window.__FAIL_IMAGE__) throw new Error('AI tạo ảnh đang gặp sự cố, vui lòng thử lại.');
    const p = args?.payload ?? {};
    if (!String(p.title || '').trim() && !String(p.body || '').trim()) throw new Error('Chưa có nội dung bài viết để tạo ảnh.');
    return {
      mediaId: 'img-1', path: 'C:\\\\tmp\\\\flowpost.png', name: 'AI.png', extension: 'png', size: 1234,
      model: p.model || 'img-model', styleId: p.styleId || 'cinematic', savedCopyPath: null,
    };
  }
  if (cmd === 'list_media') return [];
  if (cmd === 'list_providers') {
    return [{ id: 'p1', displayName: 'P1', baseUrl: 'http://127.0.0.1:20128/v1', protocol: 'openai-completions', models: ['text-model'], defaultModel: 'text-model', defaultImageModel: 'img-model', hasApiKey: true }];
  }
  throw new Error('Unknown mock command: ' + cmd);
};
`;

test.describe('AI generate_content E2E (real server)', () => {
  test.skip(
    (!process.env.E2E_API_KEY && !process.env.CI && process.env.E2E_MOCK !== 'true'),
    'E2E_API_KEY not set, skipping'
  );

  test.beforeEach(async ({ page }) => {
    if (process.env.E2E_MOCK === 'true') {
      await page.addInitScript(mockImplCode);
    }
    await page.goto('/');
    const tauri = await isTauri(page);
    if (!tauri) test.skip(true, 'Not in Tauri window');
    await ensureProvider(page);
  });

  test('generate_content with real provider returns title/body', async ({ page }) => {
    const payload = {
      prompt: 'Bí quyết năng lượng buổi sáng cho dân văn phòng',
      style: 'Viral',
      customStyle: null,
    };
    const res: any = await invoke(page, 'generate_content', { payload });
    expect(res.title).toBeTruthy();
    expect(res.body).toBeTruthy();
    expect(res.isMock).toBe(false);
    expect(res.prompt).toBe(payload.prompt);
    expect(res.style).toBe(payload.style);
  });

  test('AI Content uses provider default model unless a model is chosen', async ({ page }) => {
    await page.getByRole('button', { name: 'AI Content', exact: true }).click();

    const modelInput = page.getByRole('combobox', { name: 'Model', exact: true });
    await expect(modelInput).toHaveAttribute('placeholder', 'Mặc định: text-model');

    await page
      .getByPlaceholder('VD: Bí quyết năng lượng buổi sáng cho dân văn phòng...')
      .fill('Bí quyết năng lượng buổi sáng cho dân văn phòng');
    await page.getByRole('button', { name: 'Tạo nội dung', exact: true }).click();
    await expect(page.getByText('Đã tạo nội dung')).toBeVisible();

    const firstPayload: any = await page.evaluate(
      () => (window as any).__LAST_GENERATE_PAYLOAD__
    );
    expect(firstPayload).toBeTruthy();
    expect(Object.prototype.hasOwnProperty.call(firstPayload, 'model')).toBe(false);

    await modelInput.fill('my-text-model');
    await page.getByRole('button', { name: 'Tạo nội dung', exact: true }).click();
    await expect
      .poll(() =>
        page.evaluate(() => (window as any).__LAST_GENERATE_PAYLOAD__?.model)
      )
      .toBe('my-text-model');
  });

  test('generate_content should fail without prompt', async ({ page }) => {
    try {
      await invoke(page, 'generate_content', {
        payload: { prompt: '   ', style: 'Viral', customStyle: null },
      });
      throw new Error('should have failed');
    } catch (e) {
      expect(String(e)).toContain('prompt');
    }
  });

  test('AI Content generates an image when the checkbox is enabled', async ({ page }) => {
    await page.getByRole('button', { name: 'AI Content', exact: true }).click();
    await page.getByLabel('Tạo ảnh minh hoạ bằng AI').check();
    await page.getByPlaceholder('VD: Bí quyết năng lượng buổi sáng cho dân văn phòng...').fill('Bí quyết năng lượng buổi sáng');
    await page.getByRole('button', { name: 'Tạo nội dung', exact: true }).click();
    await expect(page.getByText('Đã tạo ảnh minh hoạ')).toBeVisible();
    const imgPayload: any = await page.evaluate(() => (window as any).__LAST_IMAGE_PAYLOAD__);
    expect(imgPayload).toBeTruthy();
    expect(imgPayload.title).toBeTruthy();
    expect(imgPayload.body).toBeTruthy();
    expect(imgPayload.styleId).toBe('cinematic');
    await expect(page.getByText('Đã gắn vào bài viết')).toBeVisible();
    await expect(page.getByText('Gắn media (1)')).toBeVisible();
  });

  test('AI Content keeps the text when image generation fails', async ({ page }) => {
    await page.getByRole('button', { name: 'AI Content', exact: true }).click();
    await page.getByLabel('Tạo ảnh minh hoạ bằng AI').check();
    await page.evaluate(() => { (window as any).__FAIL_IMAGE__ = true; });
    await page.getByPlaceholder('VD: Bí quyết năng lượng buổi sáng cho dân văn phòng...').fill('Chủ đề lỗi ảnh');
    await page.getByRole('button', { name: 'Tạo nội dung', exact: true }).click();
    await expect(page.getByText('Đã tạo nội dung')).toBeVisible();
    await expect(page.getByText('AI tạo ảnh đang gặp sự cố')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Thử tạo lại ảnh' })).toBeVisible();
    await expect(page.locator('textarea').nth(1)).toHaveValue('Nội dung được tạo bởi mock');
  });
});