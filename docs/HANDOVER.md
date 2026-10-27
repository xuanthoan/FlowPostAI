# FlowPostAI — Bàn giao dự án

Tài liệu bàn giao cho người tiếp nhận codebase FlowPost AI (phiên bản 0.4.0).

## Tổng quan

- Desktop app Windows, xây bằng **Tauri v2** (Rust backend) + **React 19** + **Vite**.
- Toàn bộ dữ liệu lưu cục bộ trên máy người dùng (`app_data_dir()` của hệ điều hành); không có server trung gian, không có tài khoản/đăng nhập riêng.
- Backend Rust chịu trách nhiệm: đọc/ghi file index, gọi Facebook Graph API, gọi AI provider, lưu secret, chạy scheduler. Frontend React chỉ gọi qua IPC.

## Điểm vào

- Frontend: `src/App.jsx` — monolith ~2900 dòng (2.985 dòng tại thời điểm 0.4.0), chứa toàn bộ page/component. Khi sửa lớn nên tách dần thành module theo page (ví dụ `AI Content`, `Kho nội dung`, `Đăng bài`, `Cài đặt`, `Báo cáo`) trước khi thêm tính năng mới.
- Các file frontend phụ: `src/main.jsx` (mount), `src/tauri.js` (wrapper `invokeDesktop`/`isDesktop`/`localAssetUrl`), `src/providerPresets.js`, `src/reportCsv.js`, `src/styles.css`.
- Backend: `src-tauri/src/lib.rs` — khai báo state, media command, `run()` và `generate_handler!`.
- Module `.rs` trong `src-tauri/src/`:
  - `lib.rs` — media (import/list/delete/location), `save_report_csv`, entrypoint `run()`.
  - `main.rs` — gọi `flowpost_ai_lib::run()`.
  - `ai.rs` — sinh nội dung qua AI provider.
  - `image_gen.rs` — cấu hình và sinh ảnh minh hoạ.
  - `image_styles.rs` — CRUD phong cách ảnh.
  - `styles.rs` — CRUD phong cách viết.
  - `topics.rs` — CRUD chủ đề.
  - `providers.rs` — CRUD provider AI + model.
  - `content.rs` — kho nội dung (`content-index.json`).
  - `schedule.rs` — lịch đăng thủ công + scheduler.
  - `auto_schedule.rs` — lịch đăng tự động + auto scheduler.
  - `facebook.rs` — Graph API (token, page, publish, posts, insights, report).
  - `security.rs` — lưu/đọc secret (keyring + fallback `secure-credentials.json`).

## IPC surface

Tổng số command đăng ký trong `generate_handler![...]` của `src-tauri/src/lib.rs`: **71**.

Content (6):

- `list_content`, `get_content`, `save_content`, `delete_content`, `update_content_status`, `update_content`

Media & image (8):

- `list_media`, `import_media`, `delete_media`, `media_location`
- `get_image_config`, `set_image_config`, `pick_image_folder`, `generate_image`

Providers (7):

- `list_providers`, `get_active_provider`, `create_provider`, `update_provider`, `delete_provider`, `fetch_provider_models`, `set_active_provider`

Publish & schedule (22):

- `list_schedule`, `get_schedule`, `create_schedule`, `delete_schedule`, `update_schedule_status`, `reschedule`, `retry_schedule`
- `get_scheduler_config`, `set_scheduler_config`, `select_scheduler_page`, `start_scheduler`, `stop_scheduler`
- `list_auto_schedule`, `get_auto_schedule`, `create_auto_schedule`, `update_auto_schedule`, `delete_auto_schedule`, `toggle_auto_schedule`
- `get_auto_scheduler_config`, `set_auto_scheduler_config`, `start_auto_scheduler_cmd`, `stop_auto_scheduler_cmd`

Graph (7):

- `validate_facebook_token`, `list_facebook_pages`, `publish_content`, `list_page_posts`, `get_post_insights`, `get_page_report`, `get_page_report_range`

Security & AI & styles & topics (20):

- Security: `set_facebook_token`, `delete_facebook_token`, `credential_status`
- AI: `generate_content`
- Styles: `list_styles`, `create_style`, `update_style`, `duplicate_style`, `delete_style`, `reset_style`
- Image styles: `list_image_styles`, `create_image_style`, `update_image_style`, `duplicate_image_style`, `delete_image_style`, `reset_image_style`
- Topics: `list_topics`, `create_topic`, `update_topic`, `delete_topic`

Reports (1):

- `save_report_csv`

> Lưu ý: `start_auto_scheduler_cmd` / `stop_auto_scheduler_cmd` được đăng ký với `#[tauri::command(rename = "start_auto_scheduler")]` / `#[tauri::command(rename = "stop_auto_scheduler")]`, nên tên IPC thực tế mà frontend gọi là `start_auto_scheduler` / `stop_auto_scheduler`.

## Vị trí dữ liệu

Tất cả nằm dưới `app_data_dir()` của hệ điều hành (trên Windows thường là `%APPDATA%\vn.flowpost.desktop`), không nằm trong thư mục cài đặt:

- `app_data_dir()/content/content-index.json` — kho nội dung (bài viết, prompt, style, `mediaIds`, trạng thái).
- `app_data_dir()/media/` — thư mục media chứa ảnh/video đã tải lên và ảnh do AI tạo; `media-index.json` nằm cùng thư mục.
- `app_data_dir()/schedule/schedule-index.json` — lịch đăng thủ công; `schedule-config.json` cho cấu hình scheduler.
- `app_data_dir()/image-config.json`, `image_styles.json`, `styles.json`, `topics.json` — cấu hình ảnh, phong cách ảnh, phong cách viết, chủ đề.
- `app_data_dir()/secure-credentials.json` — fallback plaintext cho token Facebook và API key provider khi máy không có secure store. Cơ chế khớp `README.md` mục `Lưu trữ cục bộ` và `SECURITY.md`: bản fallback chỉ bị xoá sau khi keyring ghi và đọc lại được giá trị vừa ghi.

## Lệnh build & test

Chạy tại root repo trừ khi ghi chú khác:

- `npm exec -- eslint .` — lint toàn bộ frontend/e2e.
- `npm exec -- vite build` — build frontend ra `dist/`.
- `npm exec -- tauri build` — build bộ cài Windows (MSI + NSIS) trong `src-tauri/target/release/bundle/`. Phải chạy trên Windows.
- `npm exec -- playwright test --config=playwright.e2e-mock.config.ts` — E2E chế độ mock. **Cần Vite dev server đang chạy ở `http://127.0.0.1:1420`** (ví dụ `npm run dev`) vì khối `webServer` trong `playwright.config.ts` đang bị comment.
- `cargo test --lib` — chạy trong `src-tauri` (unit test Rust).

Ghi chú môi trường Windows: trên máy Windows dùng để build bản này, `cargo test --lib` từng lỗi `0xc0000139 STATUS_ENTRYPOINT_NOT_FOUND` do test harness thiếu manifest comctl32. Workaround đã dùng là nhúng manifest v6 vào exe test trong `src-tauri/target/debug/deps/` trước khi chạy lại. Cách này chỉ ảnh hưởng thư mục `target/` (đã gitignore), không thay đổi file nào trong repo, và cần làm lại sau khi `cargo` build lại test harness.

## Cấu trúc repo

```
FlowPostAI/
├─ src/                     # frontend React
│  ├─ App.jsx               # monolith UI (~2.985 dòng)
│  ├─ main.jsx
│  ├─ tauri.js              # wrapper invokeDesktop/isDesktop/localAssetUrl
│  ├─ providerPresets.js
│  ├─ reportCsv.js
│  └─ styles.css
├─ src-tauri/               # backend Rust + cấu hình Tauri
│  ├─ src/                  # 13 file .rs (xem "Điểm vào")
│  ├─ capabilities/
│  ├─ icons/
│  ├─ gen/
│  ├─ target/               # output build (gitignored)
│  ├─ build.rs
│  ├─ Cargo.toml / Cargo.lock
│  └─ tauri.conf.json
├─ e2e/                     # Playwright specs + helpers/ + README.md
├─ docs/                    # tài liệu phát hành & bàn giao
│  ├─ RELEASE_NOTES_0.4.0.md
│  └─ HANDOVER.md
├─ .github/workflows/       # ci.yml
├─ CHANGELOG.md
├─ README.md
├─ SECURITY.md
├─ SECURITY_AUDIT_FACEBOOK_GRAPH.md
├─ package.json / package-lock.json
├─ vite.config.js
├─ eslint.config.js
├─ playwright.config.ts
├─ playwright.e2e-mock.config.ts
├─ index.html
└─ LICENSE
```

## Chính sách nhánh

- Phát triển trên nhánh `dev`.
- Phát hành bằng cách đưa `dev` vào `main`; không squash.
  - Nếu `main` là tổ tiên của `dev` (`git merge-base --is-ancestor main dev` trả mã 0) → `git merge --ff-only dev` (lịch sử tuyến tính, không merge commit).
  - Nếu lịch sử đã phân kỳ → dùng merge commit: `git merge --no-ff dev -m "merge: dev -> main — vX.Y.Z release"`; kiểm tra xung đột trước bằng `git merge-tree --write-tree main dev` (trả mã 0 = không xung đột).
  - Sau khi phát hành, đồng bộ lại `dev`: `git checkout dev && git merge --ff-only main`.
  - Bản 0.4.0 đã dùng merge commit `28ebd4b` vì `main` có 8 commit không nằm trên `dev`.
- Build production từ `main`; tag/version phải khớp `package.json`, `src-tauri/Cargo.toml` và `src-tauri/tauri.conf.json`.

## Khoảng trống đã biết

1. Bộ cài chưa ký số → Windows SmartScreen có thể cảnh báo khi chạy lần đầu.
2. Không có auto-updater; nâng cấp bằng cách cài đè bộ cài mới.
3. E2E chỉ chạy ở chế độ mock (`playwright.e2e-mock.config.ts`), chưa có test với Facebook Graph thật.
4. Guard giới hạn 10 ảnh trong `Kho nội dung` chưa có test tự động.
5. Bài có prompt rỗng bị chặn khi tạo ảnh AI từ `Kho nội dung` (phải bấm `Sửa` để thêm prompt).
6. `secure-credentials.json` là fallback plaintext khi máy không có secure store.
7. Chưa test tạo ảnh với provider ảnh thật trên bản Tauri desktop.

Ngoài ra: `e2e/README.md` chỉ mô tả đường real-server (`tauri-driver`, biến môi trường `E2E_API_KEY`) và chưa có hướng dẫn chạy `playwright.e2e-mock.config.ts` (mock mode, không cần server thật). Người tiếp nhận nên bổ sung mục mock vào tài liệu đó.

## Cách thêm một IPC command

1. Viết hàm `#[tauri::command]` trong module phù hợp ở `src-tauri/src/` (ví dụ thêm vào `content.rs`), trả `Result<T, String>` và nhận `AppHandle` nếu cần đường dẫn dữ liệu.
2. Đăng ký hàm vào `tauri::generate_handler![...]` trong `src-tauri/src/lib.rs` (giữ nguyên tên hàm, hoặc dùng `#[tauri::command(rename = "...")]` nếu muốn tên IPC khác tên hàm).
3. Gọi từ frontend bằng `invokeDesktop('tên_command', { ...payload })` trong `src/App.jsx` và bọc `try/catch` để hiển thị lỗi cho người dùng (theo pattern hiện có: `setErr`/`.error-box`).
