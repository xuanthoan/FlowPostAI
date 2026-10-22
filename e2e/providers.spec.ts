import { test, expect } from '@playwright/test';
import { invoke, isTauri } from './helpers/tauri';
import { ensureProvider, DEFAULT_MODEL } from './helpers/provider';

const BASE_URL = process.env.E2E_BASE_URL || 'http://127.0.0.1:20128/v1';

const mockImplCode = `
window.__TAURI_MOCK__ = true;
window.__TAURI_INTERNALS__ = {};
window.__TAURI_MOCK_IMPL__ = async (cmd, args) => {
  const baseUrl = ${JSON.stringify(BASE_URL)};
  let activeId = 'custom-provider';
  switch (cmd) {
    case 'list_providers':
      return [
        {
          id: 'custom-provider',
          displayName: 'Custom provider',
          baseUrl: baseUrl,
          protocol: 'openai-completions',
          models: ['xoay-vong-worker-web-128k'],
          defaultImageModel: 'img-model',
          hasApiKey: true,
        },
      ];
    case 'get_active_provider':
      return {
        id: activeId,
        displayName: 'Custom provider',
        baseUrl: baseUrl,
        protocol: 'openai-completions',
        models: ['xoay-vong-worker-web-128k'],
        defaultImageModel: 'img-model',
        hasApiKey: true,
      };
    case 'update_provider': {
      const protocol = args?.payload?.protocol;
      const validProtocols = ['openai-completions', 'openai-responses', 'anthropic-messages'];
      if (protocol && !validProtocols.includes(protocol)) {
        throw new Error("Protocol không hợp lệ: " + protocol + ". Chỉ hỗ trợ: " + validProtocols.join(", "));
      }
      return {
        id: 'custom-provider',
        displayName: 'Custom provider',
        baseUrl: baseUrl,
        protocol: protocol || 'openai-completions',
        models: ['xoay-vong-worker-web-128k'],
        defaultImageModel: 'img-model',
        hasApiKey: true,
      };
    }
    case 'create_provider': {
      const id = args?.payload?.id;
      if (!id || !/^[a-z][a-z0-9-]*$/.test(id) || id.includes('--')) {
        throw new Error('Provider ID không hợp lệ: phải lowercase, bắt đầu bằng chữ cái, chỉ chứa a-z, 0-9, \\'-\\', không --, 1-64 ký tự');
      }
      return { id, hasApiKey: true };
    }
    case 'fetch_provider_models':
      return ['xoay-vong-worker-web-128k'];
    case 'set_active_provider':
      activeId = args?.id ?? activeId;
      return {
        id: activeId,
        displayName: 'Custom provider',
        baseUrl: baseUrl,
        protocol: 'openai-completions',
        models: ['xoay-vong-worker-web-128k'],
        defaultImageModel: 'img-model',
        hasApiKey: true,
      };
    case 'delete_provider':
      return;
    default:
      throw new Error('Unknown mock command: ' + cmd);
  }
};
`;

test.describe('Providers E2E (real server)', () => {
  test.skip(
    (!process.env.E2E_API_KEY && !process.env.CI && process.env.E2E_MOCK !== 'true'),
    'E2E_API_KEY not set, skipping real server test. Set E2E_API_KEY=sk-... to run.'
  );

  test.beforeEach(async ({ page }) => {
    if (process.env.E2E_MOCK === 'true') {
      await page.addInitScript(mockImplCode);
    }
    await page.goto('/');
    // Wait for Tauri to be ready; if not in Tauri, skip
    const tauri = await isTauri(page);
    if (!tauri) {
      test.skip(true, 'Not in Tauri window, skip. Run via npm run tauri:dev + tauri-driver');
    }
  });

  test('listProviders returns custom-provider default', async ({ page }) => {
    const list: any[] = await invoke(page, 'list_providers');
    expect(Array.isArray(list)).toBe(true);
    expect(list.length).toBeGreaterThanOrEqual(1);
    const custom = list.find((p) => p.id === 'custom-provider');
    expect(custom).toBeTruthy();
    expect(custom.baseUrl).toContain('http');
    expect(custom.protocol).toBe('openai-completions');
  });

  test('updateProvider and get_active_provider', async ({ page }) => {
    const active = await ensureProvider(page);
    expect(active.id).toBe('custom-provider');
    expect(active.hasApiKey).toBe(true);
    expect(active.baseUrl).toBe(BASE_URL);
  });

  test('fetch_provider_models with real server', async ({ page }) => {
    await ensureProvider(page);
    const models: string[] = await invoke(page, 'fetch_provider_models', { id: 'custom-provider' });
    expect(Array.isArray(models)).toBe(true);
    // Real server should return at least the requested model
    expect(models.length).toBeGreaterThan(0);
    expect(models).toContain(DEFAULT_MODEL);
  });

  test('create_provider with invalid id should fail (MVP validation)', async ({ page }) => {
    try {
      await invoke(page, 'create_provider', {
        payload: {
          id: 'Invalid_ID',
          baseUrl: 'http://127.0.0.1:20128/v1',
          protocol: 'openai-completions',
          models: [DEFAULT_MODEL],
          apiKey: 'dummy',
        },
      });
      throw new Error('should have failed');
    } catch (e) {
      expect(String(e)).toContain('Provider ID không hợp lệ');
    }
  });

  test('update_provider with invalid protocol should fail', async ({ page }) => {
    try {
      await invoke(page, 'update_provider', {
        id: 'custom-provider',
        payload: { protocol: 'invalid-protocol' },
      });
      throw new Error('should have failed');
    } catch (e) {
      expect(String(e)).toContain('Protocol không hợp lệ');
    }
  });

  test('create_provider second provider succeeds (multi-provider)', async ({ page }) => {
    const secondId = 'second-provider';
    const res: any = await invoke(page, 'create_provider', {
      payload: {
        id: secondId,
        displayName: 'Second Provider',
        baseUrl: BASE_URL,
        protocol: 'openai-responses',
        models: [DEFAULT_MODEL],
        apiKey: 'dummy-key-2',
      },
    });
    expect(res.id).toBe(secondId);
    // cleanup: try delete if real server, ignore if mock
    try { await invoke(page, 'delete_provider', { id: secondId }); } catch {}
  });

  test('update_provider with anthropic-messages should succeed', async ({ page }) => {
    const res: any = await invoke(page, 'update_provider', {
      id: 'custom-provider',
      payload: { protocol: 'anthropic-messages' },
    });
    expect(res.protocol).toBe('anthropic-messages');
    // restore
    await invoke(page, 'update_provider', {
      id: 'custom-provider',
      payload: { protocol: 'openai-completions' },
    });
  });

  test('set_active_provider updates active provider', async ({ page }) => {
    await ensureProvider(page);
    const res: any = await invoke(page, 'set_active_provider', { id: 'custom-provider' });
    expect(res.id).toBe('custom-provider');
    const active: any = await invoke(page, 'get_active_provider');
    expect(active.id).toBe('custom-provider');
  });
});