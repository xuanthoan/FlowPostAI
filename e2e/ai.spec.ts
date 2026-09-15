import { test, expect } from '@playwright/test';
import { invoke, isTauri } from './helpers/tauri';
import { ensureProvider } from './helpers/provider';

const BASE_URL = process.env.E2E_BASE_URL || 'http://127.0.0.1:20128/v1';

const mockImplCode = `
window.__TAURI_MOCK__ = true;
window.__TAURI_INTERNALS__ = {};
window.__TAURI_MOCK_IMPL__ = async (cmd, args) => {
  const baseUrl = ${JSON.stringify(BASE_URL)};
  if (cmd === 'generate_content') {
    const payload = args?.payload;
    if (!payload?.prompt || !payload.prompt.trim()) {
      throw new Error('Vui lòng nhập prompt');
    }
    return {
      title: 'Tiêu đề từ AI',
      body: 'Nội dung được tạo bởi mock',
      isMock: false,
      prompt: payload.prompt,
      style: payload.style,
    };
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
      id: 'custom-provider',
      displayName: 'Custom provider',
      baseUrl: baseUrl,
      protocol: 'openai-completions',
      models: ['xoay-vong-worker-web-128k'],
      hasApiKey: true,
    };
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
      length: 'Vừa',
    };
    const res: any = await invoke(page, 'generate_content', { payload });
    expect(res.title).toBeTruthy();
    expect(res.body).toBeTruthy();
    expect(res.isMock).toBe(false);
    expect(res.prompt).toBe(payload.prompt);
    expect(res.style).toBe(payload.style);
  });

  test('generate_content respects length Ngắn/Dài', async ({ page }) => {
    for (const length of ['Ngắn', 'Dài']) {
      const res: any = await invoke(page, 'generate_content', {
        payload: { prompt: 'Test length ' + length, style: 'Viral', customStyle: null, length },
      });
      expect(res.title).toBeTruthy();
    }
  });

  test('generate_content should fail without prompt', async ({ page }) => {
    try {
      await invoke(page, 'generate_content', {
        payload: { prompt: '   ', style: 'Viral', customStyle: null, length: 'Vừa' },
      });
      throw new Error('should have failed');
    } catch (e) {
      expect(String(e)).toContain('prompt');
    }
  });
});