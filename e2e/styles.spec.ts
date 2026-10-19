import { test, expect } from '@playwright/test';
import { invoke, isTauri } from './helpers/tauri';

const mockImplCode = `
window.__TAURI_MOCK__ = true;
window.__TAURI_INTERNALS__ = {};
window.__STYLES__ = [
  { id: 'viral', name: 'Viral', content: 'Nội dung Viral mặc định', builtin: true },
  { id: 'motivational', name: 'Motivational', content: 'Nội dung Motivational mặc định', builtin: true },
  { id: 'story', name: 'Story', content: 'Nội dung Story mặc định', builtin: true },
];
window.__TAURI_MOCK_IMPL__ = async (cmd, args) => {
  const styles = window.__STYLES__;
  const findByName = (name, excludeId) => styles.find(s => s.id !== excludeId && s.name.trim().toLowerCase() === String(name).trim().toLowerCase());
  switch (cmd) {
    case 'list_styles':
      return styles.map(s => ({ ...s }));
    case 'create_style': {
      const name = String(args?.payload?.name ?? '').trim();
      const content = String(args?.payload?.content ?? '').trim();
      if (!name) throw new Error('Tên style không được để trống');
      if (!content) throw new Error('Nội dung style không được để trống');
      if (findByName(name, null)) throw new Error("Tên style '" + name + "' đã tồn tại");
      const style = { id: 'style-' + (styles.length + 1) + '-' + Date.now(), name, content, builtin: false };
      styles.push(style);
      return { ...style };
    }
    case 'update_style': {
      const id = args?.payload?.id;
      const name = String(args?.payload?.name ?? '').trim();
      const content = String(args?.payload?.content ?? '').trim();
      const target = styles.find(s => s.id === id);
      if (!target) throw new Error("Không tìm thấy style '" + id + "'");
      if (!name) throw new Error('Tên style không được để trống');
      if (findByName(name, id)) throw new Error("Tên style '" + name + "' đã tồn tại");
      target.name = name;
      target.content = content;
      return { ...target };
    }
    case 'duplicate_style': {
      const id = args?.id;
      const source = styles.find(s => s.id === id);
      if (!source) throw new Error("Không tìm thấy style '" + id + "'");
      const base = source.name.trim();
      let n = 1;
      let candidate = base + ' (bản sao)';
      while (findByName(candidate, null)) { n += 1; candidate = base + ' (bản sao ' + n + ')'; }
      const style = { id: 'dup-' + Date.now() + '-' + n, name: candidate, content: source.content, builtin: false };
      styles.push(style);
      return { ...style };
    }
    case 'delete_style': {
      const id = args?.id;
      const target = styles.find(s => s.id === id);
      if (!target) throw new Error("Không tìm thấy style '" + id + "'");
      if (target.builtin) throw new Error('Không thể xóa style mặc định. Dùng "Khôi phục mặc định" để đặt lại.');
      styles.splice(styles.indexOf(target), 1);
      return;
    }
    case 'reset_style': {
      const id = args?.id;
      const defaults = {
        viral: 'Nội dung Viral mặc định',
        motivational: 'Nội dung Motivational mặc định',
        story: 'Nội dung Story mặc định',
      };
      const target = styles.find(s => s.id === id);
      if (!target) throw new Error("Không tìm thấy style '" + id + "'");
      if (!(id in defaults)) throw new Error('Chỉ có thể khôi phục mặc định cho style mặc định');
      target.content = defaults[id];
      target.builtin = true;
      return { ...target };
    }
    default:
      throw new Error('Unknown mock command: ' + cmd);
  }
};
`;

test.describe('Styles E2E', () => {
  test.skip(
    (!process.env.E2E_API_KEY && !process.env.CI && process.env.E2E_MOCK !== 'true'),
    'E2E_MOCK not enabled, skipping styles E2E. Set E2E_MOCK=true to run.'
  );

  test.beforeEach(async ({ page }) => {
    if (process.env.E2E_MOCK === 'true') {
      await page.addInitScript(mockImplCode);
    }
    await page.goto('/');
    const tauri = await isTauri(page);
    if (!tauri) {
      test.skip(true, 'Not in Tauri window, skip. Run via npm run tauri:dev + tauri-driver');
    }
  });

  test('list_styles returns the 3 seeded built-ins', async ({ page }) => {
    const list: any[] = await invoke(page, 'list_styles');
    expect(Array.isArray(list)).toBe(true);
    expect(list.length).toBe(3);
    expect(list.map((s) => s.id)).toEqual(['viral', 'motivational', 'story']);
    for (const s of list) {
      expect(s.builtin).toBe(true);
      expect(s.name.length).toBeGreaterThan(0);
      expect(s.content.length).toBeGreaterThan(0);
    }
  });

  test('create_style validates an empty name', async ({ page }) => {
    try {
      await invoke(page, 'create_style', { payload: { name: '   ', content: 'nội dung' } });
      throw new Error('should have failed');
    } catch (e) {
      expect(String(e)).toContain('Tên style không được để trống');
    }
  });

  test('update_style changes content', async ({ page }) => {
    const updated: any = await invoke(page, 'update_style', {
      payload: { id: 'viral', name: 'Viral', content: 'Nội dung đã sửa' },
    });
    expect(updated.name).toBe('Viral');
    expect(updated.content).toBe('Nội dung đã sửa');
    const list: any[] = await invoke(page, 'list_styles');
    const viral = list.find((s) => s.id === 'viral');
    expect(viral.content).toBe('Nội dung đã sửa');
  });

  test('duplicate_style adds a "(bản sao)" suffix', async ({ page }) => {
    const dup: any = await invoke(page, 'duplicate_style', { id: 'viral' });
    expect(dup.builtin).toBe(false);
    expect(dup.id).not.toBe('viral');
    expect(dup.name).toBe('Viral (bản sao)');
    expect(dup.content).toBe('Nội dung Viral mặc định');
  });

  test('delete_style rejects a built-in', async ({ page }) => {
    try {
      await invoke(page, 'delete_style', { id: 'viral' });
      throw new Error('should have failed');
    } catch (e) {
      expect(String(e)).toContain('Không thể xóa style mặc định');
    }
    const list: any[] = await invoke(page, 'list_styles');
    expect(list.some((s) => s.id === 'viral')).toBe(true);
  });

  test('reset_style returns the default content', async ({ page }) => {
    await invoke(page, 'update_style', {
      payload: { id: 'story', name: 'Story', content: 'Nội dung bị đổi' },
    });
    const reset: any = await invoke(page, 'reset_style', { id: 'story' });
    expect(reset.content).toBe('Nội dung Story mặc định');
    expect(reset.builtin).toBe(true);
  });
});
