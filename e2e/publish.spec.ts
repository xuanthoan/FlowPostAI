import { test, expect } from '@playwright/test';
import { invoke, isTauri } from './helpers/tauri';

const mockImplCode = `
window.__TAURI_MOCK__ = true;
window.__TAURI_INTERNALS__ = {};
window.__TAURI_MOCK_IMPL__ = async (cmd, args) => {
  switch (cmd) {
    case 'validate_facebook_token': {
      const t = (args?.token || '').trim();
      if (!t) throw new Error('Vui lòng nhập Facebook Token');
      if (t.length < 10) throw new Error('Token quá ngắn, vui lòng kiểm tra lại');
      if (t.includes('invalid')) throw new Error('Token không hợp lệ hoặc hết hạn (code 190): Invalid OAuth access token');
      return { id: '123456789', name: 'Test User' };
    }
    case 'list_facebook_pages': {
      let t = (args?.token || '').trim();
      if (!t) t = 'cached_mock_token';
      if (!t) throw new Error('Vui lòng nhập Facebook Token');
      return [{ id: '111', name: 'Sống Tích Cực' }, { id: '222', name: 'Daily Motivation' }];
    }
    case 'publish_content': {
      const pid = (args?.page_id || '').trim();
      const msg = (args?.message || '').trim();
      const imgs = Array.isArray(args?.image_paths)
        ? args.image_paths
        : (args?.image_path ? [args.image_path] : []);
      if (!pid) throw new Error('Thiếu pageId');
      if (!msg) throw new Error('Vui lòng nhập nội dung');
      if (imgs.length > 10) throw new Error('Chỉ đăng tối đa 10 ảnh mỗi bài');
      for (const p of imgs) {
        if (String(p).endsWith('.exe')) throw new Error('Định dạng ảnh không hỗ trợ (chỉ jpg/jpeg/png/webp/gif)');
        if (String(p).includes('notfound')) throw new Error('Không tìm thấy tệp ảnh');
      }
      return { id: 'mock_post_123', imageCount: imgs.length, imagePaths: imgs };
    }
    case 'get_scheduler_config':
      return { intervalSeconds: 30, enabled: true, selectedPageId: null };
    default:
      throw new Error('Unknown mock command: ' + cmd);
  }
};
`;

test.describe('Publish E2E (mock)', () => {
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

  test('publish_content text should succeed', async ({ page }) => {
    const res: any = await invoke(page, 'publish_content', { page_id: '111', message: 'hello from e2e', image_path: null });
    expect(res.id).toBe('mock_post_123');
  });

  test('publish_content with image .exe should fail', async ({ page }) => {
    try {
      await invoke(page, 'publish_content', { page_id: '111', message: 'hello', image_path: 'C:\\tmp\\a.exe' });
      throw new Error('should have failed');
    } catch (e) {
      expect(String(e)).toContain('Định dạng ảnh không hỗ trợ');
    }
  });

  test('validate_facebook_token invalid should fail', async ({ page }) => {
    try {
      await invoke(page, 'validate_facebook_token', { token: 'invalid_token_123' });
      throw new Error('should have failed');
    } catch (e) {
      expect(String(e)).toContain('Token không hợp lệ');
    }
  });

  test('list_facebook_pages returns 2 pages', async ({ page }) => {
    const list: any[] = await invoke(page, 'list_facebook_pages', { token: '' });
    expect(Array.isArray(list)).toBe(true);
    expect(list.length).toBe(2);
    expect(list[0].id).toBe('111');
  });

  test('publish_content with 2 images should succeed', async ({ page }) => {
    const res: any = await invoke(page, 'publish_content', {
      page_id: '111', message: 'album',
      image_path: 'C:\\tmp\\a.png',
      image_paths: ['C:\\tmp\\a.png', 'C:\\tmp\\b.jpg'],
    });
    expect(res.id).toBe('mock_post_123');
    expect(res.imageCount).toBe(2);
  });

  test('publish_content with 11 images should fail', async ({ page }) => {
    try {
      await invoke(page, 'publish_content', {
        page_id: '111', message: 'too many', image_path: null,
        image_paths: Array.from({ length: 11 }, (_, i) => `C:\\tmp\\${i}.png`),
      });
      throw new Error('should have failed');
    } catch (e) {
      expect(String(e)).toContain('tối đa 10');
    }
  });
});
