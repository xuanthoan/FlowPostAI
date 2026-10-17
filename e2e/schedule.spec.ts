import { test, expect } from '@playwright/test';
import { invoke, isTauri } from './helpers/tauri';

const mockImplCode = `
window.__TAURI_MOCK__ = true;
window.__TAURI_INTERNALS__ = {};
(() => {
  let mockContents = [];
  let mockSchedules = [];
  let mockSelectedPageId = null;
  let mockConfig = { intervalSeconds: 30, enabled: true, selectedPageId: null };
  window.__TAURI_MOCK_IMPL__ = async (cmd, args) => {
    const baseUrl = 'http://127.0.0.1:20128/v1';
    switch (cmd) {
      case 'list_providers':
        return [{ id: 'custom-provider', displayName: 'Custom provider', baseUrl: baseUrl, protocol: 'openai-completions', models: ['xoay-vong-worker-web-128k'], hasApiKey: true }];
      case 'get_active_provider':
        return { id: 'custom-provider', displayName: 'Custom provider', baseUrl: baseUrl, protocol: 'openai-completions', models: ['xoay-vong-worker-web-128k'], hasApiKey: true };
      case 'update_provider': {
        const protocol = args?.payload?.protocol;
        const validProtocols = ['openai-completions', 'openai-responses', 'anthropic-messages'];
        if (protocol && !validProtocols.includes(protocol)) throw new Error("Protocol không hợp lệ: " + protocol);
        return { id: 'custom-provider', protocol: protocol || 'openai-completions', hasApiKey: true };
      }
      case 'create_provider': {
        const id = args?.payload?.id;
        if (!id || !/^[a-z][a-z0-9-]*$/.test(id) || id.includes('--')) throw new Error('Provider ID không hợp lệ');
        return { id, hasApiKey: true };
      }
      case 'fetch_provider_models':
        return ['xoay-vong-worker-web-128k'];
      case 'delete_provider':
        return;
      case 'list_content':
        return mockContents;
      case 'get_content': {
        const found = mockContents.find(c => c.id === args?.id);
        return found || null;
      }
      case 'save_content': {
        const p = args?.payload;
        if (!p?.prompt?.trim()) throw new Error('Prompt không được để trống');
        if (!p?.title?.trim() && !p?.body?.trim()) throw new Error('Tiêu đề hoặc nội dung không được để trống');
        const item = { id: 'c_' + Date.now() + '_' + Math.random().toString(36).slice(2,6), title: p.title, body: p.body, prompt: p.prompt, style: p.style, customStyle: p.customStyle || null, mediaIds: p.mediaIds || [], status: 'draft', createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() };
        mockContents.push(item);
        return item;
      }
      case 'update_content_status': {
        const it = mockContents.find(c => c.id === args?.id);
        if (!it) throw new Error('Không tìm thấy ' + args?.id);
        it.status = args?.status;
        it.updatedAt = new Date().toISOString();
        return it;
      }
      case 'delete_content': {
        const len = mockContents.length;
        mockContents = mockContents.filter(c => c.id !== args?.id);
        if (mockContents.length === len) throw new Error('Không tìm thấy nội dung ' + args?.id);
        return;
      }
      case 'list_schedule': {
        const sorted = [...mockSchedules].sort((a,b) => new Date(a.scheduledAt) - new Date(b.scheduledAt));
        return sorted;
      }
      case 'get_schedule': {
        return mockSchedules.find(s => s.id === args?.id) || null;
      }
      case 'create_schedule': {
        const p = args?.payload;
        if (!p?.contentId?.trim()) throw new Error('Thiếu contentId');
        if (!p?.platform?.trim()) throw new Error('Vui lòng chọn nền tảng');
        const scheduledAt = new Date(p.scheduledAt || p.scheduled_at);
        if (scheduledAt <= new Date(Date.now() + 5*60*1000)) throw new Error('Thời gian lên lịch phải trong tương lai (tối thiểu 5 phút)');
        const content = mockContents.find(c => c.id === p.contentId);
        if (!content) throw new Error('Không tìm thấy nội dung ' + p.contentId);
        if (content.status !== 'approved') throw new Error('Chỉ nội dung đã duyệt mới được lên lịch');
        const resolvedPageId = (p.pageId || p.page_id || (p.pages && p.pages[0]) || '').trim();
        const now = new Date().toISOString();
        const item = { id: 's_' + Date.now() + '_' + Math.random().toString(36).slice(2,6), contentId: p.contentId, scheduledAt: scheduledAt.toISOString(), platform: p.platform.trim(), pages: p.pages || [], pageId: resolvedPageId || null, status: 'Scheduled', retryCount: 0, lastError: null, createdAt: now, updatedAt: now };
        mockSchedules.push(item);
        return item;
      }
      case 'delete_schedule': {
        const len = mockSchedules.length;
        mockSchedules = mockSchedules.filter(s => s.id !== args?.id);
        if (mockSchedules.length === len) throw new Error('Không tìm thấy lịch ' + args?.id);
        return;
      }
      case 'update_schedule_status': {
        const it = mockSchedules.find(s => s.id === args?.id);
        if (!it) throw new Error('Không tìm thấy ' + args?.id);
        it.status = args?.status;
        it.updatedAt = new Date().toISOString();
        return it;
      }
      case 'reschedule': {
        const at = new Date(args?.scheduledAt || args?.scheduled_at);
        if (at <= new Date(Date.now() + 5*60*1000)) throw new Error('Thời gian lên lịch phải trong tương lai (tối thiểu 5 phút)');
        const it = mockSchedules.find(s => s.id === args?.id);
        if (!it) throw new Error('Không tìm thấy ' + args?.id);
        it.scheduledAt = at.toISOString();
        it.updatedAt = new Date().toISOString();
        return it;
      }
      case 'get_scheduler_config':
        return { intervalSeconds: mockConfig.intervalSeconds, enabled: mockConfig.enabled, selectedPageId: mockSelectedPageId };
      case 'set_scheduler_config': {
        const cfg = args?.config;
        if (!cfg || cfg.intervalSeconds == 0 || cfg.intervalSeconds > 3600) throw new Error('Khoảng kiểm tra phải từ 1 đến 3600 giây');
        mockConfig = cfg;
        return;
      }
      case 'select_scheduler_page': {
        mockSelectedPageId = args?.pageId || args?.page_id || null;
        mockConfig.selectedPageId = mockSelectedPageId;
        return;
      }
      case 'list_facebook_pages':
        return [{ id: '111', name: 'Sống Tích Cực' }, { id: '222', name: 'Daily Motivation' }];
      case 'validate_facebook_token': {
        const t = (args?.token || '').trim();
        if (!t) throw new Error('Vui lòng nhập Facebook Token');
        return { id: '123', name: 'Test User' };
      }
      case 'publish_content': {
        const pid = (args?.page_id || args?.pageId || '').trim();
        const msg = (args?.message || '').trim();
        if (!pid) throw new Error('Thiếu pageId');
        if (!msg) throw new Error('Vui lòng nhập nội dung');
        return { id: 'mock_post_123' };
      }
      case 'credential_status':
        return { hasFacebookToken: true };
      case 'list_media':
        return [];
      default:
        throw new Error('Unknown mock command: ' + cmd);
    }
  };
})();
`;

test.describe('Schedule E2E (mock)', () => {
  test.skip(
    (!process.env.E2E_API_KEY && !process.env.CI && process.env.E2E_MOCK !== 'true'),
    'E2E_API_KEY not set, skipping. Set E2E_MOCK=true to run mock.',
  );

  test.beforeEach(async ({ page }) => {
    if (process.env.E2E_MOCK === 'true') {
      await page.addInitScript(mockImplCode);
    }
    await page.goto('/');
    const tauri = await isTauri(page);
    if (!tauri) test.skip(true, 'Not in Tauri window, skip. Run via npm run tauri:dev + tauri-driver');
  });

  test('create_schedule should succeed for Approved content', async ({ page }) => {
    const saved: any = await invoke(page, 'save_content', { payload: { title: 'Test', body: 'Body', prompt: 'prompt', style: 'Viral', customStyle: null, mediaIds: [] } });
    await invoke(page, 'update_content_status', { id: saved.id, status: 'approved' });
    const future = new Date(Date.now() + 10 * 60 * 1000).toISOString();
    const item: any = await invoke(page, 'create_schedule', { payload: { contentId: saved.id, scheduledAt: future, platform: 'Sống Tích Cực', pageId: '111', pages: ['111'] } });
    expect(item.status).toBe('Scheduled');
    expect(item.pageId).toBe('111');
    const list: any[] = await invoke(page, 'list_schedule');
    expect(list.find(s => s.id === item.id)).toBeTruthy();
    await invoke(page, 'delete_schedule', { id: item.id });
    await invoke(page, 'delete_content', { id: saved.id });
  });

  test('create_schedule should fail for Draft', async ({ page }) => {
    const saved: any = await invoke(page, 'save_content', { payload: { title: 'Draft', body: 'Body', prompt: 'prompt', style: 'Viral', customStyle: null, mediaIds: [] } });
    const future = new Date(Date.now() + 10 * 60 * 1000).toISOString();
    try {
      await invoke(page, 'create_schedule', { payload: { contentId: saved.id, scheduledAt: future, platform: 'Sống Tích Cực', pageId: '111', pages: ['111'] } });
      throw new Error('should have failed');
    } catch (e) {
      expect(String(e)).toContain('Chỉ nội dung đã duyệt');
    }
    await invoke(page, 'delete_content', { id: saved.id });
  });

  test('create_schedule should fail if scheduledAt < now+5m', async ({ page }) => {
    const saved: any = await invoke(page, 'save_content', { payload: { title: 'Test', body: 'Body', prompt: 'prompt', style: 'Viral', customStyle: null, mediaIds: [] } });
    await invoke(page, 'update_content_status', { id: saved.id, status: 'approved' });
    const soon = new Date(Date.now() + 1 * 60 * 1000).toISOString();
    try {
      await invoke(page, 'create_schedule', { payload: { contentId: saved.id, scheduledAt: soon, platform: 'Sống Tích Cực', pageId: '111', pages: ['111'] } });
      throw new Error('should have failed');
    } catch (e) {
      expect(String(e)).toContain('tối thiểu 5 phút');
    }
    await invoke(page, 'delete_content', { id: saved.id });
  });

  test('reschedule should move time', async ({ page }) => {
    const saved: any = await invoke(page, 'save_content', { payload: { title: 'Test', body: 'Body', prompt: 'prompt', style: 'Viral', customStyle: null, mediaIds: [] } });
    await invoke(page, 'update_content_status', { id: saved.id, status: 'approved' });
    const future = new Date(Date.now() + 10 * 60 * 1000).toISOString();
    const item: any = await invoke(page, 'create_schedule', { payload: { contentId: saved.id, scheduledAt: future, platform: 'Sống Tích Cực', pageId: '111', pages: ['111'] } });
    const newTime = new Date(Date.now() + 20 * 60 * 1000).toISOString();
    const updated: any = await invoke(page, 'reschedule', { id: item.id, scheduledAt: newTime });
    expect(new Date(updated.scheduledAt).getTime()).toBeGreaterThan(new Date(item.scheduledAt).getTime());
    await invoke(page, 'delete_schedule', { id: item.id });
    await invoke(page, 'delete_content', { id: saved.id });
  });

  test('update_schedule_status Cancelled should persist', async ({ page }) => {
    const saved: any = await invoke(page, 'save_content', { payload: { title: 'Test', body: 'Body', prompt: 'prompt', style: 'Viral', customStyle: null, mediaIds: [] } });
    await invoke(page, 'update_content_status', { id: saved.id, status: 'approved' });
    const future = new Date(Date.now() + 10 * 60 * 1000).toISOString();
    const item: any = await invoke(page, 'create_schedule', { payload: { contentId: saved.id, scheduledAt: future, platform: 'Sống Tích Cực', pageId: '111', pages: ['111'] } });
    const updated: any = await invoke(page, 'update_schedule_status', { id: item.id, status: 'Cancelled' });
    expect(updated.status).toBe('Cancelled');
    await invoke(page, 'delete_schedule', { id: item.id });
    await invoke(page, 'delete_content', { id: saved.id });
  });
});
