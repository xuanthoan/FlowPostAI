# FlowPostAI 0.4.0 — Release Notes

Phát hành: 2026-10-27
Build từ commit: `28ebd4b`

## Có gì mới

- Trang `AI Content` bố cục 2 cột: soạn nội dung bên trái, tuỳ chọn ảnh AI bên phải.
- Nút gộp `Tạo nội dung + Ảnh AI`: một lần bấm tạo cả bài viết và 1 ảnh minh hoạ.
- Hộp hướng dẫn trong cột ảnh dành cho người dùng lần đầu: ảnh tạo từ Tiêu đề + Nội dung (không cần prompt ảnh riêng), 2 cách dùng (tự động / thủ công), ảnh tự lưu vào Thư viện media và tự gắn vào bài.
- Trang `Đăng bài` bố cục 2 cột (soạn bài + media).
- `Kho nội dung` có nút theo từng bài: `Tạo ảnh AI` (bài chưa có ảnh) và `Đổi ảnh AI` (bài đã có ảnh) với lựa chọn `Thêm ảnh` / `Thay ảnh`, giới hạn tối đa 10 ảnh mỗi bài.
- Sửa lỗi bộ cài MSI thiếu icon (`bundle.icon`).

## Cài đặt

Bộ cài cho Windows x64:

- MSI: `src-tauri\target\release\bundle\msi\FlowPost AI_0.4.0_x64_en-US.msi`
- NSIS: `src-tauri\target\release\bundle\nsis\FlowPost AI_0.4.0_x64-setup.exe`

Cài đè lên 0.3.0 được, không cần gỡ bản cũ. Dữ liệu người dùng giữ nguyên vì nằm trong `app_data_dir()` của hệ điều hành, không nằm trong thư mục cài đặt — gỡ/cài bộ cài không xoá nội dung, media, lịch đăng hay cấu hình AI.

## Yêu cầu hệ thống

- Windows x64.
- Facebook Page token có đủ 5 quyền: `pages_show_list`, `pages_manage_posts`, `pages_read_engagement`, `pages_read_user_content`, `read_insights`.
- Cần cấu hình AI provider trong `Cài đặt` → `Model AI` trước khi dùng các tính năng AI (tạo nội dung, tạo ảnh).

## Hạn chế đã biết

1. Bộ cài chưa ký số → Windows SmartScreen có thể cảnh báo khi chạy lần đầu.
2. Không có auto-updater; nâng cấp bằng cách cài đè bộ cài mới.
3. E2E chỉ chạy ở chế độ mock (`playwright.e2e-mock.config.ts`), chưa có test với Facebook Graph thật.
4. Guard giới hạn 10 ảnh trong `Kho nội dung` chưa có test tự động.
5. Bài có prompt rỗng bị chặn khi tạo ảnh AI từ `Kho nội dung` (phải bấm `Sửa` để thêm prompt).
6. `secure-credentials.json` là fallback plaintext khi máy không có secure store.
7. Chưa test tạo ảnh với provider ảnh thật trên bản Tauri desktop.

## Bằng chứng kiểm chứng

| Hạng mục | Kết quả |
| --- | --- |
| ESLint (`npm exec -- eslint .`) | exit 0, 0 findings |
| `npm exec -- vite build` | success (Vite 8.2.2) |
| `cargo test --lib` | 352 passed / 0 failed |
| E2E mock (`playwright.e2e-mock.config.ts`) | 60 passed / 0 failed |
| MSI | `FlowPost AI_0.4.0_x64_en-US.msi` — 4923392 bytes — SHA256: `00ec309ae2d500cfe408587e51a8f2073c60f45f59a00296f7d4ea6c2f3983da` |
| NSIS | `FlowPost AI_0.4.0_x64-setup.exe` — 3451928 bytes — SHA256: `ae9860b1e2d46464b84a4af34d22c515afe32e268abd05eef9944a3efc277e37` |
| `flowpost-ai.exe` | 13819392 bytes — 2026-10-27 16:50 |

## Rollback

Cài lại bộ cài 0.3.0:

- MSI: `src-tauri\target\release\bundle\msi\FlowPost AI_0.3.0_x64_en-US.msi`
- NSIS: `src-tauri\target\release\bundle\nsis\FlowPost AI_0.3.0_x64-setup.exe`

0.4.0 không thay đổi định dạng `content-index.json`, nên rollback về 0.3.0 không mất dữ liệu. Dữ liệu vẫn nằm trong `app_data_dir()` và được bản cũ đọc lại bình thường.
